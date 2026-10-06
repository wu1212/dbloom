# AGENTS.md — dbloom 项目说明

## 项目定位
将「数据同步功能」与「各种数据库客户端工具」融合为一个项目（`dbloom`）。
- 数据同步能力 ← 参考 `G:\work\seatunnel`（Apache SeaTunnel 2.3.12，Java 8 / Maven 多模块）
- 数据库客户端能力 ← 参考 `G:\work\dbx`（dbx：Rust / Tauri / Cargo workspace，100+ 数据库，AI 助手，MCP Server）

## Git 约定
- 远程仓库：https://github.com/wu1212/dbloom.git（origin）
- 默认分支：main
- 提交规范：中文或英文均可，语义化描述改动

## 参考项目关键信息
### seatunnel（数据同步/ETL）
- 版本 2.3.12，Java 8，Maven 多模块
- 核心模块：seatunnel-engine（任务引擎）、seatunnel-connectors-v2（连接器 v2）、seatunnel-api、seatunnel-transforms-v2
- 本地其他项目引用时优先复制/改造，不直接耦合外部仓库

### dbx（数据库客户端，Rust/Tauri）
- Cargo workspace，主要 crate：
  - `crates/dbx-core` — 核心
  - `crates/dbx-driver-*` — 各数据库驱动（mysql/postgres/redis/mongodb/sqlserver/elasticsearch…）
  - `crates/dbx-sql` / `dbx-sql-dialect` / `dbx-sql-schema` — SQL 解析/方言/Schema
  - `crates/dbx-cli` / `crates/dbx-mcp` — CLI 与 MCP Server
  - `src-tauri` — Tauri 桌面壳
- 前端：pnpm workspace（`packages/`、`apps/`）

## 需求与架构决策（2026-10-06 用户确认）
- 整体架构沿用 seatunnel 的 **master + worker** 模式；支持 **docker / docker compose / k8s** 三种部署
- **不部署 seatunnel 的 reload 服务**
- 页面配置各种数据库连接 → 作为**数据库客户端**使用
- 已配置的连接可直接**创建 seatunnel 任务、管理 seatunnel 任务**
- 主推融合形态（✅ 已确认）：Rust 控制面（dbloom-server，借鉴 dbx 驱动/SQL 层）+ React 前端 + SeaTunnel 引擎（Java）作同步执行面
- ✅ 需要**用户登录**：内置 1 个管理员账户，其余均为普通用户；**用户之间数据隔离**，管理员可查看/管理所有用户及其数据
- ✅ dbloom 通过 **OpenAPI 对外提供服务**；管理员为普通用户分配 **API Key**（可设置生效状态/生效时间/失效时间）
- ✅ **持久化 = 外部元数据库 + 共享文件卷**（2026-10-06 拍板，变更 v1.0 的 SQLite 决策 D10）：
  **元数据用外部关系数据库**（默认 **MySQL**，PostgreSQL 可切换，经 **ORM 层**适配——预估 SeaORM/sqlx，M0 锁定），
  走 docker volume / k8s PVC 或外部托管实例；**多节点共享文件卷**只放非结构化数据：日志 / 上传下载文件 /
  自定义 jar / checkpoint（路径 `logs|files|plugins/custom-jar|checkpoint`，见 docs/design/01-data-model.md §0/§4）
- ✅ **自定义 jar = 普通用户各自上传、按用户隔离**（2026-10-06 拍板，D26）：jar 供 SeaTunnel 自定义
  source/sink/transform 用；仅本人任务可引用本人 jar；管理员可审计/禁用用户（docs/design/04-security.md §5.1）
- ✅ **引擎融合方式 = 完整内置引擎**（2026-10-06 拍板）：SeaTunnel Engine（本地 2.3.12 定制版）源码
  纳入本仓 `engine/` 统一维护，构建**统一镜像**（前端 + Rust 控制面 + JVM 引擎，同镜像双进程）；
  **不再依赖官方 apache/seatunnel 镜像**；控制面经引擎 REST（8080）提交/管理任务
- ✅ **OpenAPI API Key 粒度 = 完整开放、不分作用域**（2026-10-06 拍板）：Key 是纯身份凭证，
  鉴权通过即拥有所绑用户的全部能力；不细分 scope（限流/IP 白名单等仅作预留扩展）
- ✅ **前端形态 = 纯 Web**（浏览器访问，dbx 的 React 资源平移；非 Tauri 桌面壳）
- ✅ **数据库客户端首批覆盖 = 对齐 dbx 现有 6 库**（2026-10-06 拍板）：mysql/postgres/sqlserver/
  mongodb/redis/elasticsearch；同步侧源/目标按 SeaTunnel connector 交集补齐
- ✅ **任务管理 = 完整档**（2026-10-06 拍板）：调度/定时、增量断点续跑、失败重试、任务依赖 DAG、
  告警通知、血缘/审计
- ✅ **master 与 dbloom-server 同节点**（2026-10-06 拍板）：master 上跑控制面 + 引擎 master，
  worker 独立扩展（最小部署 = 1 个 master 节点）
- ✅ **普通用户仅管理员创建**（2026-10-06 拍板）：后台创建、无自助注册
- ✅ **API Key 生命周期 = 简单策略**（2026-10-06 拍板）：到期即失效、撤销立即全局生效、
  暂不设数量上限（不自动续期/提醒，保留后续增强）
- ✅ **目标态全套设计已产出**（2026-10-06，`docs/design/00–06`，逐项经用户拍板）：① 写边界 = 读+写 +
  危险二次确认 + 生产只读锁；② 同步类型 = **透传 SeaTunnel 原生能力**（批量/增量/CDC，不改自研断点）；
  ③ 告警渠道 = **SMTP + Webhook**；④ UI 组件库 = **antd**；⑤ **无 CLI**；⑥ **License = Apache-2.0**；
  ⑦ 引擎纳入 = **裁剪精简（最小可用）**；⑧ dbloom 只出 HTTP（TLS 由外部网关终止，不内置 nginx）；
  ⑨ **统一转发铁律**：所有数据库 IO 一律由 dbloom-server 代执行，浏览器永不直连数据库（连接地址可能是
  集群内部 service 名）；⑩ 查询上限 5000 行/超时 120s、JWT 2h+7d、argon2、登录失败 5 次/5min 锁定、
  管理员随机密码 + 首登强改 + **后台强制重置密码能力**、审计粒度全落库、日志 14 天/任务历史 90 天、
  导出格式 **CSV + XLSX + JSON + SQL**、内置 cron 调度器、中文优先预留 i18n

## 架构文档
- **`docs/design/`（v1.1 目标态设计）**：00-overview（总览 + 26 项决策清单，含 D10 外部库/ORM、D26 自定义 jar）/
  01-data-model（MySQL/PG ORM 数据模型）/ 02-api（OpenAPI + 权限矩阵）/ 03-modules（crate 与前端结构、连接 manifest）/
  04-security / 05-deploy（三形态 + 元数据库编排）/ 06-milestones（M0–M7）。**实现按这套设计执行。**
- **`docs/architecture.md`（v0.7 草案）**：需求、两参考项目调研结论、融合拓扑、代码结构规划、持久化方案、
  三部署形态、里程碑、待确认项 —— 作为背景与决策来源依据（v0.7 重置持久化为外部库+ORM）。

## 关键实现依据（调研结论速记）
- seatunnel：Hazelcast 自治集群，master 端口 5801(hazelcast)+8080(http REST/UI)，worker 5802；启动 `bin/seatunnel-cluster.sh -r master|worker`；任务配置为 HOCON（env/source/sink 三段）；checkpoint 多副本需共享卷；k8s 官方 chart 在 `deploy/kubernetes/seatunnel/`
- dbx：= Rust workspace 分层（core→drivers→sql→types 单向依赖），连接类型插件化 `plugins/connection-types`（manifest 单一事实来源，build.rs 生成前端 TS html）；服务端 dbx-web 端口 4224；docker 单镜像多阶段构建

## 当前状态
- 2026-10-06：**M0 已完成并通过全链路验收**（commit af20669 / 首次 M0 提交见 e428266 后追加）
  - Rust workspace 7 crate 骨架；`dbloom-storage`（MySQL+迁移+DAO+TenantScope）、`dbloom-iam`
    （argon2/JWT/refresh/登录锁定/用户CRUD/API Key/审计）、`dbloom-server`（axum 路由 + 认证中间件 + OpenAPI）
  - `cargo build`+`cargo test` 绿（11 个逻辑单测）；真实 MySQL（WSL Docker）迁移+种子+curl 全链路
    （login/refresh/create-user/reset-password/issue-key/APIKey 鉴权/越权 403/OpenAPI）全部通过
  - 已知坑：axum 0.7 路由参数用 `:id`；SwaggerUI 构建期联网已弃用；详见 knowledge「dbloom M0 实施踩坑」
  - 下一步：M1（连接管理 + 客户端第一块，连接类型 manifest 落 types crate）
- 2026-10-06：仓库初始化（首次提交 e428266 已推送），架构草案 `docs/architecture.md`，设计 `docs/design/00-06`（v1.1，26 项决策全拍板）

## 注意
- 落代码前先读 `docs/architecture.md`（技术栈与两批决策均已确认，见上文「需求与架构决策」）
- 融合方案 = Rust 控制面 + SeaTunnel 执行面；后续随需求迭代细化模块与里程碑
