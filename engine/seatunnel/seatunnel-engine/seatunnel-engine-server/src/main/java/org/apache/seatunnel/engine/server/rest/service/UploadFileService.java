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

import org.apache.seatunnel.common.config.Common;
import org.apache.seatunnel.engine.common.config.SeaTunnelConfig;
import org.apache.seatunnel.engine.common.config.server.UploadFileConfig;

import org.apache.commons.lang3.StringUtils;

import com.hazelcast.internal.json.JsonArray;
import com.hazelcast.internal.json.JsonObject;
import com.hazelcast.spi.impl.NodeEngineImpl;
import lombok.extern.slf4j.Slf4j;

import javax.servlet.http.Part;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.DirectoryNotEmptyException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import java.util.Optional;
import java.util.Timer;
import java.util.stream.Collectors;
import java.util.stream.Stream;

/**
 * Upload files to a shared storage directory so that in a multi-replica deployment every node can
 * read the uploaded file as a local file source.
 *
 * <p>Uploaded files are classified into two types: data files used as source/sink and custom jar
 * packages, which are stored in different directories. The storage directory of each type can be
 * configured by system property or environment variable ({@code SEATUNNEL_UPLOAD_FILE_PATH} for
 * data files, {@code SEATUNNEL_UPLOAD_JAR_PATH} for jars, with the highest priority), by the {@code
 * upload-file} section in seatunnel.yaml, and defaults to {@code
 * ${SEATUNNEL_HOME}/data/upload-files} and {@code ${SEATUNNEL_HOME}/data/upload-jars}. In Docker /
 * Docker Compose / Kubernetes deployments, mount the same shared volume to these directories on
 * every replica.
 *
 * <p>Data files are not kept permanently: a scheduled task scans the configured cleanup directories
 * (the {@code cleanup-paths} option or the {@code SEATUNNEL_UPLOAD_CLEANUP_PATHS} system property /
 * environment variable, comma separated) and deletes files older than the configured retention
 * period. When no cleanup directory is configured, the storage directory of uploaded data files is
 * cleaned up by default; the jar directory is never cleaned automatically.
 */
@Slf4j
public class UploadFileService extends BaseService {

    public static final String UPLOAD_FILE_PATH_CONFIG = "SEATUNNEL_UPLOAD_FILE_PATH";
    public static final String UPLOAD_JAR_PATH_CONFIG = "SEATUNNEL_UPLOAD_JAR_PATH";
    public static final String UPLOAD_CLEANUP_PATHS_CONFIG = "SEATUNNEL_UPLOAD_CLEANUP_PATHS";

    private static final String DEFAULT_FILE_DIR = "upload-files";
    private static final String DEFAULT_JAR_DIR = "upload-jars";

    /** Marker of temp files which are being written and not yet visible as uploaded files. */
    private static final String TEMP_FILE_MARKER = ".uploading.";

    private final UploadFileConfig uploadFileConfig;
    private final Timer cleanupTimer;

    public UploadFileService(NodeEngineImpl nodeEngine, SeaTunnelConfig seaTunnelConfig) {
        super(nodeEngine);
        this.uploadFileConfig = seaTunnelConfig.getEngineConfig().getUploadFileConfig();
        long cleanupIntervalMs = uploadFileConfig.getCleanupInterval() * 60_000L;
        long retentionMillis = uploadFileConfig.getRetentionDays() * 24L * 60 * 60 * 1000;
        this.cleanupTimer = new Timer("seatunnel-upload-file-cleanup", true);
        this.cleanupTimer.schedule(
                new UploadFileCleanupTask(
                        () -> {
                            try {
                                return getCleanupDirs();
                            } catch (IOException e) {
                                throw new RuntimeException(
                                        "Failed to get the cleanup directories", e);
                            }
                        },
                        retentionMillis),
                cleanupIntervalMs,
                cleanupIntervalMs);
    }

    /**
     * The directories scanned by the scheduled cleanup task. They can be configured by system
     * property or environment variable ({@code SEATUNNEL_UPLOAD_CLEANUP_PATHS}, comma separated,
     * with the highest priority) or by the {@code cleanup-paths} option of the {@code upload-file}
     * section; when not configured, defaults to the storage directory of uploaded data files.
     */
    private List<Path> getCleanupDirs() throws IOException {
        String configuredPaths = System.getProperty(UPLOAD_CLEANUP_PATHS_CONFIG);
        if (StringUtils.isBlank(configuredPaths)) {
            configuredPaths = System.getenv(UPLOAD_CLEANUP_PATHS_CONFIG);
        }
        List<String> paths = splitCommaSeparated(configuredPaths);
        if (paths.isEmpty() && uploadFileConfig.getCleanupPaths() != null) {
            for (String path : uploadFileConfig.getCleanupPaths()) {
                if (StringUtils.isNotBlank(path)) {
                    paths.add(path.trim());
                }
            }
        }
        if (paths.isEmpty()) {
            return Collections.singletonList(getUploadDir(FileType.FILE));
        }
        List<Path> dirs = new ArrayList<>();
        for (String path : paths) {
            Path dir = Paths.get(path);
            if (!Files.exists(dir)) {
                Files.createDirectories(dir);
            }
            dirs.add(dir);
        }
        return dirs;
    }

    private List<String> splitCommaSeparated(String value) {
        List<String> values = new ArrayList<>();
        if (StringUtils.isBlank(value)) {
            return values;
        }
        for (String part : value.split(",")) {
            if (StringUtils.isNotBlank(part)) {
                values.add(part.trim());
            }
        }
        return values;
    }

    public JsonObject uploadFile(Part filePart, FileType fileType) throws IOException {
        return uploadFile(filePart, fileType, null);
    }

    /**
     * Upload a file to the storage directory of the given file type. {@code relativePath} is an
     * optional target path relative to the storage directory, including the file name, e.g. {@code
     * data_source/123123/aaa.csv}; missing directories are created automatically. When it is not
     * given, the relative path carried in the multipart file name itself is used.
     */
    public JsonObject uploadFile(Part filePart, FileType fileType, String relativePath)
            throws IOException {
        String targetRelativePath =
                StringUtils.isNotBlank(relativePath)
                        ? sanitizeRelativePath(relativePath)
                        : sanitizeRelativePath(filePart.getSubmittedFileName());
        Path uploadDir = getUploadDir(fileType);
        Path targetPath = resolveInsideUploadDir(uploadDir, targetRelativePath);
        Files.createDirectories(targetPath.getParent());

        // Write to a temp file first and then move it atomically, so that other nodes
        // reading from the shared volume will never see a partially written file.
        Path tempPath =
                targetPath.resolveSibling(
                        targetPath.getFileName()
                                + TEMP_FILE_MARKER
                                + Thread.currentThread().getId());
        try (InputStream inputStream = filePart.getInputStream()) {
            Files.copy(inputStream, tempPath, StandardCopyOption.REPLACE_EXISTING);
            try {
                Files.move(
                        tempPath,
                        targetPath,
                        StandardCopyOption.REPLACE_EXISTING,
                        StandardCopyOption.ATOMIC_MOVE);
            } catch (AtomicMoveNotSupportedException e) {
                Files.move(tempPath, targetPath, StandardCopyOption.REPLACE_EXISTING);
            }
        } catch (IOException e) {
            Files.deleteIfExists(tempPath);
            throw e;
        }

        log.info(
                "Uploaded {} [{}] to [{}]",
                fileType.name().toLowerCase(Locale.ROOT),
                targetRelativePath,
                targetPath);
        return new JsonObject()
                .add("fileName", targetRelativePath)
                .add("filePath", targetPath.toString())
                .add("fileType", fileType.name().toLowerCase(Locale.ROOT))
                .add("fileSize", Files.size(targetPath));
    }

    public JsonArray listFiles() throws IOException {
        JsonArray files = new JsonArray();
        addFilesToResult(files, FileType.FILE);
        addFilesToResult(files, FileType.JAR);
        return files;
    }

    public Optional<Path> getFile(FileType fileType, String fileName) throws IOException {
        Path filePath =
                resolveInsideUploadDir(getUploadDir(fileType), sanitizeRelativePath(fileName));
        if (Files.isRegularFile(filePath)) {
            return Optional.of(filePath);
        }
        return Optional.empty();
    }

    /**
     * Check whether an uploaded file or jar exists by its path relative to the storage directory.
     */
    public boolean existsFile(FileType fileType, String fileName) throws IOException {
        Path filePath =
                resolveInsideUploadDir(getUploadDir(fileType), sanitizeRelativePath(fileName));
        return Files.isRegularFile(filePath);
    }

    public FileType resolveFileType(String fileName, String fileTypeParam) {
        if ("jar".equalsIgnoreCase(fileTypeParam)) {
            return FileType.JAR;
        }
        if ("file".equalsIgnoreCase(fileTypeParam)) {
            return FileType.FILE;
        }
        if (fileName != null && fileName.toLowerCase(Locale.ROOT).endsWith(".jar")) {
            return FileType.JAR;
        }
        return FileType.FILE;
    }

    private void addFilesToResult(JsonArray files, FileType fileType) throws IOException {
        Path uploadDir = getUploadDir(fileType);
        try (Stream<Path> stream = Files.walk(uploadDir)) {
            stream.filter(Files::isRegularFile)
                    .filter(path -> !path.getFileName().toString().contains(TEMP_FILE_MARKER))
                    .sorted()
                    .forEach(
                            path -> {
                                try {
                                    String fileName =
                                            uploadDir
                                                    .relativize(path)
                                                    .toString()
                                                    .replace('\\', '/');
                                    files.add(
                                            new JsonObject()
                                                    .add("fileName", fileName)
                                                    .add("filePath", path.toString())
                                                    .add(
                                                            "fileType",
                                                            fileType.name()
                                                                    .toLowerCase(Locale.ROOT))
                                                    .add("fileSize", Files.size(path))
                                                    .add(
                                                            "lastModified",
                                                            Files.getLastModifiedTime(path)
                                                                    .toMillis()));
                                } catch (IOException e) {
                                    log.warn("Failed to read file info of [{}]", path, e);
                                }
                            });
        }
    }

    private Path getUploadDir(FileType fileType) throws IOException {
        String configKey =
                fileType == FileType.JAR ? UPLOAD_JAR_PATH_CONFIG : UPLOAD_FILE_PATH_CONFIG;
        String uploadPath = System.getProperty(configKey);
        if (StringUtils.isBlank(uploadPath)) {
            uploadPath = System.getenv(configKey);
        }
        if (StringUtils.isBlank(uploadPath)) {
            uploadPath =
                    fileType == FileType.JAR
                            ? uploadFileConfig.getJarPath()
                            : uploadFileConfig.getPath();
        }
        if (StringUtils.isBlank(uploadPath)) {
            String defaultDir = fileType == FileType.JAR ? DEFAULT_JAR_DIR : DEFAULT_FILE_DIR;
            uploadPath = Paths.get(Common.getSeaTunnelHome(), "data", defaultDir).toString();
        }
        Path uploadDir = Paths.get(uploadPath);
        if (!Files.exists(uploadDir)) {
            Files.createDirectories(uploadDir);
        }
        return uploadDir;
    }

    /**
     * Delete an uploaded file or jar by its path relative to the storage directory. Returns true if
     * the file existed and was deleted. Parent directories left empty by the deletion are removed
     * as well, keeping the disk state consistent with the file-based folder view of the UI.
     */
    public boolean deleteFile(FileType fileType, String fileName) throws IOException {
        Path uploadDir = getUploadDir(fileType);
        Path filePath = resolveInsideUploadDir(uploadDir, sanitizeRelativePath(fileName));
        if (!Files.isRegularFile(filePath)) {
            return false;
        }
        boolean deleted = Files.deleteIfExists(filePath);
        if (deleted) {
            log.info(
                    "Deleted uploaded {} [{}]", fileType.name().toLowerCase(Locale.ROOT), filePath);
            removeEmptyParentDirs(uploadDir, filePath);
        }
        return deleted;
    }

    /**
     * Delete an uploaded directory (and everything under it) by its path relative to the storage
     * directory. Returns true if the directory existed and was deleted.
     */
    public boolean deleteDir(FileType fileType, String dirName) throws IOException {
        Path uploadDir = getUploadDir(fileType);
        Path dirPath = resolveInsideUploadDir(uploadDir, sanitizeRelativePath(dirName));
        if (!Files.isDirectory(dirPath)) {
            return false;
        }
        // Delete the deepest paths first so that parents become empty before their turn.
        List<Path> subtree;
        try (Stream<Path> stream = Files.walk(dirPath)) {
            subtree = stream.sorted(Comparator.reverseOrder()).collect(Collectors.toList());
        }
        for (Path path : subtree) {
            Files.deleteIfExists(path);
        }
        log.info(
                "Deleted uploaded {} directory [{}]",
                fileType.name().toLowerCase(Locale.ROOT),
                dirPath);
        return true;
    }

    /**
     * Best effort removal of the parent directories left empty after a file deletion, walking up to
     * (but excluding) the storage directory itself.
     */
    private void removeEmptyParentDirs(Path uploadDir, Path deletedFile) {
        Path root = uploadDir.normalize();
        Path parent = deletedFile.getParent();
        while (parent != null && !parent.equals(root) && parent.startsWith(root)) {
            try {
                if (!Files.deleteIfExists(parent)) {
                    break;
                }
                log.info("Removed empty upload directory [{}]", parent);
            } catch (DirectoryNotEmptyException e) {
                // Siblings still live here, keep the directory
                break;
            } catch (IOException e) {
                log.warn("Failed to remove empty upload directory [{}]", parent, e);
                break;
            }
            parent = parent.getParent();
        }
    }

    /**
     * Normalize the given path to a '/'-separated relative path, stripping any absolute prefix and
     * rejecting segments which could escape the upload directory or create hidden entries.
     */
    private String sanitizeRelativePath(String path) {
        if (StringUtils.isBlank(path)) {
            throw new IllegalArgumentException("The uploaded file name cannot be empty.");
        }
        String normalized = path.trim().replace('\\', '/');
        // Strip a windows drive letter prefix such as "C:/fakepath/aaa.csv"
        int colonIndex = normalized.indexOf(':');
        if (colonIndex >= 0 && colonIndex <= 2) {
            normalized = normalized.substring(colonIndex + 1);
        }
        StringBuilder result = new StringBuilder();
        for (String segment : normalized.split("/")) {
            if (segment.isEmpty() || ".".equals(segment)) {
                continue;
            }
            if ("..".equals(segment) || segment.startsWith(".")) {
                throw new IllegalArgumentException("The file path is invalid: " + path);
            }
            if (result.length() > 0) {
                result.append('/');
            }
            result.append(segment);
        }
        if (result.length() == 0) {
            throw new IllegalArgumentException("The file path is invalid: " + path);
        }
        return result.toString();
    }

    private Path resolveInsideUploadDir(Path uploadDir, String relativePath) {
        Path resolved = uploadDir.resolve(relativePath).normalize();
        if (!resolved.startsWith(uploadDir.normalize())) {
            throw new IllegalArgumentException("The file path is invalid: " + relativePath);
        }
        return resolved;
    }

    public enum FileType {
        FILE,
        JAR
    }
}
