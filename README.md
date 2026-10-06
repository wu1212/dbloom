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

## 技术选型（方向）

需求确认后拟定，初步考虑：

- 底座：Rust + Tauri（桌面 UI 与客户端能力，参考 dbx）
- 数据同步：集成 seatunnel 连接器 / 任务引擎能力
- 数据库驱动层：参考 dbx 的 `crates/dbx-driver-*` 分层

## 状态

- [x] GitHub 仓库创建并关联远程
- [ ] 需求确认
- [ ] 架构设计
- [ ] 模块骨架
- [ ] 首个可运行版本

## 开发

（构建与运行说明待需求与选型确定后补充）

## License

待定
