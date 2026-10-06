//! dbloom-server 入口：加载配置 → 连接并迁移 MySQL → 种子 admin → 起 HTTP 服务。
//!
//! 环境变量：
//! - `DB_DSN`            MySQL DSN（如 `mysql://root:pass@localhost:3306/dbloom`）
//! - `DBLOOM_JWT_SECRET` JWT 签名密钥（生产必须注入；缺省为 dev 值并告警）
//! - `DBLOOM_HTTP_PORT`  dbloom-server 对外 HTTP 端口（默认 8081；引擎 REST 8080 仅内部）
//! - `DBLOOM_ROLE`       角色 master|worker（默认 master；后续 M 阶段使用）

use dbloom_common::time::now_ms;
use dbloom_iam::{
    Config,
    password::{generate_random_password, hash_password},
};
use dbloom_storage::dao::{ApiKeyDao, AuditDao, SessionDao, UserDao};
use std::{env, net::SocketAddr, sync::Arc};

use dbloom_iam::Iam;
use dbloom_storage::connect_pool;
use dbloom_types::UserRole;

use crate::state::AppState;

mod api;
mod auth;
mod error;
mod openapi;
mod state;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,dbloom_server=debug".into()),
        )
        .init();

    if let Err(e) = run().await {
        tracing::error!("dbloom-server 启动失败: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dsn = env::var("DB_DSN").ok();
    let pool = connect_pool(dsn.as_deref()).await?;
    dbloom_storage::run_migrations(&pool).await?;
    tracing::info!("数据库连接与迁移完成");

    // ---- JWT 签名密钥 ----
    let jwt_secret = env::var("DBLOOM_JWT_SECRET").unwrap_or_else(|_| {
        tracing::warn!("未配置 DBLOOM_JWT_SECRET，使用开发默认密钥（生产必须注入！）");
        "dev-insecure-jwt-secret-do-not-use-in-prod".to_string()
    });

    // ---- 种子：内置管理员（D20 随机密码 + 首登强改） ----
    let users = UserDao::new(pool.clone());
    let sessions = SessionDao::new(pool.clone());
    let api_keys = ApiKeyDao::new(pool.clone());
    let audit = AuditDao::new(pool.clone());

    ensure_admin(&users).await?;

    let iam = Arc::new(Iam::new(
        UserDao::new(pool.clone()),
        sessions,
        api_keys,
        audit,
        Config { jwt_secret },
    ));

    let state = Arc::new(AppState { iam, pool });

    // ---- 启动 HTTP ----
    let port = env::var("DBLOOM_HTTP_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8081);
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let app = api::build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("dbloom-server 监听 {addr}（OpenAPI: http://localhost:{port}/api/v1/docs）");
    axum::serve(listener, app).await?;
    Ok(())
}

/// 幂等确保内置 admin 存在（随机密码打印日志 + 首登强改，D20）。
async fn ensure_admin(users: &UserDao) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match users.get_by_username("admin").await {
        Ok(_) => {
            tracing::info!("内置管理员 admin 已存在");
            Ok(())
        }
        Err(_) => {
            let pw = generate_random_password(16);
            let hash = hash_password(&pw)?;
            let created_at = now_ms();
            users
                .create("admin", &hash, UserRole::Admin.as_str(), None, created_at)
                .await?;
            tracing::warn!(
                "已创建内置管理员 admin —— 初始密码（仅此一次打印，请即刻记录并登录后修改，D20）: {pw}"
            );
            tracing::info!("admin must_change_password=1（首登强制改密）");
            Ok(())
        }
    }
}
