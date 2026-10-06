//! OpenAPI 契约（utoipa）。
//!
//! M0 提供文档骨架 + 模型 schema；路由 path 注解随各端点逐步补全
//! （`docs/design/02-api.md` §1 约定 `GET /api/v1/openapi.json` 与 `/api/v1/docs`）。

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "dbloom OpenAPI",
        description = "一站式数据融合平台：数据库客户端 + SeaTunnel 同步编排。\
                       凭据路径：Authorization: Bearer <JWT>（人机）或 <API Key>（机器，D9）。",
        version = "0.1.0",
        license(name = "Apache-2.0")
    ),
    servers(
        (url = "/", description = "本地 dbloom-server"),
    ),
    components(
        schemas(
            dbloom_types::auth::LoginRequest,
            dbloom_types::auth::LoginResponse,
            dbloom_types::auth::RefreshRequest,
            dbloom_types::auth::RefreshResponse,
            dbloom_types::auth::ChangePasswordRequest,
            dbloom_types::user::UserDto,
            dbloom_types::user::UserListResponse,
            dbloom_types::user::UserRole,
            dbloom_types::user::UserStatus,
            dbloom_types::apikey::ApiKeyDto,
            dbloom_types::apikey::ApiKeyStatus,
            dbloom_types::apikey::CreateApiKeyResponse,
            dbloom_types::apikey::UpdateApiKeyRequest,
        )
    ),
    tags(
        (name = "auth", description = "登录/会话"),
        (name = "users", description = "用户管理（admin）"),
        (name = "api-keys", description = "API Key 管理（admin，D9）"),
        (name = "system", description = "系统"),
    )
)]
pub struct ApiDoc;

/// 渲染 openapi.json 文本。
pub fn openapi_json() -> String {
    ApiDoc::openapi().to_pretty_json().unwrap_or_else(|_| "{}".into())
}

/// GET /api/v1/openapi.json handler（自产契约，无需构建期外部下载）。
pub async fn openapi_json_handler() -> axum::Json<serde_json::Value> {
    let text = openapi_json();
    let parsed = serde_json::from_str::<serde_json::Value>(&text)
        .unwrap_or_else(|_| serde_json::json!({"error": "openapi 渲染失败"}));
    axum::Json(parsed)
}
