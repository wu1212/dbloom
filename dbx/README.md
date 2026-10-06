# dbx/ — dbx 源码（vendored 复制进仓）

> ⭐ 本项目源码复用原则（最高优先级）：
> dbx 源码是经过无数场景验证的成熟代码，dbloom 是**融合**它，**不是重写**。
> 本目录源码头复制自 `G:\work\dbx`（用户定制版），用不到就删、不合适就改源码。

## 来源与复制范围
- **来源路径**：`G:\work\dbx`（本机工作区，用户深度定制的数据库客户端工具）
- **复制时间**：2026-10-06
- **已复制**（大约占比）：
  - `crates/` — **26 个 Rust crate 全量**（司机/核心/SQL/驱动聚合，见下）
  - `plugins/connection-types/` — 连接类型（connection-types）manifest 实体定义（D7/D27 里的「连接类型 manifest」真源）
  - `plugins/dialects/` — SQL 方言定义（连接类型 per-db 方言/默认值）
  - `vendor/` — dbx 自 vendored 的第三方依赖（tiberius=sqlserver 驱动、rumqttc 等；其中 Tauri 相关可删）
  - 根配置：`Cargo.toml`（workspace）、`Cargo.lock`、`rustfmt.toml`、`clippy.toml`、`LICENSE`、`SECURITY.md`
- **未复制**（用不到的就不拿）：`apps/`（桌面 App 壳）、`src-tauri/`（Tauri 界面）、`docs/`、`packages/`（各平台二进制发行包）、`deploy/`、`examples/`、`scripts/`、`tests/`、`skills/`、`agents/`

## crates/ 清单（26 个）
驱动（dbloom 客户端要复用的核心）：
| crate | 用途 | dbloom 状态 |
|---|---|---|
| `dbx-driver-mysql/postgres/sqlserver/mongodb/redis/elasticsearch` | **6 库驱动** | M2 后支线替换自研 connector |
| `dbx-drivers` | 驱动聚合/工厂/连接测试 | 接入点 |
| `dbx-driver-support` / `dbx-driver-agent` | 驱动公共支持 | reuse |
| `dbx-sql` / `dbx-sql-core` | SQL 处理核心 | reuse |
| `dbx-sql-dialect` / `dbx-sql-schema` / `dbx-sql-data` | 方言/元数据/数据 | reuse |
| `dbx-types` / `dbx-formats` / `dbx-platform` | 类型/格式/平台抽象 | reuse |
| `dbx-sqlite-worker` | sqlite worker | 暂不接入 |
| `dbx-core` | dbx 核心逻辑 | 评估接入 |
| `dbx-mcp` / `dbx-ai-provider` / `dbx-cli` / `dbx-web` / `dbx-tauri-*` | MCP/AI/CLI/Web/Tauri | **用不到，可删** |
| `dbx-plugin-runtime` / `dbx-tauri-consul` | 插件运行时 | 评估 |

## 接入方式（M2 后支线，见 docs/design/03-modules.md）
- **dbx/ 保持独立 Cargo workspace**（自带 Cargo.toml/lock），与 dbloom workspace 分离——避免依赖版本互相污染。
- dbloom 通过 **path 依赖**引入需要的 crate：`dbx-drivers = { path = "../dbx/crates/dbx-drivers" }`，需要改源码时直接在 `dbx/crates/` 内改（vendored 即本仓源码）。
- `dbloom-connector` 对外 API（连接测试/查询/元数据/行编辑）保持不变，只换底层实现为 dbx 驱动。

## 裁剪计划（按「用不到就删」）
- `vendor/`：删 `webview2-com-sys`、`wry`、`tauri-plugin-updater`（Tauri 相关）；其余保留。
- `crates/`：删 `dbx-mcp`、`dbx-ai-provider`、`dbx-cli`、`dbx-web`、`dbx-tauri-consul`、`dbx-tauri-schema`（M2 后支线接入时按需导出 workspace members 再删）。
- ⚠️ 裁剪前先确认被删 crate 无其它 crate 依赖（用 `cargo tree` 核对）。
