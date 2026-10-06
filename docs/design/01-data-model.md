# dbloom 目标态设计 · 数据模型（MySQL/PostgreSQL，ORM 层，v1.1）

> 配套：`00-overview.md` §2 决策 D8/D9/D10/D19/D20/D21/D22/D26；安全设计见 `04-security.md`。
> **v1.1（2026-10-06，D10 变更）**：元数据由 SQLite 改为**外部关系数据库**（默认 **MySQL**，PostgreSQL 可切换）。
> 所有 schema 经 **ORM 层**（M0 锁定：预估 SeaORM/sqlx）以迁移方式落地；本文 DDL 以 **MySQL 8 语法**示例，
> PostgreSQL 差异（`AUTO_INCREMENT`→`GENERATED ALWAYS AS IDENTITY`、`INTEGER PRIMARY KEY`→`SERIAL/BIGSERIAL`、
> JSON 类型映射、索引名长度等）由 ORM 迁移生成器按方言处理，业务代码看不到方言差异（多方言封装见 `03-modules.md` §2.3）。

---

## 0. 全局约定

- **元数据库实例**：部署形态自带（compose/k8s 编排）或连接外部托管实例（`DB_DSN` 环境变量），多副本可并行写（D10）。
- **主键**：`BIGINT AUTO_INCREMENT PRIMARY KEY`（PG：`GENERATED ALWAYS AS IDENTITY`）。
- **时间**：统一 `BIGINT`（Unix 毫秒，UTC）——跨 MySQL/PG/前端/Java 无时区歧义，ORM `bigint` 列。
- **软删除**：业务表带 `deleted_at`（NULL=未删）；删除一律软删（保审计）。
- **版本**：`schema_migrations(version, applied_at)`（由 ORM 迁移器管理）。
- **租户列**：业务实体统一带 `owner_user_id`；**查询强制带租户过滤**（普通用户附加 `owner_user_id = ?`，管理员可显式指定）。
- **外键**：MySQL InnoDB 引擎启用；`ON DELETE` 业务上多数用软删，故用 RESTRICT / 应用层处理。
- **索引命名**：`idx_<表>_<列>`（PG 全库索引名唯一，ORM 生成时注意去重）。
- 字符集：MySQL `utf8mb4`；连接池推荐 `utf8mb4_general_ci` 排序。

---

## 1. 表清单

| 表 | 用途 | 决策关联 |
| --- | --- | --- |
| `users` | 用户账号 | D8/D19/D20 |
| `sessions` | refresh token 会话（可吊销） | D19 |
| `api_keys` | OpenAPI 访问凭证 | D9 |
| `connections` | 数据库连接配置 | D6/D7/D17 |
| `tasks` | 同步任务定义 | D4/D5 |
| `task_dependencies` | 任务 DAG 依赖边 | D5 |
| `task_runs` | 任务运行记录（job 实例） | D5/D22 |
| `task_run_logs` | 任务运行日志指针 / 摘要 | D5/D22 |
| `scheduler_jobs` | 调度器与任务的关联状态 | D5 |
| `notification_channels` | 告警渠道（SMTP/Webhook） | D16 |
| `alert_rules` | 告警规则 | D5/D16 |
| `alert_events` | 告警事件流水 | D5/D21 |
| `audit_logs` | 审计日志 | D21 |
| `files` | 上传/下载文件元数据（含 **自定义 jar**，D26） | D10/D17/D26 |
| `favorites` | 用户收藏（连接/表 快捷） | （体验） |
| `settings` | 系统级 key-value 配置 | 通用 |

---

## 2. 建表 DDL（目标态，MySQL 8 语法；PG 由 ORM 迁移器按方言生成）

```sql
-- 元数据库初始化：CREATE DATABASE dbloom CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci;
-- 由 dbloom-storage 的 ORM 迁移器在启动时按版本应用（见 §4）。

-- ============================ 用户与认证 ============================
CREATE TABLE users (
  id                   BIGINT AUTO_INCREMENT PRIMARY KEY,
  username             VARCHAR(128) NOT NULL,                 -- 登录名（唯一）
  password_hash        VARCHAR(255) NOT NULL,                 -- argon2 hash
  role                 VARCHAR(16)  NOT NULL DEFAULT 'user',  -- 'admin' | 'user'
  status               VARCHAR(16)  NOT NULL DEFAULT 'active',
                                                        -- 'active'|'locked'|'disabled'
  display_name         VARCHAR(255),
  must_change_password TINYINT(1)   NOT NULL DEFAULT 0,      -- 1=首登/被重置后强制改密 D20
  failed_login_count   INT          NOT NULL DEFAULT 0,      -- 连续失败次数（D19 锁定阈值）
  locked_until         BIGINT,                               -- 锁到该时间（ms，NULL=未锁）
  created_by           BIGINT,                               -- 管理员 id（内置 admin 为 NULL）
  last_login_at        BIGINT,
  created_at           BIGINT NOT NULL,
  updated_at           BIGINT NOT NULL,
  deleted_at           BIGINT,
  CONSTRAINT uq_users_username UNIQUE (username, deleted_at)
);
-- PG: username VARCHAR(128) NOT NULL UNIQUE + 部分唯一索引 (WHERE deleted_at IS NULL)

-- refresh token 会话（D19：refresh 7d、可吊销、踢下线）
CREATE TABLE sessions (
  id           BIGINT AUTO_INCREMENT PRIMARY KEY,
  user_id      BIGINT NOT NULL,
  refresh_hash VARCHAR(128) NOT NULL,          -- 只存哈希，不存明文
  expires_at   BIGINT NOT NULL,
  revoke_reason VARCHAR(64),                   -- NULL=未吊销；'logout'|'password_reset'|'expired'…
  created_at   BIGINT NOT NULL,
  last_used_at BIGINT,
  CONSTRAINT fk_sessions_user FOREIGN KEY (user_id) REFERENCES users(id)
);
CREATE INDEX idx_sessions_user ON sessions(user_id);
CREATE INDEX idx_sessions_expiry ON sessions(expires_at);

-- ============================ API Key（D9）============================
CREATE TABLE api_keys (
  id          BIGINT AUTO_INCREMENT PRIMARY KEY,
  user_id     BIGINT NOT NULL,                -- 所属普通用户
  created_by  BIGINT NOT NULL,                -- 签发管理员
  name        VARCHAR(255) NOT NULL,          -- 用途备注
  key_hash    VARCHAR(128) NOT NULL,          -- SHA-256(key 明文)（密钥段）D9
  prefix      VARCHAR(32)  NOT NULL,          -- 展示前缀 dbk_xxxx…（便于识别）
  status      VARCHAR(16)  NOT NULL DEFAULT 'enabled',
  valid_from  BIGINT,                         -- 生效时间（NULL=即刻）
  valid_until BIGINT,                         -- 失效时间（NULL=永久）
  last_used_at BIGINT,
  created_at  BIGINT NOT NULL,
  updated_at  BIGINT NOT NULL,
  deleted_at  BIGINT,
  CONSTRAINT fk_apikeys_user FOREIGN KEY (user_id) REFERENCES users(id)
);
CREATE INDEX idx_api_keys_user ON api_keys(user_id, deleted_at);

-- ============================ 连接（D6/D7/D17）========================
CREATE TABLE connections (
  id            BIGINT AUTO_INCREMENT PRIMARY KEY,
  owner_user_id BIGINT NOT NULL,              -- 租户列
  name          VARCHAR(255) NOT NULL,
  conn_type     VARCHAR(32)  NOT NULL,        -- mysql|postgres|sqlserver|mongodb|redis|elasticsearch（D7）
  host          VARCHAR(255) NOT NULL,        -- 可为集群内部 service 名（D17）
  port          INT,
  database_name VARCHAR(255),                 -- 库/索引 名
  username      VARCHAR(255),
  password_enc  TEXT,                         -- AES-256-GCM 密文（base64），主密钥见 04-security §3
  ssl_mode      VARCHAR(32)  DEFAULT 'disable',-- disable|require|verify-ca|verify-full
  extra_params  JSON,                         -- 驱动专有参数/字符集/超时等
  is_production TINYINT(1)   NOT NULL DEFAULT 0,  -- 生产标记（D6）
  read_only_lock TINYINT(1)  NOT NULL DEFAULT 0,  -- 生产只读锁：1=禁止任何写（D6）
  created_at    BIGINT NOT NULL,
  updated_at    BIGINT NOT NULL,
  deleted_at    BIGINT,
  CONSTRAINT fk_conns_owner FOREIGN KEY (owner_user_id) REFERENCES users(id)
);
CREATE INDEX idx_connections_owner ON connections(owner_user_id, deleted_at);
CREATE INDEX idx_connections_type ON connections(conn_type);

-- ============================ 同步任务（D4/D5）========================
CREATE TABLE tasks (
  id            BIGINT AUTO_INCREMENT PRIMARY KEY,
  owner_user_id BIGINT NOT NULL,              -- 租户列
  name          VARCHAR(255) NOT NULL,
  description   VARCHAR(1024),
  source_connection_id BIGINT NOT NULL,
  sink_connection_id   BIGINT NOT NULL,
  sync_mode     VARCHAR(16)  NOT NULL DEFAULT 'batch',
                                        -- 'batch'|'increment'|'cdc'（D4：透传 ST 能力）
  config_hocon  TEXT NOT NULL,                -- 渲染后的 ST HOCON（快照，提交即锁定）
  schedule_cron VARCHAR(64),                  -- cron 表达式；NULL=仅手动
  enabled       TINYINT(1)   NOT NULL DEFAULT 1,   -- 调度开关
  timeout_sec   INT          NOT NULL DEFAULT 0,   -- 0=不限
  retry_times   INT          NOT NULL DEFAULT 0,   -- 失败重试次数（D5）
  concurrency_limit INT       NOT NULL DEFAULT 1,  -- 同任务并发实例上限
  created_by    BIGINT,
  created_at    BIGINT NOT NULL,
  updated_at    BIGINT NOT NULL,
  deleted_at    BIGINT,
  CONSTRAINT fk_tasks_source FOREIGN KEY (source_connection_id) REFERENCES connections(id),
  CONSTRAINT fk_tasks_sink   FOREIGN KEY (sink_connection_id)   REFERENCES connections(id),
  CONSTRAINT fk_tasks_owner  FOREIGN KEY (owner_user_id)        REFERENCES users(id)
);
CREATE INDEX idx_tasks_owner ON tasks(owner_user_id, deleted_at);

-- 任务 DAG（D5）
CREATE TABLE task_dependencies (
  task_id            BIGINT NOT NULL,
  depends_on_task_id BIGINT NOT NULL,
  created_at         BIGINT NOT NULL,
  PRIMARY KEY (task_id, depends_on_task_id),   -- 注意同 owner 校验在应用层
  CONSTRAINT fk_td_task   FOREIGN KEY (task_id)            REFERENCES tasks(id),
  CONSTRAINT fk_td_depends FOREIGN KEY (depends_on_task_id) REFERENCES tasks(id)
);

-- 任务运行实例
CREATE TABLE task_runs (
  id                BIGINT AUTO_INCREMENT PRIMARY KEY,
  task_id           BIGINT NOT NULL,
  owner_user_id     BIGINT NOT NULL,          -- 冗余租户列，加速过滤（D8）
  trigger_type      VARCHAR(16) NOT NULL,     -- 'manual'|'cron'|'dag'
  sea_tunnel_job_id VARCHAR(128),             -- 引擎侧 job id（提交后回填）
  status            VARCHAR(16)  NOT NULL DEFAULT 'pending',
                                -- 'pending'|'running'|'succeeded'|'failed'|'stopped'|'canceled'
  attempt           INT          NOT NULL DEFAULT 1,   -- 第几次尝试（重试 D5）
  start_time        BIGINT,
  end_time          BIGINT,
  error_message     TEXT,
  log_path          VARCHAR(1024),            -- 共享卷 logs/tasks/<runId>.log（D22）
  created_at        BIGINT NOT NULL,
  updated_at        BIGINT NOT NULL,
  CONSTRAINT fk_runs_task FOREIGN KEY (task_id) REFERENCES tasks(id)
);
CREATE INDEX idx_runs_task ON task_runs(task_id, id);
CREATE INDEX idx_runs_owner ON task_runs(owner_user_id, id);
CREATE INDEX idx_runs_status ON task_runs(status);

-- 运行日志摘要/游标（WebSocket tail 用）
CREATE TABLE task_run_logs (
  run_id       BIGINT PRIMARY KEY,
  last_line    BIGINT NOT NULL DEFAULT 0,     -- 已推送行号游标
  log_size     BIGINT NOT NULL DEFAULT 0,
  updated_at   BIGINT NOT NULL,
  CONSTRAINT fk_rlog_run FOREIGN KEY (run_id) REFERENCES task_runs(id)
);

-- 调度器任务登记（内置 cron，D5）
CREATE TABLE scheduler_jobs (
  id              BIGINT AUTO_INCREMENT PRIMARY KEY,
  task_id         BIGINT NOT NULL,
  next_run_at     BIGINT,
  last_run_at     BIGINT,
  last_result     VARCHAR(255),
  lock_until      BIGINT,                     -- 多副本抢占调度（DB 行锁 CAS，D10）
  updated_at      BIGINT NOT NULL,
  CONSTRAINT uq_sched_task UNIQUE (task_id),
  CONSTRAINT fk_sched_task FOREIGN KEY (task_id) REFERENCES tasks(id)
);

-- ============================ 告警（D5/D16）=========================
CREATE TABLE notification_channels (
  id          BIGINT AUTO_INCREMENT PRIMARY KEY,
  name        VARCHAR(255) NOT NULL,
  channel_type VARCHAR(16)  NOT NULL,         -- 'smtp'|'webhook'
  config_json JSON NOT NULL,                  -- smtp:{host,port,user,pass,sender} webhook:{url,secret_header,secret}
  enabled     TINYINT(1)   NOT NULL DEFAULT 1,
  created_by  BIGINT,
  created_at  BIGINT NOT NULL,
  updated_at  BIGINT NOT NULL,
  deleted_at  BIGINT
);

CREATE TABLE alert_rules (
  id            BIGINT AUTO_INCREMENT PRIMARY KEY,
  owner_user_id BIGINT NOT NULL,              -- 租户列
  name          VARCHAR(255) NOT NULL,
  scope         VARCHAR(16)  NOT NULL DEFAULT 'task',  -- 'task'|'global'
  task_id       BIGINT,
  conditions    JSON NOT NULL,                -- [{type: task_failed|task_success|long_running, threshold}]
  channel_ids   JSON NOT NULL,                -- JSON 数组 → notification_channels.id
  enabled       TINYINT(1)   NOT NULL DEFAULT 1,
  created_by    BIGINT,
  created_at    BIGINT NOT NULL,
  updated_at    BIGINT NOT NULL,
  deleted_at    BIGINT,
  CONSTRAINT fk_rules_owner FOREIGN KEY (owner_user_id) REFERENCES users(id)
);
CREATE INDEX idx_alert_rules_owner ON alert_rules(owner_user_id, deleted_at);

CREATE TABLE alert_events (
  id          BIGINT AUTO_INCREMENT PRIMARY KEY,
  rule_id     BIGINT NOT NULL,
  task_run_id BIGINT,
  condition   VARCHAR(255) NOT NULL,
  message     VARCHAR(1024),
  channels    JSON NOT NULL,                  -- 实际投递的渠道 ids
  deliver_status VARCHAR(16) NOT NULL DEFAULT 'pending',
                                        -- 'pending'|'success'|'failed'|'skipped'
  delivered_at BIGINT,
  created_at  BIGINT NOT NULL,
  CONSTRAINT fk_events_rule FOREIGN KEY (rule_id) REFERENCES alert_rules(id)
);
CREATE INDEX idx_alert_events_rule ON alert_events(rule_id, id);

-- ============================ 审计（D21）============================
CREATE TABLE audit_logs (
  id            BIGINT AUTO_INCREMENT PRIMARY KEY,
  actor_user_id BIGINT,                       -- 操作者（机器=Key 绑定用户）
  actor_type    VARCHAR(16)  NOT NULL DEFAULT 'user',  -- 'user'|'api_key'
  action        VARCHAR(64)  NOT NULL,        -- login|logout|user_create|user_reset_password|conn_create|conn_test|sql_execute|sql_write|task_create|task_trigger|task_stop|task_retry|apikey_issue|apikey_revoke|schedule_change|…
  resource_type VARCHAR(64),
  resource_id   VARCHAR(128),
  detail_json   JSON,                         -- 额外上下文（SQL 摘要、来源 IP、User-Agent）
  ip            VARCHAR(64),
  created_at    BIGINT NOT NULL
);
CREATE INDEX idx_audit_actor ON audit_logs(actor_user_id, id);
CREATE INDEX idx_audit_created ON audit_logs(created_at);
CREATE INDEX idx_audit_action ON audit_logs(action);

-- ============================ 文件（D10/D17/D26）=====================
CREATE TABLE files (
  id            BIGINT AUTO_INCREMENT PRIMARY KEY,
  owner_user_id BIGINT NOT NULL,              -- 租户列（D26：自定义 jar 属各自用户）
  name          VARCHAR(255) NOT NULL,        -- 原始文件名
  stored_path   VARCHAR(1024) NOT NULL,       -- 共享卷内相对路径 files/upload/<uuid>.<ext>
  size          BIGINT NOT NULL,
  sha256        VARCHAR(64),
  purpose       VARCHAR(32),                  -- 'data_upload'|'custom_jar'|'download'（D26）
  created_at    BIGINT NOT NULL,
  deleted_at    BIGINT,
  CONSTRAINT fk_files_owner FOREIGN KEY (owner_user_id) REFERENCES users(id)
);
CREATE INDEX idx_files_owner ON files(owner_user_id, deleted_at);
CREATE INDEX idx_files_purpose ON files(purpose);

-- ============================ 收藏 / 系统设置 ========================
CREATE TABLE favorites (
  id          BIGINT AUTO_INCREMENT PRIMARY KEY,
  owner_user_id BIGINT NOT NULL,
  kind        VARCHAR(32) NOT NULL,           -- connection|table|sql_template
  ref_id      VARCHAR(255) NOT NULL,          -- 连接 id / 库.表 / SQL 模板 id
  sort_order  INT NOT NULL DEFAULT 0,
  created_at  BIGINT NOT NULL,
  CONSTRAINT fk_fav_owner FOREIGN KEY (owner_user_id) REFERENCES users(id)
);

CREATE TABLE settings (
  key        VARCHAR(255) PRIMARY KEY,
  value      TEXT,
  updated_by BIGINT,
  updated_at BIGINT NOT NULL,
  CONSTRAINT fk_settings_user FOREIGN KEY (updated_by) REFERENCES users(id)
);
```

---

## 3. 关键字段说明

### 3.1 连接凭据加密（`connections.password_enc`）
- 密文格式：`<version>.<salt_b64>.<iv_b64>.<ciphertext_b64>`，AES-256-GCM。
- 主密钥来自 `DBLOOM_SECRET_KEY_FILE`（挂载 secret 文件 → 环境变量），见 `04-security.md` §3。
- 重启重加密/密钥轮换：提供 `dbloom-server` 运维子命令 `reencrypt`（遍历 connections 用新密钥重写）；先落地接口与文档。
- `extra_params` 用 JSON 列（PG 可用 JSONB），承载驱动专有参数与多方言字段差异。

### 3.2 任务 `config_hocon` 快照 vs 动态
- 保存**提交时的渲染快照**（审计/复现）。任务详情可查看/导出快照；
- 变更连接/参数后重新渲染需**重新提交**，历史 run 不受影响。

### 3.3 审计 `detail_json` 只存「摘要」
- SQL 只存脱敏摘要（截断 + 隐去宿主/密码字面量），不整段明文落库（防拖库泄露），完整日志留在共享卷 `logs/`。
- 写操作必审计（D21）；`sql_execute` 只记 SELECT 摘要（可配）。

### 3.4 多租户强制过滤
- 所有对 `connections/tasks/task_runs/alert_rules/files` 的 DAO 查询，`dbloom-storage` 层注入 `owner_user_id`。
- 管理员查询带可选 `?user_id=` 显式跨用户（D8）。
- 唯一例外：`sessions/api_keys/audit_logs` 属系统/管理员级资源（API Key 绑定用户但由管理员管理）。

### 3.5 自定义 jar（D26，v1.1 新增）
- `files.purpose='custom_jar'`、`owner_user_id=上传者`：每个用户上传的 jar 只归自己，**仅本人可引用**。
- 任务引用 jar：`tasks.config_hocon` 中或系统配置里给出 jar 归属校验（构建 HOCON 时只允许引用 owner 本人的 jar）。
- 管理员可查看全局 jar 清单并审计谁传了什么；可禁用用户（其 jar 随之不可用）。

---

## 4. 迁移与初始化

- `dbloom-storage` 内置 ORM 迁移清单（v1…vN），`schema_migrations` 记录，启动时按顺序应用（多副本用 DB 级迁移锁：MySQL `GET_LOCK` / PG `pg_advisory_lock`，防并发迁移）。
- **元数据库连接**：环境变量 `DB_DSN`（如 `mysql://user:pass@host:3306/dbloom`），可连外部托管实例；部署形态默认编排一份（见 `05-deploy.md`）。
- **初始化种子**（幂等）：
  1. 内置管理员（`username=admin`，密码随机生成打印日志 + `must_change_password=1`，D20）；
  2. 系统默认审计/日志保留参数写入 `settings`（D22：日志 14 天、任务历史 90 天）。
- 首次启动创建库表；非结构化文件（日志/上传下载/自定义 jar/checkpoint）不在此库，落共享卷（D10）。

---

## 5. 数据删除与保留

- 用户删除 = **软删** `users.deleted_at` + 匿名化（username 追加 `__deleted_<id>`）；其业务数据（连接/任务/文件）进入该用户名下，不再对任何非管理员可见；管理员可归档导出后物理清理（提供运维命令，不自动执行）。
- **自定义 jar 清理**：用户软删后其 jar 文件保留但不可引用；由管理员归档后清理。
- **任务历史**：默认保留 90 天（D22），超期由后台清理任务删除 `task_runs` + 共享卷 `logs/tasks/*.log`。
- **应用日志**：14 天滚动（共享卷 `logs/app/`）。
- 元数据库备份策略见 `05-deploy.md`（外部 DB 自带备份机制/快照）。
