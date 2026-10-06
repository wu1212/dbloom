//! 连接类型 manifest（`docs/design/03-modules.md` §3，单一事实来源，D7）。
//!
//! 前端表单、驱动适配、HOCON 同步模板全部从同一份 manifest 推导——
//! 实现「配一次连接，客户端 + 同步都能用」的关键。
//! `implemented` 字段标记该类型当前版本的后端驱动是否已实现（未实现则
//! 前端可展示但「测试/查询」提示暂不可用，M2/M3 按里程碑补齐）。

use serde::{Deserialize, Serialize};

/// 表单字段（前端 manifest 驱动渲染）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FormField {
    /// 字段 key（对应 CreateConnectionRequest 字段）。
    pub key: String,
    /// 展示文案。
    pub label: String,
    /// 控件类型：text | number | password | select。
    pub field_type: String,
    pub required: bool,
    pub default: Option<String>,
    pub placeholder: Option<String>,
    /// select 类型的候选项。
    pub options: Vec<String>,
}

/// 连接类型能力声明（目标态能力，非「已实现」，见 top 注释）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// 客户端能力：query/schema/rows/edit/export/import。
    pub client: Vec<String>,
    /// 同步能力：batch/increment/cdc（M4 按引擎 connector 交集裁剪）。
    pub sync: Vec<String>,
}

/// 单个连接类型 manifest。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnTypeManifest {
    /// 类型标识：mysql/postgres/…
    pub name: String,
    /// 展示名：MySQL/PostgreSQL/…
    pub label: String,
    /// 大类：sql | nosql | cache。
    pub kind: String,
    /// 驱动 URL 模板（客户端侧，M2 起用于驱动参数推导）。
    pub url_template: String,
    /// 当前版本后端驱动是否已实现（test/query 可用）。
    pub implemented: bool,
    pub form_fields: Vec<FormField>,
    pub capabilities: Capabilities,
}

fn field(key: &str, label: &str, field_type: &str, required: bool, placeholder: &str) -> FormField {
    FormField {
        key: key.into(),
        label: label.into(),
        field_type: field_type.into(),
        required,
        default: None,
        placeholder: Some(placeholder.into()),
        options: Vec::new(),
    }
}

fn select_field(key: &str, label: &str, options: &[&str], default: &str) -> FormField {
    FormField {
        key: key.into(),
        label: label.into(),
        field_type: "select".into(),
        required: false,
        default: Some(default.into()),
        placeholder: None,
        options: options.iter().map(|s| s.to_string()).collect(),
    }
}

/// 全部受支持连接类型（首批 6 库，D7）。实现期按 milestone 逐步点亮 `implemented`。
pub fn all_manifests() -> Vec<ConnTypeManifest> {
    vec![
        ConnTypeManifest {
            name: "mysql".into(),
            label: "MySQL".into(),
            kind: "sql".into(),
            url_template: "mysql://{host}:{port}/{database}".into(),
            implemented: true,
            form_fields: vec![
                field("host", "主机", "text", true, "如 mysql-service 或 10.0.1.5（集群内可达地址）"),
                field("port", "端口", "number", false, "默认 3306"),
                field("database", "数据库", "text", false, "目标库名"),
                field("username", "用户名", "text", false, "连接用户"),
                field("password", "密码", "password", false, "连接密码（密文落库）"),
                select_field("ssl_mode", "SSL", &["disable", "require", "verify-ca", "verify-full"], "disable"),
            ],
            capabilities: Capabilities {
                client: vec!["query".into(), "schema".into(), "rows".into(), "edit".into(), "export".into(), "import".into()],
                sync: vec!["batch".into(), "increment".into(), "cdc".into()],
            },
        },
        ConnTypeManifest {
            name: "postgres".into(),
            label: "PostgreSQL".into(),
            kind: "sql".into(),
            url_template: "postgres://{host}:{port}/{database}".into(),
            implemented: true,
            form_fields: vec![
                field("host", "主机", "text", true, "如 pg-service 或 10.0.1.6"),
                field("port", "端口", "number", false, "默认 5432"),
                field("database", "数据库", "text", false, "目标库名"),
                field("username", "用户名", "text", false, "连接用户"),
                field("password", "密码", "password", false, "连接密码（密文落库）"),
                select_field("ssl_mode", "SSL", &["disable", "require", "verify-ca", "verify-full"], "disable"),
            ],
            capabilities: Capabilities {
                client: vec!["query".into(), "schema".into(), "rows".into(), "edit".into(), "export".into(), "import".into()],
                sync: vec!["batch".into(), "increment".into()],
            },
        },
        ConnTypeManifest {
            name: "sqlserver".into(),
            label: "SQL Server".into(),
            kind: "sql".into(),
            url_template: "sqlserver://{host}:{port};database={database}".into(),
            implemented: false,
            form_fields: vec![
                field("host", "主机", "text", true, "如 sqlsrv-service"),
                field("port", "端口", "number", false, "默认 1433"),
                field("database", "数据库", "text", false, "目标库名"),
                field("username", "用户名", "text", false, "连接用户"),
                field("password", "密码", "password", false, "连接密码"),
                select_field("ssl_mode", "SSL", &["disable", "require"], "disable"),
            ],
            capabilities: Capabilities {
                client: vec!["query".into(), "schema".into(), "rows".into(), "edit".into(), "export".into(), "import".into()],
                sync: vec!["batch".into(), "increment".into()],
            },
        },
        ConnTypeManifest {
            name: "mongodb".into(),
            label: "MongoDB".into(),
            kind: "nosql".into(),
            url_template: "mongodb://{host}:{port}/{database}".into(),
            implemented: false,
            form_fields: vec![
                field("host", "主机", "text", true, "如 mongo-service"),
                field("port", "端口", "number", false, "默认 27017"),
                field("database", "数据库", "text", false, "目标库名"),
                field("username", "用户名", "text", false, "连接用户"),
                field("password", "密码", "password", false, "连接密码"),
                select_field("ssl_mode", "SSL", &["disable", "require"], "disable"),
            ],
            capabilities: Capabilities {
                client: vec!["query".into(), "schema".into(), "rows".into(), "export".into(), "import".into()],
                sync: vec!["batch".into(), "increment".into(), "cdc".into()],
            },
        },
        ConnTypeManifest {
            name: "redis".into(),
            label: "Redis".into(),
            kind: "cache".into(),
            url_template: "redis://{host}:{port}".into(),
            implemented: false,
            form_fields: vec![
                field("host", "主机", "text", true, "如 redis-service"),
                field("port", "端口", "number", false, "默认 6379"),
                field("database", "DB 索引", "number", false, "默认 0"),
                field("username", "用户名", "text", false, "ACL 用户"),
                field("password", "密码", "password", false, "连接密码"),
            ],
            capabilities: Capabilities {
                client: vec!["query".into(), "rows".into(), "export".into()],
                sync: vec!["batch".into(), "cdc".into()],
            },
        },
        ConnTypeManifest {
            name: "elasticsearch".into(),
            label: "Elasticsearch".into(),
            kind: "nosql".into(),
            url_template: "http://{host}:{port}".into(),
            implemented: false,
            form_fields: vec![
                field("host", "主机", "text", true, "如 es-service"),
                field("port", "端口", "number", false, "默认 9200"),
                field("database", "索引前缀", "text", false, "目标索引（可用 * 通配）"),
                field("username", "用户名", "text", false, "连接用户"),
                field("password", "密码", "password", false, "连接密码"),
                select_field("ssl_mode", "SSL", &["disable", "require"], "disable"),
            ],
            capabilities: Capabilities {
                client: vec!["query".into(), "schema".into(), "rows".into(), "export".into(), "import".into()],
                sync: vec!["batch".into(), "increment".into()],
            },
        },
    ]
}

/// 按 name 取 manifest。
pub fn manifest_by_name(name: &str) -> Option<ConnTypeManifest> {
    all_manifests().into_iter().find(|m| m.name == name)
}

/// 判断连接类型是否受支持。
pub fn is_supported_type(name: &str) -> bool {
    manifest_by_name(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_six_types_declared() {
        let manifests = all_manifests();
        let names: Vec<String> = manifests.iter().map(|m| m.name.clone()).collect();
        assert_eq!(
            names,
            vec![
                "mysql".to_string(),
                "postgres".to_string(),
                "sqlserver".to_string(),
                "mongodb".to_string(),
                "redis".to_string(),
                "elasticsearch".to_string(),
            ]
        );
    }

    #[test]
    fn mysql_has_password_field() {
        let m = manifest_by_name("mysql").unwrap();
        assert!(m.form_fields.iter().any(|f| f.key == "password" && f.field_type == "password"));
        assert!(m.form_fields.iter().any(|f| f.key == "host" && f.required));
    }

    #[test]
    fn only_mysql_postgres_implemented_in_m1() {
        for m in all_manifests() {
            if matches!(m.name.as_str(), "mysql" | "postgres") {
                assert!(m.implemented, "{} 应标为已实现", m.name);
            } else {
                assert!(!m.implemented, "{} M1 尚未实现驱动", m.name);
            }
        }
    }
}
