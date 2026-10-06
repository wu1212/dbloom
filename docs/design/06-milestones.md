# dbloom 目标态设计 · 里程碑（Milestones，v1.1）

> 目标态（M7 完成）= `docs/design/00-overview.md` 所描述全能力，对外交付目标态产物。
> v1.1（2026-10-06）：M0/M3 存储层由 SQLite 改为 **外部元数据库 MySQL/PG + ORM**（D10）、新增 D26 自定义 jar。
> 依赖可并行的后标 `∥`；每阶段验收标准明确，**逐阶段回读验证后再进下一阶段**（防返工冒进）。

---

## M0 — 基础设施底座（Rust workspace + 元数据库 + IAM 骨架）✅ 已完成
- [x] `cargo workspace` 8 个 crate 空壳 + `apps/web` 脚手架（Vite + antd）。
- [x] `dbloom-storage`：**ORM（SeaORM/sqlx）连接 MySQL**（PG 可切，D10）、连接池、迁移清单 v1、`TenantScope` 租户过滤封装；本地开发用 docker 起 MySQL 8。
- [x] `dbloom-iam`：schema（users/sessions/api_keys/audit_logs）→ 内置 admin 种子（随机密码+强改 D20）→ argon2 / JWT / refresh / 登录锁定（D19）→ 用户 CRUD + 强制重置密码（D8/D20）→ API Key 签发/校验/生命周期（D9）→ 审计写入（D21）。
- [x] `dbloom-server`：axum + auth 中间件 + OpenAPI(utoipa) + `health` + `auth/*` + `users/*` + `api-keys/*` 路由（`02-api.md` §2.1–2.3）。

## M1 — 连接管理（客户端第一块）✅ 已完成
- [x] `dbloom-types`：连接类型 manifest（首批 6 库，`03-modules.md` §3）。
- [x] `dbloom-connector`：连接池 + 驱动 trait + 连接测试（D17 服务端发起）。
- [x] `connections` schema（含加密字段）→ CRUD + test + lock/unlock 路由 + 租户过滤。
- [x] 前端：连接列表/新建表单（manifest 驱动）/测试按钮。
- **验收**：对真实 mysql/postgres 各建一条连接并测试连通；密文落库（回读 `password_enc` 非明文）；列表不回传密码。

## M2 — 数据库客户端核心（SQL 工作台 + 元数据 + 数据浏览/编辑）
- [ ] `db-query`：执行/取消/超时 120s/上限 5000 行/分页/流式（WS `/ws/query`）。
- [ ] `schema`：databases/tables/columns/DDL；`data`：行浏览/编辑/删除。
- [ ] 写保护（D6）：危险语句识别、`is_production`+`read_only_lock`、`confirm=true` 二次确认链路。
- [ ] 导出（D23 CSV/XLSX/JSON/SQL）到共享卷 `files/download/` + 导入 `files/upload/`。
- [ ] 前端：SQL 工作台（编辑器/结果表/危险确认对话框/导出菜单）+ 元数据树 + 数据浏览页。
- **验收**：查询真实库 6 库各跑通 SELECT；无 WHERE UPDATE 被拦截；只读锁连接写操作被拒；导出 4 格式文件内容正确落共享卷；WS 流式 >5000 行可分页拉完。

## M3 — 引擎纳入与同步任务跑通（最小垂直切片）
- [ ] 引擎**原样引入（D2 修正）**：SeaTunnel 官方发布包（bin/config/connectors 零改写）入统一镜像；`dbloom-sync` HOCON 生成 + `submit/status/cancel` REST；`-r master/worker` 跑通集群；`upload-file`/REST 可用。
- [ ] `dbloom-sync`：HOCON 生成（manifest snippet 渲染，含 D26 自定义 jar 归属校验）→ `submit-job` → 状态轮询 → `task_runs` 落库。
- [ ] `tasks` schema + CRUD + 手动 `trigger`/`stop`/`retry` + 运行历史/日志（`logs/tasks/`）。
- [ ] 统一镜像：前端 + server + 引擎同镜像；`DBLOOM_ROLE` entrypoint；compose 起 master+worker。
- **验收**：mysql→mysql / postgres→postgres 全量同步真实跑通（任务从 pending→succeeded）；HOCON 快照可回看；失败 run 有 error/日志；镜像仅 1 个 artifact。

## M4 — 任务管理完整档（D4/D5）
- [ ] 同步类型：增量断点 / CDC **透传引擎原生能力**（`sync_mode` 选择 + HOCON 模板片段按 connector 交集启用）。
- [ ] 内置 cron 调度器（`scheduler_jobs` 抢锁、手动/定时/dag 三种触发）。
- [ ] DAG 依赖（`task_dependencies` 拓扑 + 上游成功驱动下游）。
- [ ] 告警（D16）：notification_channels(SMTP/Webhook) + alert_rules + alert_events + 投递（lettre / reqwest 签名）。
- [ ] 实时日志 WS `/ws/runs/{id}/logs`（`task_run_logs` 游标）+ 任务 DAG 血缘前端可视化。
- **验收**：cron 到点自触发并落 run；DAG 两任务串行按序跑；任务失败触发 SMTP 与 Webhook 各收到告警；实时日志流无漂移。

## M5 — 前端体验补全（D12/D25）
- [ ] 任务创建向导（源连接→目标连接→表映射→同步类型→调度→重试）；任务详情（状态机/历史/metrics）。
- [ ] 告警规则/事件页、审计页（admin）、系统设置页（查询限额/保留天数 D18/D22）。
- [ ] SQL 工作台体验：多标签、模板、收藏（favorites）、结果编辑快捷、危险操作 UX。
- [ ] i18n 结构（中文默认，D24）。
- **验收**：用户端到端走查（登录→建连接→查询→导出→建任务→调度触发→看日志→收告警）全链路浏览器验证。

## M6 — 部署三形态收口（D1/D10/D15）
- [ ] docker（单机）/ compose（含元数据库 `dbloom-db` 或外部 DB + NFS 多机卷）/ k8s Helm（数据库 StatefulSet/外部托管 + master + worker + RWX PVC + Secret + ConfigMap + Service + Ingress）三套编排补全并写文档。
- [ ] 多副本文档：dbloom-server 无状态扩容、master（引擎）故障自动重选、元数据库高可用、备份恢复演练。
- **验收**：docker 单机全链路可用；compose scale worker=2 任务仍正常；k8s 起集群 + RWX PVC（logs/jar/checkpoint 一致）+ 元数据库连通；数据一致（日志/文件/jar 同一份）。

## M7 — 收敛与发布
- [ ] README/AGENTS/部署指南/API 文档完整；License Apache-2.0（D14）；示例 job 与快照脚本。
- [ ] 安全回归（`04-security.md` §6 清单全绿）；性能上限文档（元数据库并发、查询上限、上传限制）。
- [ ] 镜像推送 CI（可选 GitHub Actions）与发版 tag。
- **验收**：目标态功能闭环 + 三形态一键起 + 安全回归通过。

---

## 依赖与顺序图
```text
M0 ──► M1 ──► M2 ──► M3 ──► M4 ──► M5 ──► M6 ──► M7
                 ▲────────┘  │  ▲──────┘
                 (M2 完成后 M3 可 M1 并行微调)（M4 依赖 M3 引擎就绪）
```
- 关键路径：M3（引擎原样引入 + 调度打通）是最大不确定项 —— **M2 收尾后立即启动 M3**，它是最容易卡住的地方。
- M2 之后的改进切一条**独立支线**：把 `dbloom-connector` 从自研实现替换为 **dbx 复用（D27）**，与 M3 并行推进（互不阻塞）。
- M5 前端体验可与 M3/M4 同步并行（页面随后端接口逐个子模块渐进搭建）。
