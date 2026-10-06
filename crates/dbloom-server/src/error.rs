//! HTTP 展示层错误：`AppError` → `{code, message, trace_id}` 响应（02-api.md §1）。

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use dbloom_common::ErrorCode;
use serde_json::json;

/// server 侧统一错误（可直接作为 reject 返回）。
#[derive(Debug, Clone)]
pub struct ApiError(pub dbloom_common::AppError);

impl From<dbloom_common::AppError> for ApiError {
    fn from(e: dbloom_common::AppError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let e = self.0;
        let status = match e.code {
            ErrorCode::Unauthorized
            | ErrorCode::TokenExpired
            | ErrorCode::RefreshInvalid => StatusCode::UNAUTHORIZED,
            ErrorCode::AccountLocked => StatusCode::LOCKED,
            ErrorCode::Forbidden => StatusCode::FORBIDDEN,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::Conflict => StatusCode::CONFLICT,
            ErrorCode::Validation => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Ok => StatusCode::OK,
        };
        let body = json!({
            "code": e.code as i32,
            "message": e.message,
            "trace_id": e.trace_id.clone().unwrap_or_default(),
        });
        (status, Json(body)).into_response()
    }
}

/// 成功响应包装：`{code:0, data:…}`。
pub fn ok_json<T: serde::Serialize>(data: T) -> Json<serde_json::Value> {
    Json(json!({ "code": 0, "data": data }))
}

/// 无 data 的成功（如 DELETE）。
pub fn ok_no_data() -> Json<serde_json::Value> {
    Json(json!({ "code": 0, "data": null }))
}
