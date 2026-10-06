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

import org.apache.commons.lang3.StringUtils;

import com.hazelcast.spi.impl.NodeEngineImpl;
import lombok.extern.slf4j.Slf4j;

import javax.servlet.http.HttpServletResponse;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;

@Slf4j
public class LogBaseServlet extends BaseServlet {

    public LogBaseServlet(NodeEngineImpl nodeEngine) {
        super(nodeEngine);
    }
    /** Prepare Log Response */
    protected void prepareLogResponse(HttpServletResponse resp, String logPath, String logName) {
        if (StringUtils.isBlank(logPath)) {
            resp.setStatus(HttpServletResponse.SC_BAD_REQUEST);
            log.warn(
                    "Log file path is empty, no log file path configured in the current configuration file");
            return;
        }
        if (!isValidLogName(logName)) {
            resp.setStatus(HttpServletResponse.SC_BAD_REQUEST);
            log.warn(String.format("Invalid log file name: %s", logName));
            return;
        }
        File logFile = new File(logPath, logName);
        if (!logFile.isFile()) {
            // If the log file does not exist, return 400
            resp.setStatus(HttpServletResponse.SC_BAD_REQUEST);
            log.warn(
                    String.format("Log file does not exist, get log path : %s", logFile.getPath()));
            return;
        }
        // Stream the raw bytes of the log file to the client instead of loading the whole file
        // into memory, so that large log files can be displayed completely.
        resp.setContentType("text/plain; charset=UTF-8");
        try (InputStream in = new FileInputStream(logFile);
                OutputStream out = resp.getOutputStream()) {
            byte[] buffer = new byte[8192];
            int len;
            while ((len = in.read(buffer)) != -1) {
                out.write(buffer, 0, len);
            }
        } catch (IOException e) {
            resp.setStatus(HttpServletResponse.SC_BAD_REQUEST);
            log.warn(String.format("Read log file failed, get log path : %s", logFile.getPath()));
        }
    }

    /** The log name must be a plain file name without any path separator or traversal. */
    protected static boolean isValidLogName(String logName) {
        return StringUtils.isNotBlank(logName)
                && !logName.contains("/")
                && !logName.contains("\\")
                && !logName.contains("..");
    }
}
