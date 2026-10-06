//! 应用状态：共享给全部 handler。

use axum::extract::FromRef;
use dbloom_common::CryptoProvider;
use dbloom_iam::Iam;
use dbloom_storage::dao::ConnectionDao;
use sqlx::MySqlPool;
use std::sync::Arc;

/// 应用全局状态。
#[derive(Clone)]
pub struct AppState {
    pub iam: Arc<Iam>,
    pub pool: MySqlPool,
    /// 连接 DAO（租户过滤在 DAO/业务层，D8）。
    pub connections: ConnectionDao,
    /// 连接凭据加解密（AES-256-GCM，04-security §3.1）。
    pub crypto: Arc<CryptoProvider>,
}

impl FromRef<AppState> for Arc<Iam> {
    fn from_ref(state: &AppState) -> Self {
        state.iam.clone()
    }
}

impl FromRef<AppState> for MySqlPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}
