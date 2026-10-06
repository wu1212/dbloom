/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package org.apache.seatunnel.engine.server.rest.servlet;

import org.apache.seatunnel.engine.common.config.SeaTunnelConfig;
import org.apache.seatunnel.engine.server.rest.service.UploadFileService;

import org.apache.commons.lang3.StringUtils;

import com.hazelcast.internal.json.JsonArray;
import com.hazelcast.internal.json.JsonObject;
import com.hazelcast.spi.impl.NodeEngineImpl;
import lombok.extern.slf4j.Slf4j;

import javax.servlet.ServletException;
import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;
import javax.servlet.http.Part;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;

/**
 * Upload files used as local file sources or custom jar packages. Files are stored in shared
 * storage directories; in a multi-replica deployment the same volume must be mounted on every
 * replica so that each node reads the uploaded file locally. Supports listing uploaded files (GET),
 * uploading files (POST) and deleting an uploaded file (DELETE).
 */
@Slf4j
public class UploadFileServlet extends BaseServlet {

    public static final String FILE_TYPE_PARAM = "fileType";

    public static final String FILE_NAME_PARAM = "fileName";

    /**
     * When set to {@code true} the DELETE request removes the directory named by {@code fileName}
     * together with every file under it, instead of a single file.
     */
    public static final String IS_DIR_PARAM = "isDir";

    /**
     * Optional target path relative to the storage directory, including the file name, e.g. {@code
     * data_source/123123/aaa.csv}; missing directories are created automatically. When omitted, the
     * file name (possibly with a relative path) submitted in the multipart request is used.
     */
    public static final String RELATIVE_PATH_PARAM = "relativePath";

    private final UploadFileService uploadFileService;

    public UploadFileServlet(NodeEngineImpl nodeEngine, SeaTunnelConfig seaTunnelConfig) {
        super(nodeEngine);
        this.uploadFileService = new UploadFileService(nodeEngine, seaTunnelConfig);
    }

    @Override
    protected void doPost(HttpServletRequest req, HttpServletResponse resp)
            throws ServletException, IOException {

        String fileTypeParam = req.getParameter(FILE_TYPE_PARAM);
        String relativePath = req.getParameter(RELATIVE_PATH_PARAM);
        Collection<Part> parts = req.getParts();
        List<Part> fileParts = new ArrayList<>();
        for (Part part : parts) {
            if (StringUtils.isNotBlank(part.getSubmittedFileName())) {
                fileParts.add(part);
            }
        }
        if (fileParts.isEmpty()) {
            throw new IllegalArgumentException(
                    "No file found in the request, please upload a file with multipart/form-data.");
        }
        if (StringUtils.isNotBlank(relativePath) && fileParts.size() > 1) {
            throw new IllegalArgumentException(
                    "Only one file can be uploaded when the relativePath parameter is specified.");
        }
        JsonArray results = new JsonArray();
        for (Part part : fileParts) {
            UploadFileService.FileType fileType =
                    uploadFileService.resolveFileType(part.getSubmittedFileName(), fileTypeParam);
            results.add(uploadFileService.uploadFile(part, fileType, relativePath));
        }
        writeJson(resp, results);
    }

    @Override
    protected void doGet(HttpServletRequest req, HttpServletResponse resp) throws IOException {
        writeJson(resp, uploadFileService.listFiles());
    }

    /**
     * Delete an uploaded file or jar by its relative path, e.g. {@code
     * fileName=data_source/123123/aaa.csv&fileType=file}. With {@code isDir=true} the given path is
     * treated as a directory and deleted recursively, e.g. {@code
     * fileName=data_source/123123&fileType=file&isDir=true}.
     */
    @Override
    protected void doDelete(HttpServletRequest req, HttpServletResponse resp) throws IOException {
        String fileName = req.getParameter(FILE_NAME_PARAM);
        if (StringUtils.isBlank(fileName)) {
            throw new IllegalArgumentException("The fileName parameter is required.");
        }
        UploadFileService.FileType fileType =
                uploadFileService.resolveFileType(fileName, req.getParameter(FILE_TYPE_PARAM));
        boolean isDir = Boolean.parseBoolean(req.getParameter(IS_DIR_PARAM));
        boolean deleted =
                isDir
                        ? uploadFileService.deleteDir(fileType, fileName)
                        : uploadFileService.deleteFile(fileType, fileName);
        if (!deleted) {
            throw new IllegalArgumentException("File not found: " + fileName);
        }
        writeJson(
                resp,
                new JsonObject()
                        .add("fileName", fileName)
                        .add("fileType", fileType.name().toLowerCase(Locale.ROOT))
                        .add("isDir", isDir)
                        .add("deleted", true));
    }
}
