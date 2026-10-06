//! dbloom-sync：同步编排（SeaTunnel 引擎编排/调度/告警）。
//!
//! M0 为空壳；负责：
//! - HOCON 生成（连接 manifest 片段渲染，D4/D26）
//! - SeaTunnel REST 客户端（submit/cancel/status，引擎 8080）
//! - 任务状态机、内置 cron 调度（D5）、告警引擎（SMTP/Webhook，D16）
//! M3 起接入（M3 = 引擎裁剪纳入，最大风险项，`06-milestones.md`）。

pub fn placeholder() -> &'static str {
    "dbloom-sync: M3 起实现（SeaTunnel 同步编排）"
}
