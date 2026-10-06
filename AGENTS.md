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

## 当前状态
- 2026-10-06：仓库初始化（git init + origin 关联），骨架待需求确认后搭建
- 首次提交未推送前，需求尚未确认

## 注意
- 具体需求（功能范围/技术形态/模块划分）尚未由用户确认，落代码前先确认需求
- 融合方案是「以 dbx 为底座集成 seatunnel 能力」还是「独立架构两端并存」，待定
