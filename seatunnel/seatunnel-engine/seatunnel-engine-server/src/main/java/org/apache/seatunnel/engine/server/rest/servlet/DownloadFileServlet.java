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

import com.hazelcast.spi.impl.NodeEngineImpl;
import lombok.extern.slf4j.Slf4j;

import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;

import java.io.IOException;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Optional;

/**
 * Download files uploaded through the {@code /upload-file} API. Files are served from the shared
 * storage directories, so any replica can download them.
 */
@Slf4j
public class DownloadFileServlet extends BaseServlet {

    public static final String FILE_NAME_PARAM = "fileName";
    public static final String FILE_TYPE_PARAM = "fileType";

    private final UploadFileService uploadFileService;

    public DownloadFileServlet(NodeEngineImpl nodeEngine, SeaTunnelConfig seaTunnelConfig) {
        super(nodeEngine);
        this.uploadFileService = new UploadFileService(nodeEngine, seaTunnelConfig);
    }

    @Override
    protected void doGet(HttpServletRequest req, HttpServletResponse resp) throws IOException {
        String fileName = req.getParameter(FILE_NAME_PARAM);
        UploadFileService.FileType fileType =
                uploadFileService.resolveFileType(fileName, req.getParameter(FILE_TYPE_PARAM));
        Optional<Path> file = uploadFileService.getFile(fileType, fileName);
        if (!file.isPresent()) {
            throw new IllegalArgumentException("File not found: " + fileName);
        }
        Path filePath = file.get();
        resp.setContentType("application/octet-stream");
        String encodedFileName =
                URLEncoder.encode(filePath.getFileName().toString(), StandardCharsets.UTF_8.name())
                        .replace("+", "%20");
        resp.setHeader("Content-Disposition", "attachment; filename*=UTF-8''" + encodedFileName);
        resp.setContentLengthLong(Files.size(filePath));
        Files.copy(filePath, resp.getOutputStream());
        resp.getOutputStream().flush();
    }
}
