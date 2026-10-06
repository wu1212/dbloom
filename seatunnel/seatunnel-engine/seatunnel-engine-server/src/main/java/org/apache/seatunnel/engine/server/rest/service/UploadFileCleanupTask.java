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

import lombok.extern.slf4j.Slf4j;

import java.io.IOException;
import java.nio.file.DirectoryNotEmptyException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Comparator;
import java.util.List;
import java.util.TimerTask;
import java.util.function.Supplier;
import java.util.stream.Stream;

/**
 * Scheduled cleanup task for uploaded data files. Scans each configured directory, deletes files
 * whose last modified time is earlier than the retention period, including files stored in
 * sub-directories, and removes sub-directories left empty afterwards. As the cleanup directories
 * usually point to a shared volume in a multi-replica deployment, the task is idempotent and safe
 * to run on every node.
 */
@Slf4j
public class UploadFileCleanupTask extends TimerTask {

    private final Supplier<List<Path>> cleanupDirSupplier;

    private final long retentionMillis;

    public UploadFileCleanupTask(Supplier<List<Path>> cleanupDirSupplier, long retentionMillis) {
        this.cleanupDirSupplier = cleanupDirSupplier;
        this.retentionMillis = retentionMillis;
    }

    @Override
    public void run() {
        long cutoffTime = System.currentTimeMillis() - retentionMillis;
        try {
            for (Path cleanupDir : cleanupDirSupplier.get()) {
                try {
                    cleanDirectory(cleanupDir, cutoffTime);
                } catch (Exception e) {
                    // Keep scanning the remaining directories
                    log.warn("Failed to clean up expired uploaded files under [{}]", cleanupDir, e);
                }
            }
        } catch (Exception e) {
            // Catch all exceptions to keep the timer thread alive
            log.warn("Failed to clean up expired uploaded files", e);
        }
    }

    private void cleanDirectory(Path uploadDir, long cutoffTime) throws IOException {
        try (Stream<Path> stream = Files.walk(uploadDir)) {
            stream.filter(Files::isRegularFile)
                    .filter(
                            path -> {
                                try {
                                    return Files.getLastModifiedTime(path).toMillis() < cutoffTime;
                                } catch (IOException e) {
                                    log.warn("Failed to read last modified time of [{}]", path, e);
                                    return false;
                                }
                            })
                    .forEach(
                            path -> {
                                try {
                                    Files.deleteIfExists(path);
                                    log.info("Cleaned up expired uploaded file [{}]", path);
                                } catch (IOException e) {
                                    log.warn(
                                            "Failed to delete expired uploaded file [{}]", path, e);
                                }
                            });
        }
        // Best effort: remove sub-directories left empty by the cleanup above. Deleting the
        // deepest paths first keeps parents deletable after their children are removed.
        try (Stream<Path> stream = Files.walk(uploadDir)) {
            stream.filter(Files::isDirectory)
                    .filter(path -> !path.equals(uploadDir))
                    .sorted(Comparator.reverseOrder())
                    .forEach(
                            path -> {
                                try {
                                    if (Files.deleteIfExists(path)) {
                                        log.info("Cleaned up empty upload directory [{}]", path);
                                    }
                                } catch (DirectoryNotEmptyException ignored) {
                                    // A file was written concurrently, keep the directory
                                } catch (IOException e) {
                                    log.warn(
                                            "Failed to delete empty upload directory [{}]",
                                            path,
                                            e);
                                }
                            });
        }
    }
}
