# dbloom 目标态设计 · OpenAPI 接口与权限矩阵（v1.0）

> 配套：`00-overview.md` D8/D9/D17/D18/D21；路由实现在 `dbloom-server`，契约用 **OpenAPI 3.x**（utoipa 生成 openapi.json + Swagger UI）。
> 两条凭据路径统一：`Authorization: Bearer <JWT>`（Web 人机）｜`Authorization: Bearer <API Key>`（机器，D9）。**同一套路由**共用中间件，仅鉴权来源不同。

---

## 1. 通用约定

- 前缀：`/api/v1/*`；规范文档：`GET /api/v1/openapi.json`、`GET /api/v1/docs`（Swagger UI）。
- 响应统一包装：成功 `{code:0, data:…}`；错误 `{code:<业务码>, message, trace_id}`，HTTP 状态码语义保留。
- 列表分页：`?page=1&page_size=50`（默认 20，上限 100）→ `{total, items}`。
- **租户过滤**：普通用户所有请求自动限定 `owner_user_id=当前用户`；管理员可用 `?user_id=` 显式跨用户（D8）。
- **统一转发（D17）**：凡数据库 IO 接口都只发生在 dbloom-server，前端永远不直连数据源。
- 幂等/重试契约：任务提交用 `idempotency_key`（创建任务时生成）；告警投递、调度触发等带幂等键防重复。

### 认证中间件行为（`dbloom-iam`）
1. 无/非法 Bearer → 401；2. JWT 过期（access）→ 401 + `code=40101`（前端据此刷新 token）；
3. API Key：查哈希 → 校验 `status=enabled` + `valid_from/valid_until` → 校验绑定用户有效 → 伪装为用户身份 ➜ 记调用审计（D21）。

---

## 2. 资源与端点总览

### 2.1 Auth（登录/会话）
| 方法 | 路径 | 说明 | 权限 |
| --- | --- | --- | --- |
| POST | `/api/v1/auth/login` | 用户名+密码 → `{access_token, refresh_token, user}`；失败计数锁定（D19） | 匿名 |
| POST | `/api/v1/auth/refresh` | 换新 access | 持有 refresh |
| POST | `/api/v1/auth/logout` | 吊销 refresh、落审计 | 登录 |
| GET | `/api/v1/auth/me` | 当前用户信息 | 登录 |
| POST | `/api/v1/auth/change-password` | 修改自己密码（含首登强改，D20） | 登录 |

### 2.2 Users（管理员域，D8）
| 方法 | 路径 | 说明 | 权限 |
| --- | --- | --- | --- |
| GET | `/api/v1/users` | 用户列表（可 `?status=`、`?keyword=`，可看到各自用量） | admin |
| POST | `/api/v1/users` | 创建普通用户（初始随机密码） | admin |
| GET | `/api/v1/users/{id}` | 用户详情 + 名下资源统计（连接/任务/API Key） | admin |
| PUT | `/api/v1/users/{id}` | 改显示名/状态（禁用/启用） | admin |
| POST | `/api/v1/users/{id}/reset-password` | **强制重置密码**（落审计；用户下次登录强制改密 D20） | admin |
| DELETE | `/api/v1/users/{id}` | 软删用户（D4 匿名化 + 数据保留策略） | admin |

### 2.3 API Keys（管理员域，D9）
| 方法 | 路径 | 说明 | 权限 |
| --- | --- | --- | --- |
| GET | `/api/v1/users/{id}/api-keys` | 某用户的 Key 列表 | admin |
| POST | `/api/v1/users/{id}/api-keys` | 签发 Key → **明文仅返回一次** | admin |
| PUT | `/api/v1/api-keys/{keyId}` | 更新 `name/status/valid_from/valid_until`（生效/失效/停用） | admin |
| DELETE | `/api/v1/api-keys/{keyId}` | 撤销 Key（立即全局生效，D9） | admin |

### 2.4 Connections（D6/D7/D17）
| 方法 | 路径 | 说明 | 权限 |
| --- | --- | --- | --- |
| GET | `/api/v1/connections` | 列表（密文脱敏；admin 可 `?user_id=`） | user/admin |
| POST | `/api/v1/connections` | 创建连接 | user/admin |
| GET | `/api/v1/connections/{id}` | 详情（凭据不返回明文） | 所属者/admin |
| PUT | `/api/v1/connections/{id}` | 更新（含标记生产、只读锁 D6） | 所属者/admin |
| DELETE | `/api/v1/connections/{id}` | 软删（有引用任务时提示） | 所属者/admin |
| POST | `/api/v1/connections/{id}/test` | 连接测试（**server 端发起，D17**） | 所属者/admin |
| GET | `/api/v1/connections/types` | 连接类型 manifest（前端表单、驱动参数、HOCON 能力同源，D7） | 登录 |
| POST | `/api/v1/connections/{id}/lock` / `.../unlock` | 生产只读锁切换（D6，危险操作需二次确认） | 所属者/admin |

### 2.5 Query / Schema / Data（客户端引擎，全走 D17）
| 方法 | 路径 | 说明 | 权限 |
| --- | --- | --- | --- |
| POST | `/api/v1/query/execute` | 执行 SQL → 结果集（**默认上限 5000 行/120s，D18**）；写 SQL 需 `confirm=true`（D6） | 所属者/admin |
| POST | `/api/v1/query/cancel` | 取消进行中查询 | 所属者/admin |
| WS | `/api/v1/ws/query` | 流式查询（大结果集分页推送、进度） | 所属者/admin |
| GET | `/api/v1/schema/{connId}/databases` / `.../tables` / `.../columns` | 元数据/表结构（带 README 无） | 所属者/admin |
| GET | `/api/v1/schema/{connId}/ddl/{table}` | 生成 DDL（客户端浏览） | 所属者/admin |
| POST | `/api/v1/data/{connId}/rows` | 行浏览（翻页/过滤/排序） | 所属者/admin |
| PUT | `/api/v1/data/{connId}/rows` | 行编辑（**写，二次确认 D6**） | 所属者/admin |
| DELETE | `/api/v1/data/{connId}/rows` | 行删除（**写，二次确认 D6**） | 所属者/admin |
| GET | `/api/v1/export/{connId}` | 导出库表 → **CSV / XLSX / JSON / SQL**（D23；后台任务生成文件落共享卷 files/download/） | 所属者/admin |
| POST | `/api/v1/import/{connId}` | 导入数据文件（上传落共享卷 files/upload/，D17/D10） | 所属者/admin |
| GET | `/api/v1/files/{id}/download` | 下载导出的文件 / 自定义 jar（D26：仅本人或 admin） | 所属者/admin |
| POST | `/api/v1/files/upload` | 上传文件（`purpose=data_upload|custom_jar`；custom_jar 归上传者本人，D26） | 所属者/admin |
| GET | `/api/v1/files` | 文件列表（可按 purpose、admin 可 `?user_id=`） | 所属者/admin |

### 2.6 Tasks / Runs（同步，D4/D5）
| 方法 | 路径 | 说明 | 权限 |
| --- | --- | --- | --- |
| GET | `/api/v1/tasks` | 任务列表（状态/类型/调度过滤） | 所属者/admin |
| POST | `/api/v1/tasks` | 创建：源/目标连接 + 同步类型 + 表映射 + 调度 cron + 重试/超时（D4/D5） | 所属者/admin |
| GET | `/api/v1/tasks/{id}` | 任务详情（含渲染 HOCON 快照） | 所属者/admin |
| PUT | `/api/v1/tasks/{id}` | 更新（改参需重新提交） | 所属者/admin |
| DELETE | `/api/v1/tasks/{id}` | 软删（先停再删） | 所属者/admin |
| POST | `/api/v1/tasks/{id}/enable` / `disable` | 调度开关 | 所属者/admin |
| POST | `/api/v1/tasks/{id}/trigger` | **手动触发**一次（可传覆盖参数） | 所属者/admin |
| POST | `/api/v1/tasks/{id}/stop` | 停止当前 run | 所属者/admin |
| POST | `/api/v1/tasks/{id}/retry` | 重试最近失败 run | 所属者/admin |
| GET | `/api/v1/tasks/{id}/runs` | 运行历史（D22 保留 90 天） | 所属者/admin |
| GET | `/api/v1/runs/{runId}` | 单次 run 详情（状态/指标/错误） | 所属者/admin |
| GET | `/api/v1/runs/{runId}/logs` | 分页日志（共享卷 tail） | 所属者/admin |
| WS | `/api/v1/ws/runs/{runId}/logs` | 实时日志流（D5） | 所属者/admin |
| GET | `/api/v1/tasks/{id}/dag` | 任务依赖 DAG（血缘/上下游，D5） | 所属者/admin |
| PUT | `/api/v1/tasks/{id}/dependencies` | 配置 DAG 边（同 owner 校验） | 所属者/admin |

### 2.7 Alerts / Notifications（D5/D16）
| 方法 | 路径 | 说明 | 权限 |
| --- | --- | --- | --- |
| GET/POST/PUT/DELETE | `/api/v1/notification-channels/...` | SMTP/Webhook 渠道 CRUD | admin |
| GET/POST/PUT/DELETE | `/api/v1/alert-rules/...` | 告警规则（条件/渠道/开关） | 所属者/admin |
| GET | `/api/v1/alert-events` | 告警事件流水（含投递状态） | 所属者/admin |

### 2.8 Admin & System
| 方法 | 路径 | 说明 | 权限 |
| --- | --- | --- | --- |
| GET | `/api/v1/audit-logs` | 全量审计查询（可按动作/用户/IP/时间筛选，D21） | admin |
| GET | `/api/v1/system/health` | 健康检查（含引擎 8080 连通、共享卷可写性、元数据库连通） | 匿名/登录 |
| GET | `/api/v1/system/info` | 版本/特性开关 | 匿名 |
| GET | `/api/v1/settings` | 系统配置读 | admin |
| PUT | `/api/v1/settings` | 更新（日志保留天数、任务历史保留、查询上限等，D18/D22） | admin |

---

## 3. 权限矩阵（角色 × 资源）

| 资源/操作 | admin | 普通用户（本人资源） |
| --- | --- | --- |
| 用户/API Key/审计/通知渠道管理 | ✅ 全量 | ❌ |
| 连接/任务/告警规则/文件 | ✅ 全量 + `?user_id=` 跨用户 | ✅ 仅 owner；跨用户 ❌ |
| 数据库 IO（查询/元数据/行编辑/导出导入） | ✅ 对任意连接（须具 owner 或显示指定） | ✅ 仅本人连接 |
| SQL 写操作（UPDATE/DELETE/DDL/行编辑） | 二次确认 + 只读锁校验 | 二次确认 + 锁校验；**DDL/DELETE 需连接标记且二次确认**（D6）；被锁连接一律拒绝 |
| 任务启停/重试/调度开关 | ✅ | ✅ 仅本人 |
| 仅管理员可见 | 跨用户数据、审计全量、密钥全文 | — |

> 鉴权在**服务端强制**；前端只是隐藏（防越权必须靠 `dbloom-storage` 租户过滤 + 本矩阵，见 `04-security.md`）。
