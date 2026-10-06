# dbloom

> 数据同步功能 × 各种数据库客户端工具 —— 融合项目

将「数据同步（ETL/CDC）」与「数据库客户端（浏览/编辑/查询）」两大能力融合到同一项目中，
一站式完成：连接各种数据库 → 可视化浏览与编辑 → 配置并执行数据同步任务。

## 项目背景

本仓库从零构建，融合以下参考项目的核心能力：

| 参考项目 | 定位 | 技术栈 |
| --- | --- | --- |
| [Apache SeaTunnel](https://seatunnel.apache.org/)（`G:\work\seatunnel`） | 数据同步引擎：多源异构数据采集/同步/转换 | Java 8 / Maven 多模块 |
| [dbx](https://github.com/t8y2/dbx)（`G:\work\dbx`） | 桌面数据库客户端：100+ 数据库连接、查询、表编辑、AI 助手 | Rust / Tauri / Cargo workspace |

- 数据同步能力来自 seatunnel（连接器体系、source/sink/transform、服务端任务引擎）
- 数据库客户端能力来自 dbx（驱动层、SQL 解析、Schema 浏览、桌面 UI）

## 技术选型（已确认）

- 控制面：**Rust**（`dbloom-server`，借鉴 dbx 的驱动/SQL 层能力）
- 前端：**React（Vite）+ Ant Design** 纯 Web 页面（浏览器访问；dbx 前端资源平移，非 Tauri 桌面壳）
- 执行面：**SeaTunnel（Java）master + worker**，**完整内置引擎**——引擎与 dbx 源码均已**复制进仓**（`engine/`、`dbx/`），
  复用为主（删用不到、改不合适、不重写，见 AGENTS.md「源码复用原则」），构建**统一镜像**（前端 + 控制面 + 引擎同镜像），不依赖官方 apache/seatunnel 镜像
- 部署：docker / docker compose / k8s（三形态，dbloom 只出 HTTP，TLS 由外部网关终止），不部署 SeaTunnel reload 服务
- 持久化：元数据用**外部关系数据库**（默认 **MySQL**，PostgreSQL 可切换，经 **ORM 层**适配）；非结构化数据
  （日志、上传下载文件、自定义 jar、checkpoint）随 docker volume / k8s PVC 持久化，**多节点共享同一网络存储**
  （路径划分见 `docs/design/01-data-model.md` / `05-deploy.md`）
- **自定义 jar**：普通用户可各自上传自己的 jar（按用户隔离 D26），供 SeaTunnel 自定义 source/sink/transform 使用
- **OpenAPI API Key**：完整开放、不分作用域——Key 是纯身份凭证，鉴权通过即拥有所绑用户全部能力
- **统一转发**：所有数据库 IO 由 dbloom-server 代执行（用户配置的连接地址可能是集群内部 service 名，
  浏览器不直连数据库）
- License：**Apache-2.0**

## 核心需求

- 页面配置数据库连接 → 即做**数据库客户端**（查/浏览/编辑），又可直接**生成 SeaTunnel 同步任务**并管理任务
- **用户登录**：内置 1 个管理员，其余普通用户；用户之间**数据隔离**，管理员可查看/管理所有用户及其数据
- 通过 **OpenAPI 对外提供服务**：管理员为普通用户签发 **API Key**（可设置生效状态/生效时间/失效时间）

## 状态

- [x] GitHub 仓库创建并关联远程
- [x] 需求收集与架构设计（`docs/architecture.md` v0.7 + 历轮拍板：完整内置引擎 / API Key 不分作用域 / 多节点共享持久化 / 首批 6 库 / 任务管理完整档 / master 同节点 / 仅管理员创建 / API Key 简单生命周期 / 元数据库=外部MySQL+PG ORM / 自定义 jar 按用户隔离）
- [x] **目标态完整设计**（`docs/design/` v1.1：00-overview 总览与 26 项决策清单 / 01-data-model / 02-api / 03-modules / 04-security / 05-deploy / 06-milestones）
- [x] **M0 基础设施底座**：cargo workspace（common/types/storage/iam/server/connector/sync 7 crate）+ IAM（用户/会话/API Key/审计）+ 元数据库 ORM（MySQL）+ axum 服务 + OpenAPI；单测 + 集成实测通过
- [x] **M1 连接管理**：连接类型 manifest（首批 6 库）+ 凭据 AES-GCM 加密 + 连接 CRUD/test/lock + 租户隔离（越权 404）+ apps/web 脚手架（登录/连接页）
- [x] **M2 数据库客户端核心**：SQL 工作台（执行/分页/超时）+ 元数据（库/表/列/DDL）+ 行浏览/更新/删除 + D6 写保护（危险语句识别 + 二次确认 + 只读锁）+ 导出 CSV/JSON/SQL/XLSX + 路径穿越防护；cargo test 24 绿 + 浏览器端到端通过
- [x] **源码进仓**：seatunnel 2.3.12 定制版 → `engine/`（Maven 工程）+ dbx 驱动核心 → `dbx/`（Cargo workspace）；源码复用原则定稿
- [ ] **M3 引擎纳入 + 同步任务**（最大风险项：引擎构建环境 + HOCON 渲染 + REST 提交 + 任务管理；`engine/` 源码已就位待构建）
- [ ] **M4+** 调度 / 告警 / 多副本部署 / 审计台 / connector 层替换为 dbx 复用（D27 支线）

## 文档

- **目标态设计**：`docs/design/00-overview.md`（含全部决策清单，其余见 `docs/design/` 01–06）
- **背景与决策记录**：`docs/architecture.md`（v0.7 草案）

## 项目结构

```
dbloom/
├─ apps/web/         # 前端（React + Vite + Ant Design）
├─ crates/           # 控制面 Rust workspace（dbloom-{common,types,storage,iam,server,connector,sync} 7 crate）
├─ engine/seatunnel  # ★ seatunnel 源码（2.3.12 定制版，Maven 工程，复用为主：删用不到/改不合适/不重写）
├─ dbx/              # ★ dbx 源码（驱动核心 crates，Cargo workspace，复用为主）
├─ docs/             # 架构背景（architecture.md）+ 目标态设计（design/00–06）
├─ shared/           # 共享数据根（运行时：导出文件/日志/jar/checkpoint，多节点共享卷；运行数据不入库，仅 .gitkeep）
├─ probes/           # 一次性探针（探索验证用，如 dbx-bridge）
├─ .env.example      # 配置模板（复制为 .env 本地使用）
└─ Cargo.toml        # workspace 根
```

## 配置文件（.env）

dbloom 的配置统一走**环境变量**（docker / compose / k8s 均为 env 注入，清单见 `docs/design/05-deploy.md` §4）。
为方便本地开发，仓库提供了模板 **`.env.example`**：

```
cp .env.example .env    # 复制为本地配置，按需修改（.env 已 gitignore，不入库）
```

`dbloom-server` 启动时自动读取工作目录下 `.env`；**系统已存在的环境变量优先于 `.env`**
（docker/k8s 直接用 `-e` / Secret / ConfigMap 注入同名变量即可，无需 `.env`）。

关键变量一览（详见 `.env.example` 注释）：

| 变量 | 作用 | 默认 |
| --- | --- | --- |
| `DB_DSN` | 元数据库连接串（MySQL/PG，D10） | `mysql://root@localhost:3306/dbloom` |
| `DBLOOM_JWT_SECRET` | JWT 签名密钥（D19，生产必须注入随机值） | dev 值（启动告警） |
| `DBLOOM_HTTP_PORT` | dbloom-server 对外 HTTP 端口 | `8081` |
| `DBLOOM_ROLE` | 角色 `master`/`worker`（D1/D11） | `master` |
| `DBLOOM_SECRET_KEY_FILE` | 主密钥文件路径（M1 凭据加密，生产必填） | — |
| `DBLOOM_DATA_ROOT` | 共享数据根（日志/文件/jar/checkpoint，D10） | `/dbloom-data` |
| `SEATUNNEL_HTTP_PORT` | 引擎 REST（内部，不对外） | `8080` |

## 本地开发

前置：Rust 工具链 + 一个可用的 MySQL（元数据库）。启动：

```
cp .env.example .env                     # 1. 按需填 DB_DSN（含密码）/ DBLOOM_JWT_SECRET
# 编辑 .env 里的 DB_DSN 指向你的 MySQL
cargo run -p dbloom-server                # 2. 启动（自动建库/迁移/种子 admin）
```

- 启动后自动执行迁移；首启种子内置管理员 `admin`（**随机密码打印在启动日志**，登录后强制修改，D20）。
- 服务监听 `http://localhost:8081`，OpenAPI 文档：`http://localhost:8081/api/v1/docs`
- 角色：内置 1 个管理员可管理全部用户，其余普通用户数据相互隔离；管理员为普通用户签发 API Key（D 决策）。

## License

Apache-2.0
