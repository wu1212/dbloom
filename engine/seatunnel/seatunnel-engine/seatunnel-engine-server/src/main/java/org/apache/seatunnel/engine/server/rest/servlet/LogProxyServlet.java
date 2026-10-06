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

import org.apache.seatunnel.engine.server.SeaTunnelServer;
import org.apache.seatunnel.engine.server.rest.service.BaseLogService;

import org.apache.commons.lang3.StringUtils;

import com.hazelcast.spi.impl.NodeEngineImpl;
import lombok.extern.slf4j.Slf4j;

import javax.servlet.ServletException;
import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;

import java.io.IOException;
import java.net.HttpURLConnection;
import java.util.Set;
import java.util.stream.Collectors;

import static org.apache.seatunnel.engine.server.rest.RestConstant.REST_URL_LOGS;

/**
 * Proxy the log content of any cluster node through this node. The browser only needs to reach one
 * cluster entry (e.g. the k8s service or the compose port mapping), this servlet fetches the log
 * content from the target node via its internal address and streams it back.
 */
@Slf4j
public class LogProxyServlet extends LogBaseServlet {

    private final BaseLogService baseLogService;

    public LogProxyServlet(NodeEngineImpl nodeEngine) {
        super(nodeEngine);
        this.baseLogService = new BaseLogService(nodeEngine);
    }

    @Override
    protected void doGet(HttpServletRequest req, HttpServletResponse resp)
            throws ServletException, IOException {
        String node = req.getParameter("node");
        String file = req.getParameter("file");
        if (StringUtils.isBlank(node) || !isValidLogName(file)) {
            resp.setStatus(HttpServletResponse.SC_BAD_REQUEST);
            log.warn("Invalid log proxy request, node: [{}], file: [{}]", node, file);
            return;
        }

        // Only allow proxying to nodes that belong to this cluster to prevent SSRF.
        String host = node.contains(":") ? node.substring(0, node.lastIndexOf(':')) : node;
        Set<String> memberHosts =
                nodeEngine.getHazelcastInstance().getCluster().getMembers().stream()
                        .map(member -> member.getAddress().getHost())
                        .collect(Collectors.toSet());
        if (!memberHosts.contains(host)) {
            resp.setStatus(HttpServletResponse.SC_BAD_REQUEST);
            log.warn("Log proxy request to a non-cluster member [{}], reject it.", node);
            return;
        }

        SeaTunnelServer seaTunnelServer = getSeaTunnelServer(false);
        String contextPath =
                seaTunnelServer
                        .getSeaTunnelConfig()
                        .getEngineConfig()
                        .getHttpConfig()
                        .getContextPath();
        String targetUrl = "http://" + node + contextPath + REST_URL_LOGS + "/" + file;
        int responseCode = baseLogService.streamGet(targetUrl, resp);
        if (responseCode != HttpURLConnection.HTTP_OK && !resp.isCommitted()) {
            resp.setStatus(responseCode > 0 ? responseCode : HttpServletResponse.SC_BAD_GATEWAY);
            log.warn(
                    "Failed to proxy log content from [{}], response code: [{}].",
                    targetUrl,
                    responseCode);
        }
    }
}
