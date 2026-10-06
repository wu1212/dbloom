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

import com.hazelcast.internal.json.JsonObject;
import com.hazelcast.spi.impl.NodeEngineImpl;
import lombok.extern.slf4j.Slf4j;

import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;

import java.io.IOException;
import java.util.Locale;

/**
 * Check whether a file uploaded through the {@code /upload-file} API exists. The check is performed
 * against the shared storage directories, so any replica can answer it.
 */
@Slf4j
public class FileExistsServlet extends BaseServlet {

    public static final String FILE_NAME_PARAM = "fileName";
    public static final String FILE_TYPE_PARAM = "fileType";

    private final UploadFileService uploadFileService;

    public FileExistsServlet(NodeEngineImpl nodeEngine, SeaTunnelConfig seaTunnelConfig) {
        super(nodeEngine);
        this.uploadFileService = new UploadFileService(nodeEngine, seaTunnelConfig);
    }

    @Override
    protected void doGet(HttpServletRequest req, HttpServletResponse resp) throws IOException {
        String fileName = req.getParameter(FILE_NAME_PARAM);
        if (StringUtils.isBlank(fileName)) {
            throw new IllegalArgumentException("The fileName parameter is required.");
        }
        UploadFileService.FileType fileType =
                uploadFileService.resolveFileType(fileName, req.getParameter(FILE_TYPE_PARAM));
        boolean exists = uploadFileService.existsFile(fileType, fileName);
        writeJson(
                resp,
                new JsonObject()
                        .add("fileName", fileName)
                        .add("fileType", fileType.name().toLowerCase(Locale.ROOT))
                        .add("exists", exists));
    }
}
