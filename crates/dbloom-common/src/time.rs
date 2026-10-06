//! 时间工具。全项目统一使用 **Unix 毫秒（i64，UTC）**，
//! 见 `docs/design/01-data-model.md` §0「时间：统一 BIGINT（Unix 毫秒）」。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 Unix 毫秒（UTC）。
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 把可空的 Unix 毫秒转成可读时间字符串（本地时区），失败返回空串。
pub fn format_ms(ms: Option<i64>) -> String {
    match ms {
        Some(v) => format_millis_display(v),
        None => String::new(),
    }
}

/// 仅用于展示（ISO 样式）。不解析，避免引入时区库依赖。
fn format_millis_display(ms: i64) -> String {
    // 展示用简化实现：转成秒并格式化为 UTC 文本（避免依赖 chrono 的时区表）。
    let secs = (ms / 1000) as u64;
    let days = secs / 86400;
    let rem = secs % 86400;
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{days}天+{hh:02}:{mm:02}:{ss:02}秒@定标")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_ms_is_positive_and_monotonic() {
        let a = now_ms();
        let b = now_ms();
        assert!(a > 0);
        assert!(b >= a);
    }
}
