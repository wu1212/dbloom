# dbloom 目标态设计 · 代码结构（Modules，v1.1）

> 配套：`00-overview.md` §6；分层沿用 dbx 单向依赖原则（上层依赖下层，下层不得反向依赖上层）。
> v1.1（2026-10-06）：`dbloom-storage` 由 SQLite 改为 **ORM 多方言（MySQL/PG）**，见 §2.3。
> 依赖方向：`dbloom-common` ← `dbloom-storage` ← `dbloom-iam` / `dbloom-types`，业务模块 `dbloom-connector` / `dbloom-sync`，最上层 `dbloom-server`。

---

## 1. 仓库布局

```text
dbloom/
├─ apps/web/                  # 前端（React + Vite + antd，D12/D25）
├─ engine/                    # 裁剪精简的 SeaTunnel（Maven 多模块，D2）
├─ crates/
│  ├─ dbloom-server/          # HTTP+WS 装配、路由、OpenAPI 契约装配（二进制）
│  ├─ dbloom-iam/             # 用户/认证/API Key/多租户过滤/审计写入
│  ├─ dbloom-connector/       # 数据库客户端引擎（D17 执行）——连接池/驱动适配/SQL/元数据/行编辑/导出导入/写保护
│  ├─ dbloom-sync/            # 同步编排：HOCON 生成/ST REST 客户端/任务状态机/内置调度/告警引擎
│  ├─ dbloom-storage/         # 元数据库访问层（ORM 多方言 MySQL/PG + DAO + 租户过滤 + 迁移）
│  ├─ dbloom-types/           # 共享 DTO / 连接类型 manifest / 任务模型 / 错误码
│  └─ dbloom-common/          # 配置/日志/加密/时间/平台工具
├─ deploy/
│  ├─ docker/                 # Dockerfile（统一镜像多阶段：web→静态、crates→server、engine→JVM 运行时）
│  ├─ compose/                # docker-compose：dbloom-master / dbloom-worker + 共享卷
│  └─ kubernetes/             # Helm chart：master/worker Deployment + RWX PVC + ConfigMap + Service
├─ docs/
│  ├─ architecture.md         # 背景与决策记录（来源依据）
│  └─ design/                 # 本套目标态设计（00…06）
├─ README.md
└─ AGENTS.md
```

---

## 2. Rust crate 职责与边界

### 2.1 `dbloom-common`（无依赖上层）
- 配置加载（环境变量 + 配置文件 `dbloom.toml` + 共享 `settings` 覆盖）。
- 日志（非阻塞 slog/tracing，输出到 stdout + 共享卷 `logs/app/`，14 天滚动 D22）。
- **凭据加密**：AES-256-GCM 封装（`encrypt/decrypt/derive_key_from_secret_file`），主密钥来自 `DBLOOM_SECRET_KEY_FILE`。
- 错误类型（业务码 + trace_id）、时间工具、`AppError` 到 HTTP 的转换。

### 2.2 `dbloom-types`
- DTO（`/api/v1` 数据结构，前后端+OpenAPI 共享）。
- **连接类型 manifest**（单一事实来源，D7）：每个类型声明
  `form_fields[]`（页面表单字段）、`driver_params`（客户端驱动参数）、`st_source_snippet/st_sink_snippet`（SeaTunnel HOCON 模板片段）、`capabilities`（客户端能力、同步能力 flags）。
  前端表单、客户端连接参数、HOCON 生成全部从 manifest 推导（借鉴 dbx `plugins/connection-types`）。
- 任务模型（`TaskDefinition / TaskRun / SyncMode / HOCON 模板变量`）、错误码枚举。

### 2.3 `dbloom-storage`
- **ORM 层（SQLx/SeaORM，多方言）**：连接外部元数据库（默认 MySQL，PG 可切，D10）；sqlx 连接池 + 超时/重试；方言差异封装（DDL 由迁移器按目标库生成，查询构建尽量走 ORM 避免裸 SQL 方言）。
- **迁移清单**（v1…，`01-data-model.md` §4）：SeaORM Migration 或 sqlx migrate；DB 级迁移锁（MySQL `GET_LOCK` / PG advisory lock）防多副本并发迁移。
- **DAO**：按表分组；**统一租户过滤封装**（`TenantScope`：当前用户 + 是否 admin）——所有查询注入 `owner_user_id`，例外表见 `01-data-model.md` §3.4。
- **多副本并发**：外部库天然支持多写，事务/行锁保证一致性；不再有「单写者」角色。
- 分页/统计工具、`settings` 读写。

### 2.4 `dbloom-iam`
- 用户 CRUD（admin）、密码哈希（argon2）、`reset-password`（D20）。
- 会话：refresh token 签发/校验/吊销（`sessions` 表，D19）。
- JWT 签发与校验（access 2h；`sub`+`role`+`tenant` 声明）。
- **API Key**：签发（哈希存 库 + 明文仅返回一次）、校验（status/时间窗/绑定用户）、生命周期管理（D9）。
- **多租户过滤 + 权限判定**：中间件组件 `AuthCtx{user, role, key_created_by?}`；每个 handler 用它做资源归属判定。
- **审计写入**：`audit(actor, action, resource, detail)`（D21）。

### 2.5 `dbloom-connector`（客户端能力，D17 统一转发执行）—— **薄包装，复用 dbx（D27）**
> **原则（D27）：不重写已验证代码。** 查询 / 元数据 / 行编辑 / 写保护 / 导出 / 连接测试全部**直接依赖 dbx 开源 crate 族**
> （`G:\work\dbx`，Apache-2.0），dbloom-connector 只做 **DTO ↔ `dbx::models::connection::ConnectionConfig` 薄映射**。

- **层叠**：
  1. `dbx` 聚合 crate 或 `dbx-driver-{mysql,postgres,sqlserver,mongodb,redis,elasticsearch}`（D7 六库）+ `dbx-sql-*` / `dbx-formats` / `dbx-core::{safety, data, query}` —— 全部已场景验证，按需 path 依赖引入。
  2. dbloom-connector：`ConnectionDto(加密解密后) → dbx ConnectionConfig`（host/port/username/password/database/db_type/ssl/timeout 字段同构，见 D27 对齐清单）；暴露 `dbloom::Driver` 薄 trait（`test/query/meta/rows/update/export`）供 server 路由调用。
- **连接类型清单**：消费 dbx `database_manifest`（构建期生成的 manifest JSON），不再自建 manifest——dbloom 只在其上叠加 SeaTunnel 同步模板片段（`st_source/st_sink`，HOCON）。
- **写保护（safety，D6）**：复用 dbx `dbx-core::safety`（危险语句识别 / 生产只读锁 / 二次确认），dbloom 叠加「普通用户无 DDL」与任务无关约束。
- **导出（D23）**：复用 `dbx-core::data::table_export / query_result_export`、`dbx-formats`，dbloom 只负责落共享卷 `files/download/` 与下载白名单。

### 2.6 `dbloom-sync`（同步能力）
- **HOCON 生成器**：连接 manifest 模板片段 + 用户同步参数（表映射/过滤/并行度/checkpoint/同步类型 D4）→ 渲染成 `<env, source, sink(s), transform(s)>` 完整 HOCON；含 `jobName/idempotency` 等。
- **SeaTunnel REST 客户端**：`submit-job / cancel-job / get-job-status / get-job-info / upload-file`（引擎 8080，本地 master）。自定义 jar（D26）：上传时就校验归属（`files.purpose='custom_jar'` + 上传者），提交任务时只允许引用 owner 本人的 jar，构建 HOCON 时把 jar 加入 `env`/`source` 的 classpath 引用。
- **任务状态机**：`pending→running→succeeded|failed|stopped|canceled`；失败重试（`retry_times`）、超时、停/重试。
- **内置调度器**：cron 解析/排序/触发；DB 级抢占（`scheduler_jobs.lock_until` 行锁/CAS，避免多副本重复触发，D10）；`manual/cron/dag` 三种触发。
- **DAG 引擎**：按 `task_dependencies` 拓扑，上游成功后触发下游（D5）。
- **日志 tail**：从共享卷 `logs/tasks/<runId>.log` 增量读取，通过 WS 推送（含游标 `task_run_logs`）。
- **告警引擎**：规则匹配 → SMTP（lettre）/ Webhook（reqwest + 签名头）投递 → `alert_events` 记录投递结果（D5/D16）。

### 2.7 `dbloom-server`
- axum 应用装配：路由注册（`02-api.md` §2）、OpenAPI（utoipa）导出、全局中间件（认证/审计/租户/请求 ID）。
- WS 端点：`/ws/query`、`/ws/runs/{id}/logs`。
- 静态托管前端构建产物（可选）；`/api/v1/health` 探测（引擎/共享卷/元数据库）。

---

## 3. 连接类型 manifest（D7 设计样例）

```yaml
# crates/dbloom-types/src/manifests/mysql.yaml —— 单类型的 manifest
name: mysql
label: MySQL
kind: sql            # sql | nosql | cache（决定客户端驱动与 SQL 能力）
form_fields:
  - {key: host, label: 主机, required: true, placeholder: "如 mysql-service 或 10.0.1.5（集群内可达地址）"}
  - {key: port, label: 端口, default: 3306}
  - {key: database, label: 数据库}
  - {key: username, label: 用户名}
  - {key: password, label: 密码, type: password}
  - {key: ssl_mode, label: SSL, type: select, options: [disable, require, verify-ca, verify-full]}
client:
  driver: jdbc-mysql
  jdbc_url_template: "jdbc:mysql://{host}:{port}/{database}?useSSL={ssl}&{extra}"
sync:                # 同步侧映射（D4：透传 SeaTunnel 能力）
  source_snippet: |
    source { Jdbc { url="{jdbc_url}" user="{username}" password="{password}"
       table_list = [{table_path = "{database}.{table}"}]
       result_table_name = "{table}" } }
  sink_snippet: |
    sink { Jdbc { url="{jdbc_url}" user="{username}" password="{password}"
       generate_sink_sql = true } }
capabilities:
  client: [query, schema, rows, edit, export, import]
  sync:   [batch, increment, cdc]   # cdc 依赖引擎是否启用 binlog connector（M3 按交集裁剪）
```

> 前端表单、`dbloom-connector` 驱动参数、`dbloom-sync` HOCON 生成全部消费同一份 manifest —— 实现「**配一次连接，客户端 + 同步都能用**」的关键。

---

## 4. 前端 `apps/web` 结构（D12/D25）

```text
apps/web/
├─ src/
│  ├─ api/            # OpenAPI 生成/手写 client + TanStack Query hooks
│  ├─ auth/           # 登录态、token 刷新、路由守卫
│  ├─ pages/
│  │  ├─ login/
│  │  ├─ connections/     # 连接列表/新建表单(manifest 驱动)/测试/只读锁
│  │  ├─ workspace/       # SQL 工作台（编辑器 + 结果表 + 危险确认 + 导出）+ 数据浏览/编辑
│  │  ├─ schema/          # 元数据树（库/表/列/DDL）
│  │  ├─ sync/            # 任务列表/详情(状态机)/创建向导(源→目标→表映射→同步类型→调度)
│  │  ├─ runs/            # 运行历史/实时日志(WS)
│  │  ├─ alerts/          # 规则/事件/投递状态
│  │  └─ admin/           # 用户管理 / API Key / 审计 / 系统设置（admin only）
│  ├─ components/     # 通用组件（结果表格、SQL 编辑器 CodeMirror、DAG 图 echarts）
│  ├─ store/          # zustand：用户态/连接态
│  └─ i18n/           # 中文默认 + 结构预留（D24）
└─ vite.config.*
```

---

## 5. engine/ 裁剪范围（D2：裁剪精简）

纳入（复用 `G:\work\seatunnel` 源码，按需裁剪）：
- `seatunnel-api`、`seatunnel-engine/engine-core`（master/worker 运行时）、`engine-server`（REST/web）、`engine-client`、`engine-common`、`engine-storage`（checkpoint）。
- 连接器（两种取向按交集裁）：
  - 客户端/同步首期 6 库：mysql / postgres / sqlserver（JDBC）、mongodb / redis / elasticsearch 对应 connector；
  - 加通用 sink：console、local file（文件数据源）、fake source（测试）。
- **明确裁剪掉**：引擎自带的 web UI（`engine-ui`，控制面已统一）、reload 相关、无关 connector、示例 job。
- 保留多模块 Maven 工程，`pom.xml` 锁 2.3.12 定制分支；构建产物打入统一镜像（`deploy/docker/`）。
