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

import org.apache.seatunnel.shade.com.fasterxml.jackson.databind.node.ArrayNode;

import org.apache.seatunnel.common.utils.FileUtils;
import org.apache.seatunnel.common.utils.JsonUtils;
import org.apache.seatunnel.engine.common.config.server.HttpConfig;
import org.apache.seatunnel.engine.server.SeaTunnelServer;

import org.apache.commons.lang3.StringUtils;

import com.hazelcast.internal.json.JsonArray;
import com.hazelcast.internal.json.JsonObject;
import com.hazelcast.internal.json.JsonValue;
import com.hazelcast.spi.impl.NodeEngineImpl;
import lombok.extern.slf4j.Slf4j;
import scala.Tuple3;

import java.io.File;
import java.io.UnsupportedEncodingException;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Collectors;

import static org.apache.seatunnel.engine.server.rest.RestConstant.REST_URL_GET_ALL_LOG_NAME;
import static org.apache.seatunnel.engine.server.rest.RestConstant.REST_URL_LOG_PROXY;

@Slf4j
public class LogService extends BaseLogService {
    public LogService(NodeEngineImpl nodeEngine) {
        super(nodeEngine);
    }

    public List<String> allLogName() {
        String logPath = getLogPath();
        List<File> logFileList = FileUtils.listFile(logPath);
        if (logFileList == null) {
            return null;
        }
        return logFileList.stream().map(File::getName).collect(Collectors.toList());
    }

    public List<Tuple3<String, String, String>> allLogNameList(String jobId) {

        SeaTunnelServer seaTunnelServer = getSeaTunnelServer(false);
        HttpConfig httpConfig =
                seaTunnelServer.getSeaTunnelConfig().getEngineConfig().getHttpConfig();
        String contextPath = httpConfig.getContextPath();
        int port = httpConfig.getPort();

        List<Tuple3<String, String, String>> allLogNameList = new ArrayList<>();

        JsonArray systemMonitoringInformationJsonValues =
                getSystemMonitoringInformationJsonValues();
        systemMonitoringInformationJsonValues.forEach(
                systemMonitoringInformation -> {
                    JsonObject systemMonitoringInformationObject =
                            systemMonitoringInformation.asObject();
                    String host = systemMonitoringInformationObject.get("host").asString();
                    // The http port of this node's Jetty service, reported by each node itself.
                    // Fall back to the master's configured http port when the node does not
                    // report it (e.g. it is an older version).
                    JsonValue httpPortValue = systemMonitoringInformationObject.get("httpPort");
                    int nodeHttpPort = port;
                    if (httpPortValue != null && httpPortValue.isString()) {
                        try {
                            nodeHttpPort = Integer.parseInt(httpPortValue.asString());
                        } catch (NumberFormatException e) {
                            log.warn(
                                    "Failed to parse httpPort [{}] from node [{}], fall back to [{}].",
                                    httpPortValue.asString(),
                                    host,
                                    port);
                        }
                    }
                    // The internal address (host:httpPort) reachable inside the cluster network,
                    // used for node-to-node calls and as the target node of the log proxy link.
                    String internalAddress = host + ":" + nodeHttpPort;
                    String url = "http://" + internalAddress + contextPath;
                    String allName = sendGet(url + REST_URL_GET_ALL_LOG_NAME);
                    log.debug(String.format("Request: %s , Result: %s", url, allName));
                    if (StringUtils.isBlank(allName)) {
                        log.warn(
                                "Failed to get log names from node [{}], skip it.",
                                internalAddress);
                        return;
                    }
                    ArrayNode jsonNodes;
                    try {
                        jsonNodes = JsonUtils.parseArray(allName);
                    } catch (Exception e) {
                        log.warn(
                                "Failed to parse log names from node [{}], response: [{}], skip it.",
                                internalAddress,
                                allName);
                        return;
                    }

                    jsonNodes.forEach(
                            jsonNode -> {
                                String fileName = jsonNode.asText();
                                if (StringUtils.isNotBlank(jobId) && !fileName.contains(jobId)) {
                                    return;
                                }
                                // The log link points to the log proxy of the cluster entry
                                // (relative path), so the browser does not need to reach each
                                // node directly, which works for both docker compose and k8s.
                                allLogNameList.add(
                                        new Tuple3<>(
                                                internalAddress,
                                                contextPath
                                                        + REST_URL_LOG_PROXY
                                                        + "?node="
                                                        + encodeParam(internalAddress)
                                                        + "&file="
                                                        + encodeParam(fileName),
                                                fileName));
                            });
                });

        return allLogNameList;
    }

    private String encodeParam(String value) {
        try {
            return URLEncoder.encode(value, StandardCharsets.UTF_8.name());
        } catch (UnsupportedEncodingException e) {
            // UTF-8 is always supported
            return value;
        }
    }

    public JsonArray allNodeLogFormatJson(String jobId) {

        return allLogNameList(jobId).stream()
                .map(
                        tuple -> {
                            JsonObject jsonObject = new JsonObject();
                            jsonObject.add("node", tuple._1());
                            jsonObject.add("logLink", tuple._2());
                            jsonObject.add("logName", tuple._3());
                            return jsonObject;
                        })
                .collect(JsonArray::new, JsonArray::add, JsonArray::add);
    }

    public String allNodeLogFormatHtml(String jobId) {
        StringBuffer logLink = new StringBuffer();

        allLogNameList(jobId)
                .forEach(tuple -> logLink.append(buildLogLink(tuple._2(), tuple._3())));
        return buildWebSiteContent(logLink);
    }

    public String currentNodeLog(String uri) {
        List<File> logFileList = FileUtils.listFile(getLogPath());
        StringBuffer logLink = new StringBuffer();
        if (logFileList != null) {
            for (File file : logFileList) {
                logLink.append(buildLogLink("log/" + file.getName(), file.getName()));
            }
        }

        return buildWebSiteContent(logLink);
    }
}
