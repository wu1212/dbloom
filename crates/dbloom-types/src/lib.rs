//! dbloom-types：共享 DTO / 枚举 / 连接类型 manifest / 任务模型。
//!
//! 仅依赖 `dbloom-common`，被 `dbloom-storage`/`dbloom-iam`/`dbloom-server`
//! 与前端 OpenAPI 生成共享。DTO 均带 `utoipa::ToSchema` 以支持 OpenAPI 契约。
//!
//! 连接类型 manifest 见 `docs/design/03-modules.md` §3（M1 落地）。

pub mod apikey;
pub mod auth;
pub mod connection;
pub mod manifest;
pub mod query;
pub mod task;
pub mod user;

pub use apikey::{
    ApiKeyDto, ApiKeyStatus, CreateApiKeyRequest, CreateApiKeyResponse, UpdateApiKeyRequest,
};
pub use auth::{ChangePasswordRequest, LoginRequest, LoginResponse, RefreshRequest, RefreshResponse};
pub use connection::{
    ConnectionDto, ConnectionListResponse, CreateConnectionRequest, TestConnectionResponse,
    UpdateConnectionRequest,
};
pub use manifest::{Capabilities, ConnTypeManifest, FormField, all_manifests, is_supported_type, manifest_by_name};
pub use query::{
    ColumnItem, ColumnMeta, DatabaseItem, ExportRequest, ExportResult, QueryRequest, QueryResult,
    RowDeleteRequest, RowKey, RowListRequest, RowUpdateRequest, RowWriteResult, TableDdl, TableItem,
};
pub use task::{
    CreateTaskRequest, TableMappingRequest, TaskDto, TaskListResponse, TaskRunDto, TriggerRequest,
    UpdateTaskRequest,
};
pub use user::{UserDto, UserListResponse, UserRole, UserStatus};
