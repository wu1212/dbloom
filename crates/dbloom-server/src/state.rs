//! 应用状态：共享给全部 handler。

use axum::extract::FromRef;
use dbloom_iam::Iam;
use sqlx::MySqlPool;
use std::sync::Arc;

/// 应用全局状态。
#[derive(Clone)]
pub struct AppState {
    pub iam: Arc<Iam>,
    pub pool: MySqlPool,
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
