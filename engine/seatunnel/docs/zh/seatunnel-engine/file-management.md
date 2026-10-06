# 文件管理 API

SeaTunnel 提供了一组用于管理共享存储文件的 REST API，支持文件的**上传、列表、下载、删除和判存**。上传的文件分为两类：

- `file`：数据文件，用作作业的 source/sink 数据源；
- `jar`：自定义 jar 包。

两类文件存储在不同目录中。在多副本部署中，所有节点挂载同一共享卷，下载、判存、删除等操作基于共享目录，任意节点均可响应。

## 概述

- **Base URL**：`http://{host}:{port}/hazelcast/rest/maps`。文件管理接口与提交作业、停止作业等接口注册在同一个 Jetty 服务中，共用同一端口（默认 `8080`）；端口与 context-path 可通过 `seatunnel.yaml` 中的 `http` 配置项修改，详见 [RESTful API V2](rest-api-v2.md)。
- **fileType**：取值 `file` 或 `jar`。所有接口中该参数均可省略，省略时按文件名后缀推断：以 `.jar` 结尾视为 `jar`，否则视为 `file`。
- **fileName / relativePath**：均为相对存储目录的路径（含文件名），使用 `/` 分隔，如 `data_source/123123/aaa.csv`。服务端会做安全校验：拒绝 `..` 及以 `.` 开头的路径段、拒绝逃逸存储目录的路径，并自动剥离 Windows 盘符前缀（如 `C:/fakepath/`）。
- 参数校验失败时返回 400 级别错误。

## 存储目录配置

| 配置项 | 来源（优先级从高到低） | 默认值 |
|---|---|---|
| 数据文件目录 | 系统属性 / 环境变量 `SEATUNNEL_UPLOAD_FILE_PATH` → `seatunnel.yaml` 中 `upload-file.path` | `${SEATUNNEL_HOME}/data/upload-files` |
| jar 目录 | 系统属性 / 环境变量 `SEATUNNEL_UPLOAD_JAR_PATH` → `seatunnel.yaml` 中 `upload-file.jar-path` | `${SEATUNNEL_HOME}/data/upload-jars` |
| 定时清理目录 | 系统属性 / 环境变量 `SEATUNNEL_UPLOAD_CLEANUP_PATHS`（逗号分隔）→ `seatunnel.yaml` 中 `upload-file.cleanup-paths` | 数据文件存储目录 |

- 定时任务按 `cleanup-interval`（分钟）扫描清理目录，删除超过 `retention-days` 保留期的文件；**jar 目录默认永不清理**。
- Docker / Docker Compose / Kubernetes 部署时，需为每个副本挂载同一共享卷到上述目录。

## API 参考

### 上传文件

<details>
 <summary><code>POST</code> <code><b>/upload-file</b></code> <code>(上传一个或多个文件到共享存储目录。)</code></summary>

#### 参数

> | 参数名称         | 参数位置  | 是否必传 | 参数类型 | 参数描述                                                                                  |
> |--------------|-------|------|------|---------------------------------------------------------------------------------------|
> | fileType     | query | 否    | 字符串  | `file` / `jar`，缺省时按文件名后缀推断                                                            |
> | relativePath | query | 否    | 字符串  | 目标相对路径（含文件名），如 `data_source/123123/aaa.csv`，缺失目录自动创建；**指定后一次只允许上传 1 个文件**；缺省时使用 multipart 中携带的文件名 |
> | 文件内容         | body  | 是    | 二进制  | `multipart/form-data` 格式，支持多文件同时上传                                                    |

#### 行为说明

- 先写入临时文件（`.uploading.` 标记），完成后原子 move 到目标路径，避免共享存储上其他节点读到半截文件。
- 同名文件直接覆盖。

#### 响应

```json
[
  {
    "fileName": "data_source/123123/aaa.csv",
    "filePath": "/opt/seatunnel/data/upload-files/data_source/123123/aaa.csv",
    "fileType": "file",
    "fileSize": 1024
  }
]
```

#### 示例

```bash
curl -X POST "http://localhost:8080/hazelcast/rest/maps/upload-file?fileType=file&relativePath=data_source/123123/aaa.csv" \
  -F "file=@./aaa.csv"
```

</details>

------------------------------------------------------------------------------------------

### 文件列表

<details>
 <summary><code>GET</code> <code><b>/upload-file</b></code> <code>(返回 file 与 jar 两个存储目录下的全部文件列表。)</code></summary>

#### 参数

无。

#### 响应

返回数组，自动排除 `.uploading.` 临时文件：

```json
[
  {
    "fileName": "data_source/123123/aaa.csv",
    "filePath": "/opt/seatunnel/data/upload-files/data_source/123123/aaa.csv",
    "fileType": "file",
    "fileSize": 1024,
    "lastModified": 1724000000000
  }
]
```

`fileName` 为相对存储目录的路径（`/` 分隔），`lastModified` 为毫秒时间戳。

</details>

------------------------------------------------------------------------------------------

### 删除文件 / 文件夹

<details>
 <summary><code>DELETE</code> <code><b>/upload-file</b></code> <code>(按相对路径删除已上传的文件、jar 或整个文件夹。)</code></summary>

#### 参数

> | 参数名称     | 参数位置  | 是否必传 | 参数类型 | 参数描述                                |
> |----------|-------|------|------|-------------------------------------|
> | fileName | query | 是    | 字符串  | 相对存储目录的路径；删文件时含文件名（如 `data_source/123123/aaa.csv`），删文件夹时为目录路径（如 `data_source/123123`） |
> | fileType | query | 否    | 字符串  | `file` / `jar`，缺省时按文件名后缀推断          |
> | isDir    | query | 否    | 布尔值  | `true` 时把 `fileName` 当作目录，**递归删除其下全部文件与子目录**；缺省 `false` |

#### 行为说明

- 删除单个文件后，因此变空的父目录会自底向上被一并清理（不会越过存储根目录），保持磁盘状态与页面文件夹视图一致。
- `isDir=true` 时目录不存在同样报 `File not found`。

#### 响应

```json
{
  "fileName": "data_source/123123/aaa.csv",
  "fileType": "file",
  "isDir": false,
  "deleted": true
}
```

目标不存在时报错：`File not found: {fileName}`。

#### 示例

```bash
# 删除单个文件
curl -X DELETE "http://localhost:8080/hazelcast/rest/maps/upload-file?fileType=file&fileName=data_source/123123/aaa.csv"

# 递归删除整个文件夹
curl -X DELETE "http://localhost:8080/hazelcast/rest/maps/upload-file?fileType=file&fileName=data_source/123123&isDir=true"
```

</details>

------------------------------------------------------------------------------------------

### 下载文件

<details>
 <summary><code>GET</code> <code><b>/download-file</b></code> <code>(下载通过 /upload-file 上传的文件。)</code></summary>

#### 参数

> | 参数名称     | 参数位置  | 是否必传 | 参数类型 | 参数描述                                |
> |----------|-------|------|------|-------------------------------------|
> | fileName | query | 是    | 字符串  | 相对存储目录的路径（含文件名），如 `data_source/123123/aaa.csv` |
> | fileType | query | 否    | 字符串  | `file` / `jar`，缺省时按文件名后缀推断          |

#### 响应

- 成功：`200`，`Content-Type: application/octet-stream`，`Content-Disposition` 中携带 URL 编码后的文件名。
- 文件不存在时报错：`File not found: {fileName}`。

#### 示例

```bash
curl -OJ "http://localhost:8080/hazelcast/rest/maps/download-file?fileType=file&fileName=data_source/123123/aaa.csv"
```

</details>

------------------------------------------------------------------------------------------

### 判断文件是否存在

<details>
 <summary><code>GET</code> <code><b>/file-exists</b></code> <code>(判断文件是否存在于共享存储目录中，任意节点均可响应。)</code></summary>

#### 参数

> | 参数名称     | 参数位置  | 是否必传 | 参数类型 | 参数描述                                |
> |----------|-------|------|------|-------------------------------------|
> | fileName | query | 是    | 字符串  | 相对存储目录的路径（含文件名），如 `data_source/123123/aaa.csv` |
> | fileType | query | 否    | 字符串  | `file` / `jar`，缺省时按文件名后缀推断          |

#### 响应

```json
{
  "fileName": "data_source/123123/aaa.csv",
  "fileType": "file",
  "exists": true
}
```

#### 示例

```bash
curl "http://localhost:8080/hazelcast/rest/maps/file-exists?fileType=file&fileName=data_source/123123/aaa.csv"
```

</details>

------------------------------------------------------------------------------------------

## 前端调用对照

Web UI 的文件管理页面（`seatunnel-engine-ui` 中 `src/service/files/index.ts`）与上述接口的对应关系：

| 功能 | 前端方法 | 请求 |
|---|---|---|
| 列表 | `listFiles()` | `GET /upload-file` |
| 上传 | `uploadFile(file, fileType, relativePath?, onProgress?)` | `POST /upload-file` |
| 删除文件 | `deleteFile(fileName, fileType)` | `DELETE /upload-file` |
| 删除文件夹 | `deleteFile(dirPath, fileType, true)` | `DELETE /upload-file?isDir=true` |
| 判存 | `existsFile(fileName, fileType)` | `GET /file-exists` |
| 下载 | `buildDownloadUrl(fileName, fileType)` | `GET /download-file` |
