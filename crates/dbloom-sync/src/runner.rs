//! run 生命周期执行器：提交引擎 → 轮询状态 → 回写 task_runs。
//!
//! 职责（T1）：
//! - `execute_run`：把一条 pending 的 run 提交给引擎（submitJob），拿到引擎 jobId 回填，
//!   随后周期轮询 `job-info`，将引擎终态（FINISHED/FAILED/CANCELED）映射回写 dbloom run。
//! - `stop_run`：对运行中的 run 调引擎 stop-job（真停），并回写 canceled。
//! - `retry_run`：对最近失败的 run 用新幂等键重试。
//!
//! 轮询间隔 1s，超时由 `task.timeout_sec` 控制（0=不限）；每轮检查一次超时。

use dbloom_common::time::now_ms;
use dbloom_storage::dao::{TaskRunDao, TaskRunRow};
use std::sync::Arc;
use tokio::time::{Duration, sleep};

use crate::engine::{EngineClient, engine_status_to_run, is_engine_terminal};

/// 轮询间隔（秒）。
const POLL_INTERVAL_SECS: u64 = 1;

/// 提交并跑完一条 run（阻塞直到终态）。调用方会自行 spawn，不阻塞 HTTP。
///
/// - `already_submited_job_id`: 若调用方已先提交（拿到 job id），传入后跳过重复提交。
pub async fn submit_and_wait(
    runs: Arc<TaskRunDao>,
    engine: Arc<EngineClient>,
    run_id: i64,
    _task_id: i64,
    job_name: &str,
    hocon: &str,
    timeout_sec: i32,
) {
    let start = now_ms();
    // 1) 提交任务
    let job_id_result = engine.submit_job(&format!("dbloom-{job_name}-{run_id}"), hocon).await;
    let job_id = match job_id_result {
        Ok(job_id) => {
            if let Err(e) = runs.mark_running(run_id, &job_id, now_ms()).await {
                tracing::error!("run {run_id} 回写 running 失败: {e}");
            }
            job_id
        }
        Err(e) => {
            let msg = format!("任务提交失败: {e}");
            tracing::warn!("{msg}");
            let _ = runs.finish(run_id, "failed", Some(&msg), now_ms()).await;
            return;
        }
    };

    // 2) 轮询直到终态
    loop {
        match engine.job_status(&job_id).await {
            Ok((st, err)) => {
                if is_engine_terminal(&st) {
                    let run_st = engine_status_to_run(&st, true);
                    let err_msg = if run_st == "failed" {
                        Some(err.unwrap_or_else(|| format!("引擎任务失败（{st}），详见引擎日志")))
                    } else {
                        err
                    };
                    if let Err(e) = runs.finish(run_id, run_st, err_msg.as_deref(), now_ms()).await {
                        tracing::error!("run {run_id} 回写终态失败: {e}");
                    }
                    tracing::info!("run {run_id} 终态: {st} → {run_st}");
                    return;
                }
                // 非终态：查超时
                if timeout_sec > 0 && now_ms() - start > timeout_sec as i64 * 1000 {
                    let msg = format!("任务运行超时（>{timeout_sec}s），已停止");
                    tracing::warn!("{msg}");
                    let _ = engine.stop_job(&job_id).await;
                    let _ = runs.finish(run_id, "stopped", Some(&msg), now_ms()).await;
                    return;
                }
            }
            Err(e) => {
                // 引擎状态查询失败：记录一次后继续轮询，若持续失败则按失败结束
                tracing::warn!("查询 run {run_id} 状态失败: {e}");
                if now_ms() - start > 30_000 {
                    let msg = format!("引擎状态查询持续失败: {e}");
                    let _ = runs.finish(run_id, "failed", Some(&msg), now_ms()).await;
                    return;
                }
            }
        }
        sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
    }
}

/// 中止一条 run：找到它关联的引擎 job id 并调用引擎 stop（真停）。回写 canceled。
///
/// 返回 (是否仍有存活引擎任务) 供上层判断；本函数保证对未被终止的字符串做兜底。
pub async fn stop_run(
    runs: Arc<TaskRunDao>,
    engine: Arc<EngineClient>,
    run: &TaskRunRow,
) -> Result<(), String> {
    // 若 run 还没拿到 job id（还在 pending/提交中），先标记 canceled 即可。
    let Some(job_id) = run.sea_tunnel_job_id.as_deref() else {
        // pending 阶段（未提交成功前）被 stop：直接置 canceled + 结束时间
        let _ = runs.finish(run.id, "canceled", Some("任务未开始，已取消"), now_ms()).await;
        return Ok(());
    };
    engine.stop_job(job_id).await?;
    let _ = runs.finish(run.id, "canceled", Some("已由用户停止"), now_ms()).await;
    Ok(())
}

/// 重试失败 run：返回用于重试的幂等键（新 run 由上层创建）。
pub fn retry_idempotency_key() -> String {
    format!(
        "retry-{}-{}",
        uuid::Uuid::new_v4(),
        now_ms()
    )
}
