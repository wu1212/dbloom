//! 全局错误类型与业务错误码。
//!
//! 契约见 `docs/design/02-api.md` §1：响应统一 `{code, message, trace_id}`，
//! HTTP 状态码语义保留。`code` 在 0 表示成功；非 0 为业务码。

use std::fmt;

/// 业务错误码。
///
/// 约定：
/// - `0`     成功
/// - `401xx` 认证（401 / 登录失败锁定时限 40103 等）
/// - `403xx` 授权（403，越权访问）
/// - `404xx` 资源不存在（404）
/// - `409xx` 冲突（409，重名 / 状态不允许）
/// - `422xx` 参数校验失败（422）
/// - `500xx` 服务端内部错误（500）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// 成功（响应包装 `code:0`）
    Ok = 0,
    /// 401：未认证 / 凭据无效
    Unauthorized = 40100,
    /// 401：access JWT 过期（前端据此刷新 token，02-api.md §1）
    TokenExpired = 40101,
    /// 401：refresh token 无效 / 已吊销
    RefreshInvalid = 40102,
    /// 423 等效：账号被锁定（登录失败超阈值）
    AccountLocked = 42300,
    /// 403：权能不足（非管理员访问管理员接口）
    Forbidden = 40300,
    /// 404：资源不存在
    NotFound = 40400,
    /// 409：资源冲突（重名 / 状态不允许/API Key 未生效）
    Conflict = 40900,
    /// 422：请求参数校验失败
    Validation = 42200,
    /// 500：内部错误
    Internal = 50000,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as i32)
    }
}

/// 应用错误：业务码 + 用户可读信息 + 附加 context（用于日志 / trace）。
///
/// 携带 `trace_id`（由 `WithTrace` 在请求入口注入）以支持 02-api.md 的
/// `{code, message, trace_id}` 响应结构。
#[derive(Debug, Clone)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    /// 附加上下文（内部诊断用，不直接透传响应；可含 key=value 片段）
    pub context: Vec<String>,
    pub trace_id: Option<String>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            context: Vec::new(),
            trace_id: None,
        }
    }

    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context.push(ctx.into());
        self
    }

    /// 注入请求级 trace_id（由中间件调用）。
    pub fn with_trace(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, msg)
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, msg)
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, msg)
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, msg)
    }
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::Validation, msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, msg)
    }
    pub fn token_expired() -> Self {
        Self::new(ErrorCode::TokenExpired, "access token 已过期")
    }
    pub fn refresh_invalid(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::RefreshInvalid, msg)
    }
    pub fn account_locked(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::AccountLocked, msg)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::internal(e.to_string())
    }
}

/// 便捷 Result 别名。
pub type Result<T> = std::result::Result<T, AppError>;
