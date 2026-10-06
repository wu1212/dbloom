//! 存储层错误 → 统一 `AppError` 映射（含 sqlx 错误细分）。

use dbloom_common::AppError;

pub trait StorageErrorExt {
    fn storage_err(self) -> AppError;
}

impl StorageErrorExt for sqlx::Error {
    fn storage_err(self) -> AppError {
        match self {
            sqlx::Error::RowNotFound => AppError::not_found("记录不存在"),
            other => {
                let msg = other.to_string();
                // 唯一键冲突（MySQL 1062 / SQLite 2067）
                if msg.contains("Duplicate entry") || msg.contains("UNIQUE constraint failed") {
                    return AppError::conflict("唯一约束冲突（重名或重复值）");
                }
                AppError::internal(format!("数据库错误: {msg}"))
            }
        }
    }
}
