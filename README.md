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
- 执行面：**SeaTunnel（Java）master + worker**，**完整内置引擎**——引擎源码纳入本仓 `engine/`（裁剪精简），
  构建**统一镜像**（前端 + 控制面 + 引擎同镜像），不依赖官方 apache/seatunnel 镜像
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
- [ ] 技术栈落地检查（Rust 工具链 / Node / Java）
- [ ] 模块骨架（M0：用户体系 + API Key + 多租户隔离）
- [ ] 首个可运行版本

## 文档

- **目标态设计**：`docs/design/00-overview.md`（含全部决策清单，其余见 `docs/design/` 01–06）
- **背景与决策记录**：`docs/architecture.md`（v0.7 草案）

## 开发

（构建与运行说明待需求与选型确定后补充）

## License

待定
