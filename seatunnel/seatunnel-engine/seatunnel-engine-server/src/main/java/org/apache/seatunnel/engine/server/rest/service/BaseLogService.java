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

package org.apache.seatunnel.engine.server.rest.service;

import org.apache.seatunnel.common.utils.ExceptionUtils;
import org.apache.seatunnel.engine.common.utils.LogUtil;

import com.hazelcast.internal.util.StringUtil;
import com.hazelcast.spi.impl.NodeEngineImpl;
import lombok.extern.slf4j.Slf4j;

import javax.servlet.http.HttpServletResponse;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;

@Slf4j
public class BaseLogService extends BaseService {
    public BaseLogService(NodeEngineImpl nodeEngine) {
        super(nodeEngine);
    }

    /** Get configuration log path */
    public String getLogPath() {
        try {
            return LogUtil.getLogPath();
        } catch (NoSuchFieldException | IllegalAccessException e) {
            log.error("Get log path error,{}", ExceptionUtils.getMessage(e));
            return null;
        }
    }

    protected String sendGet(String urlString) {
        HttpURLConnection connection = null;
        try {
            connection = (HttpURLConnection) new URL(urlString).openConnection();
            connection.setRequestMethod("GET");
            connection.setConnectTimeout(5000);
            connection.setReadTimeout(5000);
            connection.connect();

            if (connection.getResponseCode() == 200) {
                try (InputStream is = connection.getInputStream();
                        ByteArrayOutputStream baos = new ByteArrayOutputStream()) {
                    byte[] buffer = new byte[1024];
                    int len;
                    while ((len = is.read(buffer)) != -1) {
                        baos.write(buffer, 0, len);
                    }
                    return baos.toString();
                }
            }
            log.warn(
                    "Send get to [{}] failed, response code: {}",
                    urlString,
                    connection.getResponseCode());
        } catch (IOException e) {
            log.error("Send get to [{}] fail.{}", urlString, ExceptionUtils.getMessage(e));
        }
        return null;
    }

    /**
     * Stream the content of the given url to the response without buffering the whole body into
     * memory. Used by the log proxy to forward log content from the target node.
     *
     * @return the upstream response code when the upstream responded, or -1 if the connection
     *     failed
     */
    public int streamGet(String urlString, HttpServletResponse resp) {
        HttpURLConnection connection = null;
        try {
            connection = (HttpURLConnection) new URL(urlString).openConnection();
            connection.setRequestMethod("GET");
            connection.setConnectTimeout(5000);
            connection.setReadTimeout(60000);
            connection.connect();

            int responseCode = connection.getResponseCode();
            if (responseCode != HttpURLConnection.HTTP_OK) {
                log.warn("Stream get to [{}] failed, response code: {}", urlString, responseCode);
                return responseCode;
            }
            resp.setContentType("text/plain; charset=UTF-8");
            try (InputStream is = connection.getInputStream();
                    OutputStream os = resp.getOutputStream()) {
                byte[] buffer = new byte[8192];
                int len;
                while ((len = is.read(buffer)) != -1) {
                    os.write(buffer, 0, len);
                }
            }
            return HttpURLConnection.HTTP_OK;
        } catch (IOException e) {
            log.error("Stream get to [{}] fail.{}", urlString, ExceptionUtils.getMessage(e));
            return -1;
        } finally {
            if (connection != null) {
                connection.disconnect();
            }
        }
    }

    public String getLogParam(String uri, String contextPath) {
        uri = uri.substring(uri.indexOf(contextPath) + contextPath.length());
        uri = StringUtil.stripTrailingSlash(uri).substring(1);
        int indexEnd = uri.indexOf('/');
        if (indexEnd != -1) {
            return uri.substring(indexEnd + 1);
        }
        return "";
    }

    protected String buildLogLink(String href, String name) {
        return "<li><a href=\"" + href + "\">" + name + "</a></li>\n";
    }

    protected String buildWebSiteContent(StringBuffer logLink) {
        return "<html><head><title>Seatunnel log</title></head>\n"
                + "<body>\n"
                + " <h2>Seatunnel log</h2>\n"
                + " <ul>\n"
                + logLink.toString()
                + " </ul>\n"
                + "</body></html>";
    }
}
