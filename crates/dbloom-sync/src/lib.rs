//! dbloom-sync：同步编排（SeaTunnel 引擎编排/调度/告警）。
//!
//! M3（T1 最小垂直切片）落地：
//! - `hocon`：连接配置 → SeaTunnel HOCON 生成（env/source/sink 三段 + 脱敏快照）
//! - `engine`：SeaTunnel REST 客户端（submit / status / cancel，引擎 8080）
//! - 状态机：run 生命周期（pending→running→succeeded/failed/canceled）+ 轮询回写
//!
//! 后续 M4：内置 cron 调度（D5）、告警引擎（SMTP/Webhook，D16）。

pub mod engine;
pub mod hocon;
pub mod runner;

pub use engine::{EngineClient, engine_status_to_run, is_engine_terminal};
pub use hocon::{JdbcConn, RenderedHocon, SyncMode, TableMapping, jdbc_driver, jdbc_url, parse_mappings, render_full_jdbc};
pub use runner::{retry_idempotency_key, stop_run, submit_and_wait};
