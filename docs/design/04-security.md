# dbloom 目标态设计 · 安全（Security，v1.0）

> 配套决策：D8（用户/隔离）、D9（API Key）、D6（写保护）、D17（统一转发）、D18–D21（限额/会话/密码/审计）。
> 分四个层次落地：**认证 → 授权（多租户）→ 数据保护 → 审计与监控**。

---

## 1. 认证（Authentication）

### 1.1 人机（Web UI）——JWT + Refresh
- `POST /api/v1/auth/login`（用户名+密码，argon2 校验）。
- 成功 → `access_token`（JWT，2h）+ `refresh_token`（随机 256bit，落 `sessions` 表存哈希，7 天）。
- access 无状态（签名字段：`sub=user_id, role, iat, exp`）；refresh 有状态（可吊销，logout/改密/重置密码即吊销全户）。
- 前端：access 存内存（防 XSS 窃取）；refresh 放 **HttpOnly + SameSite=Lax** Cookie（防 JS 读取）或 localStorage（可接受但记入风险清单）；access 过期 401 → 自动 refresh 静默续。
- **登录防爆破（D19）**：同一账号连续失败 5 次/5 分钟锁定（`failed_login_count` + `locked_until`）；管理员可解锁；记录审计。

### 1.2 机器（OpenAPI）——API Key（D9）
- 管理员为普通用户签发：服务端生成 `dbk_<32 hex>`，库中存 `sha256(key)` + `prefix`，**明文仅返回一次**（再次查询只给 prefix 与生命周期）。
- 校验流程（每次请求）：
  1. `Bearer <key>` → `sha256` 查 `api_keys`（含未删）；
  2. `status=enabled` 且 `valid_from <= now <= valid_until`（超出 → 401/403）；
  3. 校验绑定用户 `status=active` 且未删除；
  4. 通过 → 以该用户身份进入多租户上下文；记录 `last_used_at` + 调用审计（D21）。
- **简单生命周期（D9）**：撤销（DELETE）→ 立即全局生效（无缓存窗口）；到期即失效；暂不设数量上限/续期/提醒。
- 扩展位（默认关闭，实现时留字段/接口而不启用）：scope 白名单、来源 IP 白名单、调用限额。

### 1.3 鉴权来源归一
- JWT 与 API Key 共用一个中间件产出 `AuthCtx`，业务 handler 不关心来源，只认 `user_id + role`。

---

## 2. 授权与多租户隔离（Authorization）

### 2.1 角色
| 角色 | 能力 |
| --- | --- |
| `admin` | 全量：用户/API Key/审计/通知渠道/系统设置管理 + **跨用户视图**（`?user_id=`） |
| `user` | 仅本人资源（connections/tasks/alert_rules/files…），管理员授予身份外的 nothing else |

### 2.2 数据隔离（D8）——服务端强制
- 业务表带 `owner_user_id`；**所有 DAO 查询由 `dbloom-storage` 统一注入租户过滤**（`TenantScope`）。
- 管理员显式跨用户查询必须带 `user_id` 且校验目标存在；普通用户传 `user_id` 一律 403。
- 例外列表（`01-data-model.md` §3.4）：`sessions`（本人会话）、`api_keys`（管理员域）、`audit_logs`（管理员域）。
- **防越权规则**：路由层（路径参数中的资源 id）必须经 `dbloom-iam::authorize_resource(resource, actor)` 校验归属，未通过 404（避免泄漏存在性）。
- 隔离不依赖前端隐藏——前端只是 UX，越权防护在服务端。

### 2.3 写保护（D6）
- `connections.is_production=1` → 写操作需要**二次确认参数 `confirm=true`** + 醒目 UI；`read_only_lock=1` → 任何写操作（含 DDL/行编辑/TRUNCATE）拒绝。
- 危险语句识别：无 `WHERE` 的 UPDATE/DELETE、`DROP/ALTER/TRUNCATE/GRANT` 等 → 强制二次确认（即使非生产）。
- 普通用户在**任意连接**上无 DDL/DELETE 直接执行权限（D6 语义：需连接标记 + 二次确认 + 管理员可解锁强制）。

---

## 3. 数据保护（Data Protection）

### 3.1 连接凭据加密
- `password_enc` = `v1.<salt>.<iv>.<ciphertext_b64>`，**AES-256-GCM**（`dbloom-common`）。
- 主密钥：环境变量指向的密钥文件（`DBLOOM_SECRET_KEY_FILE`），k8s 用 Secret 挂载、compose 用 env_file、docker 用 env/挂载 secret。
- 密钥轮换：提供 `dbloom-server reencrypt <new-key-file>` 运维命令（遍历 connections 重写密文），文档说明停机窗口。
- 明文密码只在：用户填写时、服务端连接测试/执行（进程内存）、HOCON 渲染（进程内存）短暂存在——**永不落日志、永不返回前端、不在审计 detail 中出现**。

### 3.2 传输与边界
- dbloom-server 对外：HTTP（TLS 由外部网关终止，D15）；内部调用引擎 REST 走 localhost/集群内网（推荐开启引擎 basic-auth 或网络策略隔离）。
- 数据库连接：只从 dbloom-server/worker 出发（统一转发 D17），请求来源边界收敛在控制面。

### 3.3 API Key / Refresh Token 存储
- `refresh_hash`/`key_hash` 用 SHA-256（高熵随机值，无需慢哈希）；泄露面收敛到「明文仅在创建/签发时返回一次」。

### 3.4 会话与密钥配置要求
- 生产环境必须挂载 `DBLOOM_SECRET_KEY_FILE` 与 `JWT_SECRET`（随机生成）；未配置时**拒绝启动**（防开发默认密钥上线）。
- k8s Secret 用 Helm 生成的随机值 + 尽可能不落 values.yaml。

---

## 4. 审计（Audit，D21）

- 全量落 `audit_logs`（schema 见 `01-data-model.md`）。审计动作清单：
  `login / login_failed / logout / user_create / user_disable / user_reset_password / conn_create / conn_update / conn_delete / conn_test / conn_lock / sql_execute / sql_write_confirmed / export / import / api_key_issue / api_key_update / api_key_revoke / task_create / task_update / task_delete / task_trigger / task_enable_disable / task_stop / task_retry / schedule_change / alert_rule_change / settings_change`。
- **detail 只存摘要**：SQL 截断+脱敏（隐去字符串字面量/密码）；连接密码从不入审计；`actor_type=api_key` 标记外部调用来源。
- 审计查询仅 admin；保留 90 天（D22，同任务历史一起归档清理）。

---

## 5. 并发与一致性（外部元数据库，D10 v1.1）

- **元数据存外部关系库（MySQL/PG + ORM）**，多副本并行写由数据库事务/行锁保证一致性；无「单写者」角色。
- **调度防重**：`scheduler_jobs.lock_until` 用**行锁 + 条件更新（CAS）**抢占（`UPDATE ... WHERE lock_until <= now` 原子生效），抢到才触发任务；多副本即使并发触发也只有一个成功。
- **元数据库迁移锁**：启动迁移用 MySQL `GET_LOCK` / PostgreSQL advisory lock，防多副本并发改 schema。
- 故障转移：外部元数据库的高可用（主从/托管实例）由数据库层承担，dbloom-server 无状态多副本任意替换；编排细节见 `05-deploy.md` §3。

### 5.1 自定义 jar 上传安全（D26）
- jar 是**可执行代码**（SeaTunnel source/sink/transform），上传者即拥有在引擎 JVM 内执行代码的能力 → 将 worker 暴露给该用户提交的任务。
- **风险收敛**：jar 只允许**本人**引用、只参与本人提交的任务（租户隔离）；管理员可审计全局 jar 清单、可禁用问题用户；上传记录 `audit_logs`（`actor`、`files.id`、`sha256`、大小、目的）。
- 建议限制：单 jar 大小上限（如 200MB）、格式校验（z 前缀/zip 头）、只接收 `purpose=custom_jar`；worker 若与多用户数据面共享需评估隔离强度（一期同进程共享引擎，风险记录在案）。
- 用户软删后 jar 仍保留但不可再被引用（见 `01-data-model.md` §5）。

---

## 6. 安全测试验收清单（实现期回归用）

- [ ] 越权：用户 A 的 `connection/task/file` id 对用户 B → 404（非 403 泄漏）；admin 跨用户正常。
- [ ] API Key：过期/停用/撤销/绑定用户被禁用 → 立即拒绝；撤销后原 Key 调用 401。
- [ ] 写保护：只读锁连接任何写操作被拒；生产连接无 `confirm` 的写操作被拒；无 WHERE UPDATE 强制二次确认。
- [ ] SQL 注入/注入面收敛：SQL 经驱动参数化；HOCON 渲染对 `表名/列名` 做分隔符转义（反引号/双引号按方言）；连接配置只做参数不做拼串。
- [ ] 认证：锁定策略生效；登录/改密/重置写审计；敏感字段（password/jwt）不出现在任何响应与日志。
- [ ] 审计脱敏：`detail_json` 无明文密码/SQL 全量；导出文件不含密码。
- [ ] 自定义 jar（D26）：用户 A 的 jar 对用户 B 的任务 → 拒绝；`purpose=custom_jar` 校验归属；上传/引用/禁用均落审计。
