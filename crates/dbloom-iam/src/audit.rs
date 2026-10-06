//! 审计写入（D21）：统一调 `audit_logs` 表。

use dbloom_storage::dao::AuditDao;
use dbloom_common::{time::now_ms, Result};
use serde_json::{json, Value};

/// 记一条审计。detail 只存摘要（不要放明文密码/完整 SQL）。
pub async fn record_audit(
    dao: &AuditDao,
    actor_user_id: Option<i64>,
    actor_type: &str, // 'user' | 'api_key'
    action: &str,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
    detail: Option<Value>,
    ip: Option<&str>,
) -> Result<()> {
    dao.insert(
        actor_user_id,
        actor_type,
        action,
        resource_type,
        resource_id,
        detail,
        ip,
        now_ms(),
    )
    .await?;
    Ok(())
}

/// 便捷：把键值对 JSON 化（仅摘要级）。值接收 `String`（保持 Send/Sync 友好）。
pub fn detail_json(kv: &[(&str, String)]) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in kv {
        m.insert(k.to_string(), json!(v));
    }
    Value::Object(m)
}
