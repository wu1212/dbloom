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

package org.apache.seatunnel.engine.common.config.server;

import lombok.Data;

import java.util.ArrayList;
import java.util.List;

@Data
public class UploadFileConfig {

    /**
     * The storage path of uploaded data files which are used as source or sink. Keep the default
     * value as a literal instead of referencing {@link
     * ServerConfigOptions.MasterServerConfigOptions#UPLOAD_FILE_PATH}, because this class is
     * instantiated as the default value of UPLOAD_FILE_CONFIG during the static initialization of
     * MasterServerConfigOptions, where the options below are not initialized yet and would cause a
     * NullPointerException.
     */
    private String path = "";

    /** The storage path of uploaded custom jar packages. */
    private String jarPath = "";

    /** The retention days of uploaded data files. */
    private int retentionDays = 7;

    /** The interval (in minutes) of the scheduled cleanup task for uploaded data files. */
    private int cleanupInterval = 60;

    /**
     * The directories scanned by the scheduled cleanup task. Keep the default value as a literal
     * for the same reason as {@link #path}. When empty, the cleanup task only scans {@link #path}.
     */
    private List<String> cleanupPaths = new ArrayList<>();
}
