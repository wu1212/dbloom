# dbloom 架构设计草案（v0.7）

> 状态：草案（背景与决策记录）。目标态设计见 **`docs/design/00-overview.md`（v1.1）** 及其配套 01–06。
> 带 ✅ 的为已确认/已拍板方向。
> v0.2（2026-10-06）：新增用户体系 / 数据隔离 / OpenAPI + API Key 需求。
> v0.3（2026-10-06）：持久化方案确认 —— 元数据用 SQLite，持久化走 docker volume / k8s PVC，不引入外部存储数据库。（⚠️ 已被 v0.7 重置）
> v0.4（2026-10-06）：多节点持久化确认 —— 多节点共享存储（被 v0.7 部分重置：非结构化数据仍共享，元数据改外部库）。
> v0.5（2026-10-06）：**决策拍板** —— ① 同步引擎融合方式 = **完整内置引擎**（SeaTunnel Engine 源码纳入本仓，统一镜像、控制面与引擎同镜像部署，不再依赖官方 apache/seatunnel 镜像）；② OpenAPI API Key 粒度 = **完整开放、不分作用域**（Key 即身份凭证，鉴权通过即拥有所绑用户全部能力）。
> v0.6（2026-10-06）：**第二批决策拍板** —— ① 数据库客户端首批覆盖 = 对齐 dbx 现有 6 库（mysql/postgres/sqlserver/mongodb/redis/elasticsearch）；② 任务管理深度 = **完整档**（调度定时 / 增量断点 / 重试 / 任务 DAG / 告警 / 血缘审计）；③ master 与 dbloom-server **同节点**（master 上跑控制面 + 引擎 master，worker 独立扩展）；④ 普通用户**仅管理员创建**（无自助注册）；⑤ API Key 生命周期 = **简单策略**（到期即失效、撤销立即生效、暂不设数量上限）。
> **v0.7（2026-10-06，重置 D10）**：**SQLite 方案作废** —— 元数据改用**外部关系数据库（默认 MySQL，PostgreSQL 可切换）+ ORM 层适配**（设计见 `docs/design/01-data-model.md`、`05-deploy.md`）；共享卷只保留非结构化数据（日志 / 上传下载文件 / 自定义 jar / checkpoint）。dbloom-server **无状态可多副本**，不再有「单写者 / 只读副本」概念。另拍板 **D26 = 自定义 jar 普通用户各自上传、按用户隔离**（见 `docs/design/04-security.md` §5.1）。本草案下文凡与 v0.7 冲突处，一律以 **v1.1 design 卷** 为准。
> 技术栈选型已确认：**Rust 控制面 + React 前端 + SeaTunnel 引擎（Java）**。
> 后续决策（写保护/同步类型透传/告警渠道/antd/无 CLI/Apache-2.0/引擎源码复用/HTTP only/统一转发/安全默认值等 26 项 + 源码复用原则 D24a）与目标态设计
> 见 **`docs/design/00-overview.md` §2 决策清单**（v1.3）。

---

## 1. 产品定位

**一站式数据融合平台**：把「数据库客户端」与「数据同步」两种能力合并进同一个产品。

一个平台里同时做到两件事：

1. **数据库客户端**：在页面上配置各种数据库连接，然后像数据库客户端一样使用——
   连库、看表结构、写 SQL、查数据、浏览/编辑数据。
2. **数据同步**：把**已配置好的数据库连接**直接复用为 SeaTunnel 同步任务的源/目标，
   在页面上创建、提交、管理 SeaTunnel 同步任务（全量/增量/实时），并查看运行状态与日志。

核心价值：**同一条「连接配置」既用于客户端操作、又用于同步任务**，不再两套系统各配一遍。

---

## 2. 已确认需求（用户原话要点）

- ✅ **整体架构沿用 SeaTunnel 的 master + worker 模式**。
- ✅ 支持 **docker、docker compose、k8s** 三种部署方式。
- ✅ **不需要 SeaTunnel 的 reload 服务**（部署与运行时裁剪掉它）。
- ✅ 页面可配置各种数据库连接，作为**数据库客户端**使用。
- ✅ 配置好的连接信息可直接**创建 SeaTunnel 任务**，并可**管理 SeaTunnel 任务**（任务生命周期管理）。
- ✅ 需要**用户登录**：内置 1 个管理员账户，其余均为普通用户；**用户之间数据隔离**，管理员可查看、管理其它用户及其数据。
- ✅ dbloom 通过 **OpenAPI 接口对外提供服务**；管理员为普通用户分配 **API Key**（可设置生效状态、生效时间、失效时间）。
- ✅ **持久化（v0.7 重置 D10）**：元数据用**外部关系数据库**（默认 MySQL，PostgreSQL 可切换）+ **ORM 层**适配；非结构化数据（日志、上传/下载文件、自定义 jar、checkpoint）走 docker volume / k8s PVC。
- ✅ **多节点共享**：无论部署几个节点，**日志、上传/下载文件、自定义 jar 包目录等非结构化数据挂载同一共享网络存储**，各节点访问同一份数据（2026-10-06 确认）；元数据经 DB 天然共享（2026-10-06 v0.7 定稿）。
- ✅ **完整内置引擎**（2026-10-06 拍板）：SeaTunnel Engine 源码工程纳入 dbloom 仓库统一维护并打出**统一镜像**；控制面与引擎同一镜像部署（单镜像·双进程），Rust 控制面经 SeaTunnel REST 客户端提交/管理任务；不再依赖官方 `apache/seatunnel` 镜像，也**不部署 reload 服务**。
- ✅ **OpenAPI API Key 粒度 = 完整开放、不分作用域**（2026-10-06 拍板）：Key 是纯身份凭证，鉴权通过即代表所绑用户的全部能力（连接管理 / SQL 查询 / 任务提交与管理），不细分 scope；保留 `status` / `valid_from` / `valid_until` 生命周期配置。
- ✅ **数据库客户端首批覆盖 = 对齐 dbx 现有 6 库**（2026-10-06 拍板）：mysql / postgres / sqlserver / mongodb / redis / elasticsearch；同步侧连接数视 SeaTunnel connector 交集而定。
- ✅ **任务管理 = 完整档**（2026-10-06 拍板）：含调度/定时、增量断点续跑、失败重试、任务依赖 DAG、告警通知、血缘/审计。
- ✅ **master 与 dbloom-server 同节点**（2026-10-06 拍板）：master 节点同时跑 Rust 控制面与引擎 master，worker 独立扩展。
- ✅ **普通用户仅管理员创建**（2026-10-06 拍板）：后台创建、无自助注册入口。
- ✅ **API Key 生命周期 = 简单策略**（2026-10-06 拍板）：到期即失效、撤销立即全局生效、暂不设数量上限（保留扩展余地）。

---

## 3. 参考项目调研结论（架构来源依据）

### 3.1 SeaTunnel Engine（`seatunnel/`，源：`G:\work\seatunnel`）—— 数据同步引擎

| 项 | 结论（来源文件） |
| --- | --- |
| 集群形态 | **Hazelcast 自治集群**：无需指定 master，节点按 `cluster-name` 自动组网、自动选主，master 挂掉自动重选（`seatunnel-engine/README.md`） |
| 角色与端口 | master 暴露 hazelcast `5801` + HTTP `8080`（REST + Web UI）；worker 在 `5802`（`config/hazelcast-master.yaml` / `hazelcast-worker.yaml`） |
| 启动命令 | `bin/seatunnel-cluster.sh -r master` / `-r worker`（k8s deployment 模板里直接写） |
| 任务配置 | HOCON：`env { } source { } sink { }` 三段式（`config/v2.batch.config.template`） |
| HTTP 能力 | `seatunnel.yaml`：`http.enable-http: true, port: 8080`，支持 REST API、可选 basic-auth、`/upload-file` 上传数据文件与自定义 jar |
| 存储 | 本地 checkpoint（`/tmp/seatunnel/checkpoint_snapshot`）；**多副本部署要求共享卷**（注释明示）；`history-job-expire-minutes: 1440` |
| k8s | 自带 Helm chart：`deployment-seatunnel-master` / `-worker` + configmap + headless service + ingress + rbac，master/worker replicas 可配（`deploy/kubernetes/seatunnel/`） |
| 模块 | `seatunnel-engine/`：engine-core / engine-server / engine-client / engine-common / engine-storage / engine-ui |

### 3.2 dbx（`dbx/`，源：`G:\work\dbx`）—— 数据库客户端

| 项 | 结论（来源文件） |
| --- | --- |
| 技术栈 | Rust workspace + React(Vite) 前端 + Tauri 桌面壳；服务形态为 `dbx-web`（HTTP `4224` 端口，另提供 `/mcp`） |
| 模块分层 | `dbx-core`（应用编排）→ `dbx-drivers`（原生驱动/JDBC/Agent）→ `dbx-sql`（解析/方言/SQL 风险）→ `dbx-types`（DTO）；另有 `dbx-formats`（CSV/XLSX 导入导出）、`dbx-ai-provider`、`dbx-plugin-runtime`、`dbx-platform`（见 `crates/ARCHITECTURE.md`，依赖单向：core 依赖下层，下层不得反向依赖 core） |
| 核心业务目录 | core/`connection`（连接路由/凭据）、`query`（查询编排/取消/事务）、`schema`（元数据）、`data`（导入导出/迁移/备份）、`safety`（生产安全/写锁） |
| 连接类型 | **插件化**：`plugins/connection-types/` 定义连接类型 manifest，`plugins/drivers/`、`plugins/jdbc/` 提供连接实现，`build.rs` 消费后生成 TS 类型（前端直接消费同一份 manifest） |
| SQL 方言 | `plugins/dialects/` 插件化注册，前端补全/元数据也走同一来源 |
| 部署 | 单容器镜像（多阶段：前端构建 + Rust 后端，`deploy/Dockerfile`）；`docker-compose.yml` 挂 `dbx-data`、`dbx-backups` 卷，`DBX_PASSWORD` / `DBX_SECRET_KEY_FILE` 等环境变量 |
| 数据库支持 | mysql / postgres / sqlserver / mongodb / redis / elasticsearch 各有独立 driver crate + JDBC/Agent 桥 |

---

## 4. 融合架构（主推方案）

### 4.1 总体拓扑

```text
┌─────────────────────────────── 用户浏览器 ───────────────────────────────┐
│                          dbloom Web UI（React）                          │
└──────────────┬──────────────────────────────────┬─────────────────────┘
               │ HTTP / WebSocket                   │
┌──────────────▼──────────────────────────────────▼─────────────────────┐
│                       统一镜像 dbloom（单镜像 · 双进程）               │
│  ┌─────────────────────────────┐     ┌──────────────────────────────┐  │
│  │  dbloom-server（Rust 控制面）│     │  SeaTunnel Engine（JVM）     │  │
│  │  · 连接管理/连接池/凭据加密   │     │  · master / worker 角色      │  │
│  │  · 数据库客户端执行          │     │  · 执行同步任务             │  │
│  │  · 同步编排：HOCON 模板生成 → │◀───▶│    （本地 REST 通信）        │  │
│  │    REST 客户端/任务生命周期   │     └──────────────┬───────────────┘  │
│  └──────────────┬──────────────┘                    │                  │
└─────────────────┼────────────────────────────────────┼─────────────────┘
                  │                                    │
     ┌────────────▼────────────┐        ┌──────────────▼──────────────┐
     │ 元数据（外部 MySQL/PG）  │        │ checkpoint / 上传下载文件 /  │
     │ 连接/任务/历史/审计/用户  │◀──────▶│ 自定义 jar（按用户隔离 D26） │
     │ （ORM 层，无状态多副本）  │        │ （同一共享卷，所有节点同一份）│
     └─────────────────────────┘        └──────────────────────────────┘
```

### 4.2 角色说明

- **统一镜像（单镜像 · 双进程）**：Rust 控制面与 JVM 引擎在**同一镜像内**，由镜像 entrypoint/进程管理器按角色拉起（引擎不可被 Rust 进程真正内嵌，故同镜像双进程）。
- **dbloom-server（控制面进程）**：负责
  - 连接管理：连接 CRUD、连接测试、凭据加密存储、连接池
  - 数据库客户端执行：SQL 查询、表结构与元数据、数据浏览（沿用 dbx 的驱动层能力）
  - 同步编排：把「已配置连接」组装成 SeaTunnel HOCON 任务 → 经 SeaTunnel REST（本地 8080）提交 → 轮询状态 → 持久化任务历史与日志
- **dbloom Web UI**：连接配置页、SQL 工作台、同步任务创建/管理页、集群与任务状态可视化
- **SeaTunnel Engine（执行进程）**：JVM；master 角色（Hazelcast 5801 + HTTP 8080）与 worker 角色（5802）来自同一镜像，按 `seatunnel-cluster.sh -r master/worker` 启动；不部署 reload 服务

### 4.3 关键设计点

1. **同一条连接配置两用**：连接实体（dbloom 侧）与 Seatunnel source/sink 配置映射。
   连接类型注册表（借鉴 dbx `plugins/connection-types` 的 manifest 思路）作为**单一事实来源**——
   前端表单、客户端驱动参数、以及 SeaTunnel HOCON 生成的字段都从它推导，保证「配一次，客户端和同步都能用」。
2. **任务编排层**：dbloom 负责把连接 + 用户填的 源/目标/表映射/过滤/并行度/checkpoint 等参数渲染成
   SeaTunnel 的 HOCON 配置文本，通过 SeaTunnel REST API 提交。任务 CRUD/状态/日志在 dbloom 侧持久化，
   与 SeaTunnel 内部 job 通过 `jobId` 关联。
3. **master+worker 语义**：沿用 SeaTunnel 集群拓扑，但 master 与 worker 都来自**同一统一镜像**、按启动角色区分：
   - **master 角色**：控制面 + 引擎 master（`-r master`，Hazelcast 5801 + HTTP 8080）
     —— ✅ 已拍板（2026-10-06）：**控制面与引擎 master 部署在同一节点**（同一 master 服务），worker 独立扩展。
   - **worker 角色**：引擎 worker（`-r worker`，5802），可水平扩展
   - dbloom-server 控制面**无状态，可多副本并行写**（元数据走外部库，v0.7，见 §5.6）。
4. **存储分离（v0.7）**：任务元数据/连接配置/审计 → **外部 MySQL/PG（ORM）**；日志、上传/下载文件、自定义 jar、
   SeaTunnel checkpoint 全部落在**同一共享卷**——多节点部署时共享卷只放非结构化数据、由所有节点共享同一份
   （参考 seatunnel.yaml 注释的共享卷要求，见 §5.6 / §7）；元数据经外部库天然多节点共享。
5. **部署形态三选一**（见 §7），reload 服务一律不部署。

### 4.4 备选方案

- **方案 B（全 Java 控制面）**：dbloom-server 用 Java/Spring + 直接内嵌 SeaTunnel 客户端库，
  不再单独起 Rust 服务。优点：与 SeaTunnel 同生态、HOCON 生成/提交天然贴合；缺点：数据库客户端能力
  需用 JDBC 重写一遍（放弃 dbx 成熟的 Rust 驱动/SQL 层）。
- **方案 C（前端直连）**：客户端查询由浏览器 → dbloom-server 代理执行（主推）；不做浏览器直连数据库（安全与兼容性差）。

主推：**Rust 控制面 + 前端 React + SeaTunnel 引擎（Java）执行面**。✅ 已由用户确认（2026-10-06）。

### 4.5 引擎融合方式：完整内置引擎（✅ 已拍板 2026-10-06）

- **决策**：SeaTunnel Engine **不外部依赖、不采用独立官方发行版**，而是把引擎源码工程（本地
  定制版，已复制进仓 `seatunnel/`，源：`G:\work\seatunnel` 2.3.12）纳入 dbloom 仓库统一维护，构建出**统一镜像**——一个镜像内同时
  包含 Rust 控制面与 JVM 引擎。
- **进程模型**：控制面与引擎为**同镜像双进程**（JVM 无法被 Rust 进程真内嵌），由镜像 entrypoint /
  进程管理器按角色拉起；两者通过本地 REST（引擎 8080）+ 共享卷交换数据。
- **收益**：部署对象从「dbloom + 官方 seatunnel 两套镜像」收敛为**一套 dbloom 镜像**；引擎版本锁定在
  2.3.12 定制分支、升级与补丁同步、无 reload 服务、共享卷语义统一（对标用户「不要 reload / 多节点数据
  全共享」的取舍）。
- **约束（如实记录）**：镜像体积因含 JVM 而较大；引擎升级跟随本地定制分支（不追官方任意版本）。

---

## 5. 用户体系、认证与 OpenAPI / API Key

> 新增需求（2026-10-06）。**页面与 OpenAPI 两条访问路径统一经 dbloom-server 认证与授权**。

### 5.1 用户与角色
- **内置管理员**：系统初始化时建立（固定账号种子），不可删除、不可降级为普通用户；
  拥有**超级租户视角**：可查看、管理所有用户及其数据。
- **普通用户**：其余用户均为普通用户；只能访问**自己名下**的数据。
  - ✅ **仅由管理员在后台创建**（2026-10-06 拍板），系统**不开放自助注册入口**。
- 密码用强哈希（argon2/bcrypt）存储；支持管理员/新用户强制改初始密码。

### 5.2 数据隔离（多租户）
- 所有业务实体（连接配置、任务、任务历史、API Key、审计…）带 `owner_user_id`；
  **数据访问层强制按租户过滤**：普通用户查询恒附加 `owner = 当前用户`；
  管理员可显式指定目标 `user_id` 跨用户查看/管理。
- 隔离在服务端数据访问层强制（防越权拿数据），不依赖前端隐藏控件。

### 5.3 认证
- 人机（Web UI）：登录拿 JWT（access + refresh），角色放 JWT 声明中。
- 机器（OpenAPI）：用 **API Key**（见 5.4），外部系统持 Bearer 调用 dbloom 的 OpenAPI 接口。

### 5.4 API Key（OpenAPI 访问凭证）
- **仅管理员可签发**，绑定到指定普通用户；一个用户可有多个 Key（不同用途/项目）。
- 可配置：`status`（启用/停用）、`valid_from`（生效时间）、`valid_until`（失效时间）；
  扩展能力预留但**默认不启用**：scope 权限细分、限流配额、来源 IP 白名单。
- **生命周期策略（✅ 拍板 2026-10-06）**：**简单策略**——`valid_until` 到期即失效（过期后直接拒绝）、
  管理员撤销**立即全局生效**（无缓存/宽限窗口）；**暂不设数量上限**、不自动续期、不做到期提醒
  （作为后续可选增强，不影响默认行为）。
- **权限粒度（✅ 已拍板 2026-10-06）**：**完整开放、不分作用域**——Key 是纯身份凭证，鉴权通过后即
  代表所绑用户的**全部能力**（连接管理 / SQL 查询 / 任务提交与管理）；不细分 scope。限流配额、来源
  IP 白名单等作为后续可选增强，不影响默认行为。
- 鉴权流程：查 Key → 校验 `status=enabled` 且 `现在 ∈ [valid_from, valid_until]`
  → 校验绑定用户仍有效 → 以该用户身份进入数据隔离。
- **安全存储**：只存哈希（如 SHA-256），明文仅创建时展示一次；Key 的签发/停用/作废全部记审计；
  每次 API Key 调用记录（调用方、目标资源、结果）供管理员追溯。

### 5.5 落点（对应代码结构）
- 用户/角色/认证（JWT）/API Key 管理集中在 **`dbloom-iam`** crate（见 §6）；
- 数据访问统一走「强制租户过滤」封装，业务 crate 不得绕过。

### 5.6 持久化方案（v0.7 重置：外部元数据库 + 共享文件卷）
> 决策（2026-10-06，v0.7 替换 v0.3/v0.4 的 SQLite 方案）：
> ① **元数据用外部关系数据库**（默认 MySQL，PostgreSQL 可切换），经 **ORM 层**适配；
>    连接方式 `DB_DSN`，可连部署自带并编排的实例或外部托管库（`docs/design/05-deploy.md` §0）。
> ② **多节点必共享文件**：无论部署几个节点，日志、上传/下载文件、自定义 jar、checkpoint 全部落在
>    **同一个共享网络存储**（NFS / CephFS / EFS / 托管共享盘），所有节点看到的是**同一份**数据，
>    绝不是各节点各存一份。元数据经外部库天然共享，不再占用共享卷。

- **统一挂载根 `/dbloom-data`，各节点相同路径**，内容按子目录划分（只含非结构化数据）：

  | 子路径 | 内容 | 谁挂载/访问 |
  | --- | --- | --- |
  | `logs/` | 各服务日志 | dbloom / seaTunnel master / worker |
  | `files/upload/`、`files/download/` | 上传的数据文件、下载暂存 | dbloom / master / worker（任务消费同一份） |
  | `plugins/custom-jar/` | 自定义 connector jar（按用户隔离，D26） | master / worker（SeaTunnel 启动时加载） |
  | `checkpoint/` | SeaTunnel checkpoint / snapshot | master / worker |

- **元数据库**：外部 MySQL/PG（带 ORM 迁移 v1…），dbloom-server **无状态可多副本并行写**；
  调度防重用 `scheduler_jobs.lock_until` 行锁 CAS（`04-security.md` §5）；迁移用 DB 级锁防并发。
- **部署差异**：单机（§7.1）元数据库可同机/外部、共享卷用本地命名卷；**多机 compose / k8s（§7.2/§7.3）共享卷必须挂到
  同一网络共享卷**（compose 用 NFS 卷驱动；k8s 用 **RWX** PVC 并让 master/worker/dbloom 全部挂载，元数据可外部托管）。
- **备份**：**元数据库**用其原生备份（mysqldump / pg_dump / 云快照）+ **共享卷**快照（checkpoint + 文件 + jar 一体）。
- **取舍（如实记录）**：引入外部库依赖与运维（换取多副本并行写、消除 NFS 上嵌入式库的可靠性风险）；
  换取 **元数据可靠、多副本可写、非结构化数据多节点天然同一份**。

---

## 6. dbloom 自身代码结构（规划草案）

> 结构沿用 dbx 的分层与单向依赖原则（core 编排层依赖底层，底层不反向依赖 core）。

```text
dbloom/
├─ docs/                        # 本文档等
├─ apps/
│  └─ web/                      # React + Vite 前端（连接配置/SQL 工作台/任务管理）
├─ seatunnel/                   # SeaTunnel Engine（Java/Maven，本地 2.3.12 定制版）——内嵌执行引擎，纳入本仓统一维护
│  ├─ seatunnel-engine/         #   引擎本体（仓内副本，上游 G:\work\seatunnel 的 seatunnel-engine）
│  ├─ seatunnel-connectors-v2/  #   连接器（首批范围见待确认）
│  └─ pom.xml                   #   版本锁定 2.3.12 定制分支
├─ crates/
│  ├─ dbloom-server/            # 控制面 HTTP 服务（REST + WebSocket）——可运行二进制之一
│  ├─ dbloom-iam/               # 用户/角色/认证(JWT)/API Key 管理、多租户数据隔离过滤
│  ├─ dbloom-connector/         # 数据库客户端：连接池、SQL 执行、元数据、数据浏览 ← 借鉴 dbx 驱动/SQL 层
│  ├─ dbloom-sync/              # SeaTunnel 编排：HOCON 模板生成、REST 客户端、任务状态机
│  ├─ dbloom-types/             # 共享 DTO / 连接类型 manifest / 任务模型
│  ├─ dbloom-storage/           # 元数据库访问（MySQL/PG + ORM 多方言，迁移/DAO/租户过滤）
│  └─ dbloom-common/            # 凭据加密、配置、日志、平台工具
├─ deploy/
│  ├─ docker/                   # Dockerfile（统一镜像：前端 + 控制面 + 引擎）
│  ├─ compose/                  # docker-compose：dbloom-master / dbloom-worker 角色 + 共享卷
│  └─ kubernetes/               # Helm chart（dbloom master / worker，基于官方 chart 改造）
├─ README.md
└─ AGENTS.md
```

> ❓ 若最终决策为 Java 控制面，上述 crate 布局映射为 Maven 多模块（`dbloom-server` /
> `dbloom-connector` / `dbloom-sync` / `dbloom-common`），结构语义不变。

**运行时共享文件目录**（部署时整体挂载到共享卷根，多节点同路径见 §5.6；元数据在外部库，不入此卷）：

```text
<共享卷根>/dbloom-data/
├─ logs/                     各服务日志（dbloom / master / worker）
├─ files/upload|download/    上传数据文件 / 下载暂存
├─ plugins/custom-jar/       自定义 connector jar（按用户隔离 D26，master / worker 加载）
└─ checkpoint/               SeaTunnel checkpoint / snapshot
```

---

## 7. 部署形态（三种）

### 7.1 docker（单机/单容器）
- **统一 `dbloom` 镜像**（前端静态资源 + Rust 控制面 + 内嵌 SeaTunnel Engine；多阶段构建借鉴 dbx，
  JVM 引擎层源自 seatunnel/ 工程）——**不依赖官方 `apache/seatunnel` 镜像**。
- 单机方式：一个容器内按角色启动控制面 + 引擎 master/worker（本地模式：master 内置 worker，✅ 控制面与
  引擎 master 同节点），或 host 直接起进程。
- 卷：`dbloom-data`（命名卷，内部结构同 §5.6：`logs/` + `files/` + `plugins/custom-jar/` + `checkpoint/`）；元数据连外部 MySQL/PG（可同机起 mysql 容器或用 `--link`/compose 编排）。

### 7.2 docker compose（推荐开发/轻量生产）
```text
services:
  dbloom-db      # 元数据库 MySQL/PG（可选）：mysql:8，初始化 dbloom 库；也可去掉改用外部托管 DB（DB_DSN）
  dbloom-master  # 统一镜像：控制面 + 引擎 master（seatunnel-cluster.sh -r master）——✅ 同节点（2026-10-06 拍板）
  dbloom-worker  # 统一镜像：引擎 worker 角色（-r worker，可 scale=N）
volumes:
  dbloom-data:     # 共享文件根：logs / files / custom-jar / checkpoint（路径划分见 §5.6）
  dbloom-db:       # 元数据库数据卷（如连外部托管则无需）
```
- **单机**：`dbloom-data` 用本地命名卷即可，master/worker 全部挂载**同一个 `dbloom-data`**。
- **多机（集群）**：`dbloom-data` 用 **NFS 卷驱动**（`driver_opts: type: nfs ...`）挂在共享网络盘，
  所有节点挂同一共享卷 → 日志、文件、jar、checkpoint 全部共享（即用户强调的「节点间数据一致」）；元数据走外部库（见 §5.6）。
- **不部署 `/reload` 服务**；元数据用外部 MySQL/PG（compose 可加 `dbloom-db` 服务或连外部托管实例，见 `docs/design/05-deploy.md` §2.2）。

### 7.3 k8s（生产）
- 基于 SeaTunnel 官方 Helm chart 改造：保留 `deployment-seatunnel-master` / `-worker`、configmap、
  headless service、ingress 的编排骨架，但镜像换为**统一 `dbloom` 镜像**、按角色区分 master/worker。
- **一个共享 RWX PVC `dbloom-data`**：挂到 dbloom master/worker **全部节点**，
  子目录划分 `logs/` `files/` `plugins/custom-jar/` `checkpoint/`（见 §5.6）——
  任意节点访问的都是同一份日志、文件与 jar 等非结构化数据（即用户强调的「多节点持久化数据共享」）。
- **元数据库**：连外部托管 MySQL/PG（推荐）或 chart 内可选 MySQL StatefulSet（`db.mode=bundled|external`，
  `DB_DSN` 放 Secret）；dbloom-server 无状态、可多副本并行写（无单写约束）。
- 去掉 reload 编排。

---

## 8. 里程碑规划（依赖后续需求细化）

| 阶段 | 内容 | 依赖 |
| --- | --- | --- |
| M0 | workspace 骨架 + 元数据库初始化（ORM MySQL/PG，schema 迁移、租户过滤封装）+ 用户体系（内置管理员、**仅管理员创建用户**）+ JWT 登录 + API Key 管理（**简单生命周期**）| 技术栈确认 |
| M1 | 连接管理 API + 前端空壳 + 连接测试 | M0 |
| M2 | 数据库客户端（**首批 6 库**，对齐 dbx）：SQL 工作台、元数据/表结构、数据浏览 | —— |
| M3 | 引擎纳入（`seatunnel/` 工程构建出可运行内嵌引擎）+ SeaTunnel 集成：HOCON 生成、提交、状态查询 | M0 |
| M4 | 任务管理（**完整档**）：列表/调度定时/增量断点/重试/启停/日志/删除/历史/告警/血缘审计 | —— |
| M5 | 部署三形态（**master 与 dbloom-server 同节点**）：docker / compose / k8s | —— |

---

## 9. 待确认项（已全部拍板，目标态见 docs/design/00-overview.md）

1. ~~技术栈~~ ✅ 已确认：Rust 控制面 + React 前端 + SeaTunnel 引擎（Java）。
2. ~~数据库客户端覆盖范围~~ ✅ 已拍板（2026-10-06）：首批对齐 dbx 现有 6 库（mysql/postgres/sqlserver/
   mongodb/redis/elasticsearch）；同步侧源/目标支持数后续按 SeaTunnel connector 交集补齐。
3. ~~“管理任务”的深度~~ ✅ 已拍板（2026-10-06）：**完整档**——调度/定时、增量断点、失败重试、
   任务依赖 DAG、告警通知、血缘/审计。
4. ~~部署存储选型~~ ✅ 已拍板（2026-10-06，v0.7 重置）：元数据用**外部关系数据库（默认 MySQL，PG 可切）+ ORM 层**；
   **多节点共享文件卷**只放非结构化数据（日志 / 上传下载文件 / 自定义 jar / checkpoint，2026-10-06）。
   ~~master 与 dbloom-server 是否同 node~~ ✅ 已拍板（2026-10-06）：**同节点**。
5. ✅ SeaTunnel 版本与融合：**完整内置**，跟随仓内 `seatunnel/`（上游 `G:\work\seatunnel`）2.3.12 定制分支。
6. ~~API Key 权限粒度~~ ✅ 已拍板（2026-10-06）：**完整开放、不分作用域**——Key 是纯身份凭证，
   鉴权通过即拥有所绑用户全部能力。
7. ~~API Key 生命周期策略~~ ✅ 已拍板（2026-10-06）：**简单策略**——到期即失效、撤销立即全局生效、
   暂不设数量上限；不自动续期/提醒（保留后续增强）。
8. ~~OpenAPI 对外暴露的范围~~ ✅ 随「完整开放」拍板确认：对外接口默认向持 Key 方开放绑用户全部能力
   （连接管理 / SQL 查询 / 任务提交与管理）。
9. ~~普通用户来源~~ ✅ 已拍板（2026-10-06）：**仅管理员在后台创建**，无自助注册。
10. **其余设计项全部拍板完毕**（写边界=读+写+二次确认、同步类型=透传 ST 原生、告警渠道=SMTP+Webhook、
    UI=antd、无 CLI、License=Apache-2.0、引擎源码复用、HTTP only、**统一转发**、查询/会话/密码/审计/保留/
    导出等安全默认值）→ 完整 26 项决策清单 + **源码复用原则（D24a：复制进仓/删/改，禁止重写）** 见 **`docs/design/00-overview.md`**。本草案不再有 ❓ 待确认项。
