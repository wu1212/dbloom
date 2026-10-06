//! SeaTunnel 引擎 REST 客户端（dbloom 经引擎 8080 提交/查询/取消任务）。
//!
//! 端点（fork 版 Jetty 注册在 root；已实测验证，见 `.weai/logs/seatunnel-engine-server.log`）：
//! - `POST /submit-job?format=hocon&jobName=xxx`   body=HOCON 文本 → `{jobId, jobName}`
//! - `GET  /job-info/{jobId}`                     → `{jobId, jobName, jobStatus, errorMsg, ...}`
//! - `POST /stop-job`                              body=`{"jobId": n}` → `{jobId}`
//! - `GET  /overview`                              → 集群概览（health 探测）
//!
//! 引擎 JobStatus 值域（`JobStatus.java`）：INITIALIZING/CREATED/PENDING/SCHEDULED/RUNNING/
//! FAILING/FAILED/DOING_SAVEPOINT/SAVEPOINT_DONE/CANCELING/CANCELED/FINISHED/UNKNOWABLE。
//! 终态：FINISHED=成功；FAILED/UNKNOWABLE=失败；CANCELED=取消。

use serde_json::Value;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct EngineClient {
    /// 引擎 REST 基址（如 `http://127.0.0.1:8080`）。
    pub base_url: String,
    http: reqwest::Client,
}

impl EngineClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            base_url,
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(3))
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client 构建失败"),
        }
    }

    /// 从环境变量读取引擎地址（`SEATUNNEL_HTTP_PORT`，默认 8080；host 默认 127.0.0.1）。
    pub fn from_env() -> Self {
        let port = std::env::var("SEATUNNEL_HTTP_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);
        Self::new(format!("http://127.0.0.1:{port}"))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// 健康探测：GET overview（用于启动检查 / health）。
    pub async fn ping(&self) -> bool {
        match self.http.get(self.url("/overview")).send().await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }

    /// 提交任务：body 为 HOCON 文本；返回引擎 jobId（字符串）。
    pub async fn submit_job(&self, job_name: &str, hocon: &str) -> Result<String, String> {
        let resp = self
            .http
            .post(self.url("/submit-job"))
            .query(&[("format", "hocon"), ("jobName", job_name)])
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(hocon.to_string())
            .send()
            .await
            .map_err(|e| format!("引擎提交请求失败: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("读取引擎响应失败: {e}"))?;
        if !status.is_success() {
            return Err(format!("引擎 submit-job 返回 {status}: {text}"));
        }
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("引擎响应非 JSON: {e} ({text})"))?;
        v.get("jobId")
            .and_then(Value::as_str)
            .map(|s| s.to_string())
            .or_else(|| v.get("jobId").and_then(Value::as_i64).map(|n| n.to_string()))
            .ok_or_else(|| format!("引擎响应缺少 jobId: {text}"))
    }

    /// 查询任务状态：返回引擎 JobStatus 字符串与可选错误信息。
    pub async fn job_status(&self, job_id: &str) -> Result<(String, Option<String>), String> {
        let resp = self
            .http
            .get(self.url(&format!("/job-info/{job_id}")))
            .send()
            .await
            .map_err(|e| format!("引擎状态查询请求失败: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("读取引擎状态响应失败: {e}"))?;
        if !status.is_success() {
            return Err(format!("引擎 job-info 返回 {status}: {text}"));
        }
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("引擎状态响应非 JSON: {e} ({text})"))?;
        let st = v
            .get("jobStatus")
            .and_then(Value::as_str)
            .unwrap_or("UNKNOWABLE")
            .to_uppercase();
        let err = v
            .get("errorMsg")
            .and_then(Value::as_str)
            .or_else(|| v.get("error_message").and_then(Value::as_str))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        debug!("job {job_id} status = {st} err={err:?}");
        Ok((st, err))
    }

    /// 停止（取消）任务：要求 jobId 存在。
    pub async fn stop_job(&self, job_id: &str) -> Result<(), String> {
        let body = serde_json::json!({ "jobId": job_id });
        let resp = self
            .http
            .post(self.url("/stop-job"))
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| format!("引擎停止请求失败: {e}"))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("读取引擎停止响应失败: {e}"))?;
        if !status.is_success() {
            return Err(format!("引擎 stop-job 返回 {status}: {text}"));
        }
        Ok(())
    }
}

/// 引擎任务状态 → dbloom run 状态（'pending'|'running'|'succeeded'|'failed'|'canceled'）。
pub fn engine_status_to_run(engine_status: &str, finished: bool) -> &'static str {
    if !finished {
        return "running";
    }
    match engine_status {
        "FINISHED" => "succeeded",
        "CANCELED" | "SAVEPOINT_DONE" => "canceled",
        _ => "failed", // FAILED / UNKNOWABLE 等终态失败
    }
}

/// 是否引擎终态。
pub fn is_engine_terminal(engine_status: &str) -> bool {
    matches!(
        engine_status,
        "FINISHED" | "FAILED" | "CANCELED" | "UNKNOWABLE" | "SAVEPOINT_DONE"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping() {
        assert_eq!(engine_status_to_run("FINISHED", true), "succeeded");
        assert_eq!(engine_status_to_run("FAILED", true), "failed");
        assert_eq!(engine_status_to_run("CANCELED", true), "canceled");
        assert_eq!(engine_status_to_run("RUNNING", false), "running");
        assert_eq!(engine_status_to_run("PENDING", false), "running");
    }

    #[test]
    fn terminal_check() {
        assert!(is_engine_terminal("FINISHED"));
        assert!(is_engine_terminal("FAILED"));
        assert!(is_engine_terminal("CANCELED"));
        assert!(!is_engine_terminal("RUNNING"));
        assert!(!is_engine_terminal("SCHEDULED"));
    }

    #[test]
    fn base_url_normalized() {
        let c = EngineClient::new("http://127.0.0.1:8080/");
        assert_eq!(c.base_url, "http://127.0.0.1:8080");
        assert_eq!(c.url("/x"), "http://127.0.0.1:8080/x");
    }
}
