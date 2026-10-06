# dbloom 目标态设计 · 总览（v1.1）

> 本文档是 dbloom 的**目标态完整设计**（决策已由用户逐项拍板，见 §2 决策清单）。
> v1.1（2026-10-06）：**D10 变更** —— 元数据由 SQLite 改为**外部关系数据库（MySQL/PostgreSQL，经 ORM 层适配）**；
> 新增 **D26（自定义 jar 权限）**：普通用户可各自上传自己的 jar（按用户隔离）。
> v1.2（2026-10-06）：**D2 修正** —— 引擎由「源码裁剪纳入」改为「**官方发布包原样引入**、零改写」；
> 新增 **D27（客户端引擎复用 dbx）** —— 不再自研/重写已验证的数据库客户端能力，直接依赖 dbx crate。
> v1.3（2026-10-06）：**源码复用原则定为最高优先级**（用户定稿）—— seatunnel/dbx 源码**复制进本仓**
> （`seatunnel/` 与 `dbx/`），各自身擅长的领域，**用不到就删、不合适就改源码，禁止从头重写**。
> D2 由「原样引入」校准为「**源码复制进仓 + 复用为主（删用不到/改不合适）**」；D27 由「依赖 dbx」校准为「**dbx 源码 vendored 进仓**」。
> 配套文档：`01-data-model.md`（数据模型 MySQL/PG ORM）、`02-api.md`（REST/OpenAPI）、
> `03-modules.md`（代码结构）、`04-security.md`（安全）、`05-deploy.md`（部署）、`06-milestones.md`（里程碑）。
> 背景与历史决策记录见 `../architecture.md`（v0.6 草案，作为来源依据）。

---

## 1. 产品一句话

**一站式数据融合平台**：在同一个平台上既能像数据库客户端一样连接、查询、浏览、编辑各种数据库，
又能把同一条连接配置直接生成 SeaTunnel 数据同步任务（批量/增量/CDC），并对任务做全生命周期管理与调度。

核心价值：**同一条「连接配置」既用于客户端操作、又用于同步任务**，一套配置、两种使用。

---

## 2. 决策清单（全部经用户拍板，2026-10-06；v1.1 修订 D10 / 新增 D26）

| # | 决策点 | 结论 |
| --- | --- | --- |
| D1 | 总体架构 | 沿用 SeaTunnel **master + worker** 模式；部署形态 docker / docker compose / k8s；**不部署 reload 服务** |
| D2 | 引擎融合 | **源码复制进仓 + 复用为主**（v1.3 校准）：seatunnel 源码复制进 `seatunnel/`（Maven 工程，pom 锁 2.3.12），作为引擎运行时与客户端一侧融合；统一镜像内 COPY `seatunnel/` 产物（+ JVM）+ 控制面二进制双进程；`dbloom-sync` 做 **HOCON 生成 + REST(8080) 调度**（submit/status/cancel）；**用不到就删（reload/engine-ui/无关 connector）、不合适就改源码**，随官方升级评估合入；不依赖 `apache/seatunnel` 官方镜像（自建统一镜像） |
| D3 | 控制面技术栈 | **Rust**（dbloom-server）+ React(Vite)+antd 前端 + SeaTunnel(Java) 执行面 |
| D4 | 同步任务类型 | 批量 / 增量 / CDC —— **全部透传 SeaTunnel 原生能力**，dbloom 只做编排、调度与 HOCON 生成，不改自研断点 |
| D5 | 任务管理深度 | 完整档：调度(内置 cron)、增量断点(透传 ST)、失败重试、任务 DAG、告警、血缘/审计 |
| D6 | 客户端写边界 | **读+写**；UPDATE/DELETE/DDL/行编辑需**醒目二次确认**才放行；连接可标「生产」并一键只读锁；普通用户无 DDL 权限 |
| D7 | 首批数据库 | 对齐 dbx 现有 6 库：mysql / postgres / sqlserver / mongodb / redis / elasticsearch |
| D8 | 用户体系 | 内置 1 管理员；其余均普通用户；**仅管理员后台创建**；用户间**数据隔离**；管理员可查看/管理所有用户及其数据 |
| D9 | OpenAPI & API Key | dbloom 通过 OpenAPI 对外服务；**仅管理员签发** Key 绑普通用户；可配生效状态/生效时间/失效时间；**完整开放不分作用域**；**简单生命周期**（到期失效、撤销立即生效、暂不设数量上限） |
| D10 | 持久化 | 元数据用**外部关系数据库**（默认 **MySQL**，PostgreSQL 可切换），经 **ORM 层**适配（预估 SeaORM/sqlx，M0 锁定）；非结构化数据（日志/上传下载文件/自定义 jar/checkpoint）走 **docker volume / k8s PVC**，**多节点共享同一网络存储** |
| D11 | 引擎角色落点 | **master 与 dbloom-server 同节点**；worker 独立扩展（最小部署=1 master 节点） |
| D12 | 前端形态 | 纯 Web（浏览器访问），非桌面壳 |
| D13 | CLI | **不需要** CLI（Web + OpenAPI 两入口） |
| D14 | License | **Apache-2.0** |
| D15 | HTTPS/TLS | **dbloom 只出 HTTP**；TLS 终止由外部域名/网关/Ingress 负责，编排不内置 nginx |
| D16 | 告警渠道 | **邮件 SMTP + Webhook（通用 HTTP 回调）** |
| D17 | 统一转发 | **所有数据库 IO 一律由 dbloom-server 代执行**——用户连接地址可能是集群内部 service 名，浏览器访问不通；server 在集群内可达；浏览器永不直连数据库（不留直连通道） |
| D18 | 查询限制 | 单次 SELECT 默认上限 5000 行（可调）、查询超时 120s、大结果集分页/导出 |
| D19 | 会话安全 | JWT access 2h + refresh 7d；密码 argon2；登录失败 5 次/5min 锁定 |
| D20 | 管理员密码 | 部署时随机生成打印日志 + 首登强制改密；**提供后台运维强制重置密码能力**（防遗忘） |
| D21 | 审计粒度 | 登录/登出、连接 CRUD+测试、SQL 执行（含写）、任务 CRUD/启停/重试、API Key 签发/停用、调度变更，全落审计 |
| D22 | 日志/历史保留 | 应用日志滚动 14 天；任务历史默认 90 天（可配置） |
| D23 | 导出格式 | **CSV + XLSX + JSON + SQL** |
| D24 | 界面语言 | 中文优先，预留 i18n |
| D25 | UI 组件库 | **Ant Design（antd）** |
| D26 | 自定义 jar 权限 | **普通用户可各自上传自己的自定义 jar**（按用户数据隔离，D8），供 SeaTunnel 自定义 source/sink/transform 使用；仅本人任务可引用本人 jar；管理员可审计、可停用用户 |
| D27 | 客户端引擎复用 | **不重写已验证客户端能力**：查询/元数据/行编辑/写保护(D6)/导出 复用 **dbx 开源 Rust crate 族**（Apache-2.0，mysql/postgres/sqlserver/mongodb/redis/elasticsearch 等驱动 + `dbx-sql-*`/`dbx-formats` 已场景验证）；**v1.3 校准：dbx crate 源码 vendored 复制进本仓 `dbx/`，经 Cargo path 依赖引入**，直接改源码适配 |`dbloom-connector` 只做 **dbloom DTO ↔ `dbx ConnectionConfig` 薄映射**的薄包装；连接类型清单对齐 dbx `database_manifest`（不另起炉灶） |

---

## 3. 非目标（明确的「不做」）

- 不做**浏览器直连数据库**——所有数据库访问经 dbloom-server 统一转发（D17）。
- 不部署 SeaTunnel **reload** 服务（D1）。
- 元数据不用**嵌入式库**（SQLite 等）——落在通用外部关系库（默认 MySQL，PG 可切换），经 ORM 层适配（v1.1/D10 变更）。
- 不引入**独立调度系统**——内置 cron 调度器（D5）；多副本调度靠元数据库抢锁防重。
- 不做**SSO/OAuth/第三方登录**（无需求，预留扩展位）。
- 不做自研**行级增量断点/CDC**逻辑——透传 SeaTunnel 原生能力（D4）。
- 不提供 CLI（D13），不内置 TLS 网关（D15）。
- 一期不开放**自助注册**（D8）。

---

## 4. 总体架构

### 4.1 组件拓扑

```text
                    ┌──────────────────────────────────────────┐
                    │            用户浏览器（前端）              │
                    │  React + Vite + antd（纯 Web，不直连 DB） │
                    └────────────────┬─────────────────────────┘
                                     │ HTTP(S)【TLS 由外部网关终止 D15】
                                     │ REST  /  WebSocket（SQL 流式、任务日志流）
                    ┌────────────────▼─────────────────────────┐
                    │        dbloom-server（Rust 控制面）       │
                    │  · HTTP 层(axum) + OpenAPI 契约          │
                    │  · 认证/授权：JWT + API Key + 多租户过滤  │
                    │  · 用户/连接/任务/告警/审计 业务模块      │
                    │  · 数据库客户端引擎（D17 统一转发执行）   │
                    │  · SeaTunnel 编排（HOCON 生成/提交/状态） │
                    │  · 内置 cron 调度器 + 告警引擎           │
                    │  · 凭据加密（主密钥文件）                │
                    └───┬───────────────┬──────────────────────┘
                        │ 本地 REST(8080)│  同镜像·同一节点【D11】
        ┌───────────────▼───────┐       │
        │ SeaTunnel Engine(master)│◄─────┘
        │ Hazelcast 5801 +        │
        │ HTTP 8080                │
        └───────────┬─────────────┘
                    │ 集群内通信（Hazelcast 5802 等）
        ┌───────────▼─────────────┐  可水平扩展【D1/D11】
        │ SeaTunnel Engine(worker)│（=N 个）
        └───────────┬─────────────┘
                    │
     ┌──────────────▼─────────────────────────────────┐
     │  外部元数据库（MySQL/PG，ORM，D10）              │
     │  users/connections/tasks/audit/…               │
     └──────────────┬─────────────────────────────────┘
     ┌──────────────▼─────────────────────────────────┐
     │  共享文件存储（dbloom-data，多节点同一份，D10）   │
     │  logs/  files/  plugins/custom-jar/  checkpoint/ │
     └─────────────────────────────────────────────────┘
                    │ 统一转发（D17）
        ┌───────────▼───────────────────────────────┐
        │  用户配置的外部数据库（可能是集群内部 service 名）│
        │  mysql-service / postgres-service / ……      │
        └─────────────────────────────────────────────┘
```

### 4.2 组件职责

| 组件 | 职责 | 运行时 |
| --- | --- | --- |
| **前端** `apps/web` | 登录、连接管理、SQL 工作台、数据浏览/编辑、同步任务管理、调度、告警配置、审计、用户/API Key 管理(admin) | 浏览器，静态资源由 dbloom-server 托管或独立静态服务 |
| **dbloom-server** | 一切业务逻辑与数据库 IO（D17） | Rust 进程，与引擎 master **同节点**（D11） |
| **SeaTunnel Engine(master)** | 引擎 master：接收/分配任务 | JVM，`seatunnel-cluster.sh -r master`，5801/8080 |
| **SeaTunnel Engine(worker)** | 执行同步任务 | JVM，`-r worker`，5802，水平扩展 |
| **元数据库** | 用户/连接/任务/审计 元数据（外部 MySQL/PG + ORM，D10） | MySQL/PG 独立于共享卷，可多副本 |
| **共享文件卷** | 日志 + 上传下载文件 + 自定义 jar + checkpoint，全节点同一份 | docker volume / NFS / k8s RWX PVC |

### 4.3 设计铁律（不可违背）

1. **统一转发**（D17）：浏览器 → dbloom-server → 数据库。任何 SQL/元数据/行编辑/导出都不允许浏览器直连。
2. **多节点共享**（D10）：日志、上传下载文件、自定义 jar、checkpoint **永远挂同一共享卷**，各节点看到同一份数据。
3. **元数据库用外部关系库**（MySQL/PG + ORM，D10）：dbloom-server 可多副本并行写（外部库行锁/事务保证一致性）；调度防重靠**元数据库唯一约束/行锁**（`scheduler_jobs` 抢锁 CAS），不依赖单写实例（`04-security.md` §4 与 `05-deploy.md` §3）。
4. **租户过滤在数据访问层强制**：任何业务代码不得绕过 `owner_user_id` 过滤（`01-data-model.md` / `04-security.md`）。
5. **同步语义透传**（D4）：dbloom 不实现 ETL 语义，HOCON 直接生成 SeaTunnel 原生配置（含 CDC/增量），由引擎执行。
6. **源码复用（最高优先级，v1.3）**：seatunnel/dbx 已验证能力**禁止从头重写**；源码复制进本仓（`seatunnel/`、`dbx/`），
   用不到就删、不合适就改源码；dbloom 只自研融合层（控制面/租户/鉴权/manifest/HOCON/编排/前端）。

---

## 5. 运行时核心流程

### 5.1 登录（人机）
1. 用户 POST `/api/v1/auth/login`（用户名+密码）。
2. 校验密码（argon2）→ 校验账号状态（未锁定/未禁用）→ 记录审计。
3. 签发 access JWT（2h）+ refresh token（7d，落库）。前端存内存 + 落 refresh 到 HttpOnly cookie 或 localStorage（见 `04-security.md`）。

### 5.2 SQL 查询（统一转发）
1. 前端选择**连接**，在 SQL 工作台输入 SQL，经 WebSocket/REST 发到 dbloom-server。
2. 服务端按 `owner_user_id` 校验连接可见性 → 从连接池取连接 → 执行。
3. SELECT：默认上限 5000 行、120s 超时、流式分页；非 SELECT：危险操作二次确认标记校验（D6）。
4. 结果集流式返回；写操作与危险 SQL 落审计；结果可导出 CSV/XLSX/JSON/SQL（D23）。

### 5.3 同步任务创建与运行
1. 用户选**源连接 / 目标连接** + 表映射 + 同步类型（全量/增量/CDC）+ 调度 cron（可选）。
2. dbloom-server 依据「连接类型 manifest」（`03-modules.md` §3）把连接参数 + 用户配置**渲染成 SeaTunnel HOCON**。
3. 手动触发或 cron 到点 → 经引擎 REST(8080) 提交 job → 记录 `task_runs`、`sea_tunnel_job_id`。
4. 轮询/接收 job 状态并回写；日志从共享卷 `logs/` tail 经 WebSocket 推给前端；失败走告警引擎（SMTP/Webhook，D16）。

### 5.4 API Key（机器访问）
1. 管理员为普通用户签发 Key（哈希存储，明文仅展示一次）→ 用户外部系统持 `Authorization: Bearer <key>` 调 OpenAPI。
2. 鉴权：查 Key → `status=enabled` 且 现在 ∈ [valid_from, valid_until] → 绑定用户有效 → 以该用户身份进入租户过滤。
3. 每次调用记审计（调用方/目标/结果，D21）。

### 5.5 部署（三形态）
docker（本地单容器）：master 角色容器 = 控制面 + 引擎 master；worker 角色容器可另起；元数据库可连接外部 MySQL/PG 或用 compose 一并编排。
compose：`dbloom-db`（可选 MySQL/PG，或连外部实例）+ `dbloom-master` / `dbloom-worker`（scale=N）共享同一 `dbloom-data` 卷（多机用 NFS 卷驱动）。
k8s：元数据库（StatefulSet MySQL/PG 或外部托管实例）+ Deployment master / worker + **一个 RWX PVC** 全挂载 + ConfigMap + Secret + Service + (可选 worker HPA)。
细节见 `05-deploy.md`。

---

## 6. 代码结构总览（详见 `03-modules.md`）

```text
dbloom/
├─ docs/design/            # 本套目标态设计文档
├─ apps/web/               # React + Vite + antd 前端
├─ seatunnel/             # seatunnel 源码复制区（复用为主：删用不到/改不合适，D2/D24a）
├─ crates/
│  ├─ dbloom-server/       # HTTP+WS 装配、路由层
│  ├─ dbloom-iam/          # 用户/认证/API Key/多租户过滤
│  ├─ dbloom-connector/    # 数据库客户端引擎（D17 统一转发执行）
│  ├─ dbloom-sync/         # SeaTunnel 编排/调度/告警
│  ├─ dbloom-storage/      # ORM 访问层（MySQL/PG 迁移 + DAO + 租户过滤）
│  ├─ dbloom-types/        # DTO / 连接 manifest / 任务模型
│  └─ dbloom-common/       # 配置/日志/加密/错误
├─ deploy/                 # docker / compose / kubernetes
└─ …（README / AGENTS / docs/architecture.md）
```

---

## 7. 关键技术依赖清单（实现期直接采购）

| 域 | 选型 | 说明 |
| --- | --- | --- |
| Web 框架 | **axum**（Rust） | tokio 生态、WebSocket/OpenAPI 支持好 |
| ORM / 元数据库 | **SeaORM（基于 sqlx，预估）** + MySQL（默认）/ PostgreSQL（可切） | D10；M0 锁定：SeaORM 或等价异步 ORM，多方言迁移 |
| 迁移 | **SQLx migrate 或 SeaORM Migration** | 版本化 schema 迁移（MySQL/PG 通用） |
| 密码哈希 | **argon2** crate | D19 |
| JWT | **jsonwebtoken** crate | access + refresh |
| 加密 | **AES-256-GCM**（`aes-gcm` crate）+ 随机盐 | 连接凭据落库加密，主密钥来自 `DBLOOM_SECRET_KEY_FILE` |
| cron 调度 | **cron** crate（tokio 版） | 内置调度器（D5；多副本抢锁） |
| 邮件 | **lettre**（SMTP） | D16 告警邮件 |
| Webhook | reqwest + 签名头 | D16 通用回调 + 安全签名 |
| HOCON 生成 | 模板 + serde（或基于 HOCON crate） | 连接 manifest → ST 配置渲染（D4） |
| 前端 | React + Vite + **antd** + TanStack Query + zustand | D12/D25 |
| 图表 | antd charts / echarts（任务血缘 DAG 可视化） | 完整档 D5 |
| OpenAPI | utoipa（Rust 侧）→ 生成 openapi.json + swagger UI | D9 |

> 注：依赖版本号在 M0 落地时锁定（Cargo.toml）。
