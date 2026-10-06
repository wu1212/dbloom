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
package org.apache.seatunnel.connectors.seatunnel.http.client;

import org.apache.seatunnel.common.utils.JsonUtils;
import org.apache.seatunnel.connectors.seatunnel.http.config.HttpParameter;

import org.apache.http.Header;
import org.apache.http.client.methods.HttpPost;
import org.apache.http.message.BasicHeader;

import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

import java.util.HashMap;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertNotNull;

class HttpClientProviderTest {

    @Test
    void testAddDefaultJsonContentTypeWhenNotPresent() throws Exception {
        HttpPost mockRequest = new HttpPost("http://localhost:8080");
        Map<String, Object> body = new HashMap<>();
        body.put("key", "value");

        HttpClientProvider.addBody(mockRequest, body);

        // case 1: user not define content-type, use default content type
        assertNotNull(mockRequest.getFirstHeader("Content-Type"));
        Assertions.assertEquals(
                "application/json", mockRequest.getFirstHeader("Content-Type").getValue());
    }

    @Test
    void testPreserveExistingContentType() throws Exception {
        HttpPost mockRequest = new HttpPost("http://localhost:8080");
        mockRequest.addHeader(new BasicHeader("Content-Type", "text/plain"));

        Map<String, Object> body = new HashMap<>();
        body.put("key", "value");

        HttpClientProvider.addBody(mockRequest, body);

        // case 2: if user define content-type, set it
        assertNotNull(mockRequest.getFirstHeader("Content-Type"));
        Assertions.assertEquals(
                "text/plain", mockRequest.getFirstHeader("Content-Type").getValue());
    }

    @Test
    void addBody() throws Exception {
        HttpPost post = new HttpPost("http://localhost:8080");
        Map<String, Object> body = new HashMap<>();
        Header[] originalHeaders = post.getAllHeaders();
        HttpClientProvider.addBody(post, body);

        // ensure the original headers are preserved
        Header[] currentHeaders = post.getAllHeaders();
        Assertions.assertEquals(0, originalHeaders.length);
        Assertions.assertEquals(1, currentHeaders.length);
        for (int i = 0; i < originalHeaders.length; i++) {
            Assertions.assertEquals(
                    originalHeaders[i].getName(),
                    currentHeaders[i].getName(),
                    "Header name mismatch at index " + i);
            Assertions.assertEquals(
                    originalHeaders[i].getValue(),
                    currentHeaders[i].getValue(),
                    "Header value mismatch at index " + i);
        }
        // ensure no manually set content type or encoding
        Assertions.assertNull(post.getEntity().getContentEncoding());
    }

    @Test
    void testExecutePreservesNestedJsonBody() throws Exception {
        RecordingHttpClientProvider provider = new RecordingHttpClientProvider();
        Map<String, String> headers = new HashMap<>();
        Map<String, String> params = new HashMap<>();
        String body =
                "{\"ServiceType\":\"HIS1001\",\"ApplyData\":{\"pageNo\":\"1\",\"pageSize\":\"50\"}}";

        provider.execute("http://localhost:8080", "POST", headers, params, body, false);

        Assertions.assertEquals(body, provider.recordedStringBody);
        Assertions.assertNull(provider.recordedMapBody);
    }

    private static final class RecordingHttpClientProvider extends HttpClientProvider {
        private String recordedStringBody;
        private Map<String, Object> recordedMapBody;

        private RecordingHttpClientProvider() {
            super(new HttpParameter());
        }

        //        @Override
        //        public HttpResponse doPost(
        //                String url, Map<String, String> headers, Map<String, String> params,
        // String body) {
        //            this.recordedStringBody = body;
        //            return new HttpResponse(HttpResponse.STATUS_OK, body);
        //        }

        @Override
        public HttpResponse doPost(
                String url,
                Map<String, String> headers,
                Map<String, String> params,
                Map<String, Object> body) {
            this.recordedMapBody = body;
            return new HttpResponse(HttpResponse.STATUS_OK, JsonUtils.toJsonString(body));
        }
    }
}
