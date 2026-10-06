//! dbloom-common：错误类型、时间工具、全局配置项（无上层依赖）。
//!
//! 依赖方向：`dbloom-common` ← `dbloom-types` ← `dbloom-storage`；
//! 本 crate 是全部 crate 的最底层，不得依赖任何业务 crate。

pub mod error;
pub mod time;

pub use error::{AppError, ErrorCode, Result};
