//! 用户域：Row→DTO 转换与输入校验（纯函数，无 DB）。

use dbloom_common::{AppError, Result};
use dbloom_storage::dao::UserRow;
use dbloom_types::user::{UserDto, UserListResponse, UserRole, UserStatus};

/// 转换时对异常枚举走默认（防御：DB 里出现未知取值时降级为 user/active）。
pub fn user_row_to_dto(row: &UserRow) -> UserDto {
    UserDto {
        id: row.id,
        username: row.username.clone(),
        role: UserRole::parse(&row.role).unwrap_or(UserRole::User),
        status: UserStatus::parse(&row.status).unwrap_or(UserStatus::Active),
        display_name: row.display_name.clone(),
        must_change_password: row.must_change_password,
        last_login_at: row.last_login_at,
        created_at: row.created_at,
    }
}

pub fn rows_to_list(total: i64, rows: Vec<UserRow>) -> UserListResponse {
    UserListResponse {
        total,
        items: rows.iter().map(user_row_to_dto).collect(),
    }
}

/// 用户名规则：字母/数字/下划线/点/中线，3..=64。
pub fn validate_username(username: &str) -> Result<()> {
    if username.len() < 3 || username.len() > 64 {
        return Err(AppError::validation("用户名长度须在 3~64 个字符"));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return Err(AppError::validation("用户名只能包含字母、数字、_ . -"));
    }
    Ok(())
}

/// 密码强度：至少 8 位。
pub fn validate_new_password(pw: &str) -> Result<()> {
    if pw.len() < 8 {
        return Err(AppError::validation("密码至少 8 位"));
    }
    Ok(())
}
