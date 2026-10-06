//! HOCON 生成器：把「源连接 + 目标连接 + 表映射 + 同步类型」渲染成
//! SeaTunnel 可执行的 HOCON（env/source/sink 三段），并做敏感字段脱敏快照。
//!
//! 设计依据：
//! - `docs/design/03-modules.md` §3：连接 manifest 单一事实来源，HOCON 能力同源；
//! - `docs/design/01-data-model.md` §3.2：`config_hocon` 快照（提交即锁定）；
//! - `docs/design/02-api.md` §2.6：创建任务时渲染 HOCON 留存快照。
//! - 安全：**连接参数来自已加密存储的连接配置**（服务端解密后进入本生成器），
//!   快照中不含密码（用例 2：快照可回看且不含明文密码）。

use serde_json::Value;
use std::collections::BTreeMap;

/// 一条表映射：源表 → 目标表（全量透传，表名按原样写入 HOCON）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableMapping {
    pub source_table: String, // 源库表全名（含 database.table 或仅 table）
    pub sink_table: String,   // 目标库表全名
}

/// 同步类型（M4 前仅全量 batch；increment/cdc 透传引擎原生能力，见 06-milestones M4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Batch,     // 全量
    Increment, // 增量（M4）
    Cdc,       // 变更捕获（M4）
}

impl SyncMode {
    pub fn parse(s: &str) -> SyncMode {
        match s {
            "increment" => SyncMode::Increment,
            "cdc" => SyncMode::Cdc,
            _ => SyncMode::Batch,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncMode::Batch => "batch",
            SyncMode::Increment => "increment",
            SyncMode::Cdc => "cdc",
        }
    }
}

/// JDBC 连接参数（服务端从已存储连接解密后传入；密码仅内存，不落快照）。
#[derive(Debug, Clone)]
pub struct JdbcConn {
    pub conn_type: String,   // mysql|postgres|sqlserver
    pub host: String,
    pub port: Option<i32>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// JDBC 默认端口。
fn default_port(conn_type: &str) -> Option<i32> {
    match conn_type {
        "mysql" => Some(3306),
        "postgres" => Some(5432),
        "sqlserver" => Some(1433),
        _ => None,
    }
}

/// 构造 JDBC URL。
pub fn jdbc_url(c: &JdbcConn) -> String {
    let port = c.port.or_else(|| default_port(&c.conn_type));
    let host = if c.host.contains(':') && !c.host.starts_with('[') {
        format!("[{}]", c.host) // IPv6
    } else {
        c.host.clone()
    };
    let authority = match port {
        Some(p) => format!("{host}:{p}"),
        None => host,
    };
    let db = c.database.clone().unwrap_or_default();
    match c.conn_type.as_str() {
        "mysql" => format!("jdbc:mysql://{authority}/{db}"),
        "postgres" => format!("jdbc:postgresql://{authority}/{db}"),
        "sqlserver" => format!("jdbc:sqlserver://{authority};databaseName={db}"),
        other => format!("jdbc:{other}://{authority}/{db}"),
    }
}

/// JDBC driver 类名（to SeaTunnel）。
pub fn jdbc_driver(conn_type: &str) -> &'static str {
    match conn_type {
        "mysql" => "com.mysql.cj.jdbc.Driver",
        "postgres" => "org.postgresql.Driver",
        "sqlserver" => "com.microsoft.sqlserver.jdbc.SQLServerDriver",
        _ => "unknown",
    }
}

/// HOCON 字符串转义（HOCON 字符串与 JSON 字符串语法兼容；对密码/URL/表名做引号与反斜杠转义）。
fn hocon_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// 生成结果。
#[derive(Debug, Clone)]
pub struct RenderedHocon {
    /// 可执行 HOCON 全文（含密码，仅进程内、提交给引擎用）。
    pub full_hocon: String,
    /// 脱敏快照（密码替换为 `***`；落库/回看用）。
    pub snapshot_hocon: String,
}

/// 渲染 mysql/postgres 全量 JDBC→JDBC 同步 HOCON。
///
/// `source`/`sink` 均要求 JDBC 类型（mysql/postgres/sqlserver）才能走 Jdbc connector；
/// 非 JDBC 类型由上层返回明确错误（T1 首批 = mysql），本函数对未知类型返回 Err。
pub fn render_full_jdbc(
    source: &JdbcConn,
    sink: &JdbcConn,
    mappings: &[TableMapping],
    extra_env: &BTreeMap<String, String>,
) -> Result<RenderedHocon, String> {
    if mappings.is_empty() {
        return Err("表映射不能为空".into());
    }
    for m in mappings {
        if m.source_table.trim().is_empty() || m.sink_table.trim().is_empty() {
            return Err("表映射不能含空表名".into());
        }
    }
    if !matches!(source.conn_type.as_str(), "mysql" | "postgres" | "sqlserver") {
        return Err(format!("源连接类型 {} 暂不支持 JDBC 全量同步（首批: mysql/postgres/sqlserver）", source.conn_type));
    }
    if !matches!(sink.conn_type.as_str(), "mysql" | "postgres" | "sqlserver") {
        return Err(format!("目标连接类型 {} 暂不支持 JDBC 全量同步（首批: mysql/postgres/sqlserver）", sink.conn_type));
    }

    // ---- env 段 ----
    let mut env_lines = vec![
        format!("parallelism = {}", extra_env.get("parallelism").map(|v| v.as_str()).unwrap_or("1")),
        format!("job.mode = {}", hocon_str(extra_env.get("job.mode").map(|v| v.as_str()).unwrap_or("BATCH"))),
    ];
    if let Some(ck) = extra_env.get("checkpoint.interval") {
        env_lines.push(format!("checkpoint.interval = {}", hocon_str(ck)));
    }
    let env_block = format!("  {{\n    {}\n  }}", env_lines.join("\n    "));

    // ---- source 段（每张源表一个 Jdbc source，result_table_name = 源表） ----
    let mut source_blocks: Vec<String> = Vec::new();
    for m in mappings {
        source_blocks.push(render_source(&source, &m.source_table));
    }
    let source_block = if source_blocks.len() == 1 {
        source_blocks[0].clone()
    } else {
        let inner = source_blocks.join("\n  ");
        format!("  {inner}")
    };

    // ---- sink 段（每张目标表一个 Jdbc sink，延迟依据 source 的 result_table_name） ----
    let mut sink_blocks: Vec<String> = Vec::new();
    for (i, m) in mappings.iter().enumerate() {
        sink_blocks.push(render_sink(&sink, &m.sink_table, &m.source_table));
        let _ = i;
    }
    let sink_block = if sink_blocks.len() == 1 {
        sink_blocks[0].clone()
    } else {
        let inner = sink_blocks.join("\n  ");
        format!("  {inner}")
    };

    let full_hocon = format!(
        "env {env_block}\n\nsource {{\n  {source_block}\n}}\n\nsink {{\n  {sink_block}\n}}\n"
    );

    // ---- 脱敏快照：把 password 行替换为 *** ----
    let mut snapshot = String::new();
    for line in full_hocon.lines() {
        let t = line.trim_start();
        if t.starts_with("password") && t.contains('=') {
            let indent = &line[..line.len() - t.len()];
            snapshot.push_str(&format!("{indent}password = \"***\"\n"));
        } else {
            snapshot.push_str(line);
            snapshot.push('\n');
        }
    }

    Ok(RenderedHocon {
        full_hocon,
        snapshot_hocon: snapshot,
    })
}

fn render_source(src: &JdbcConn, source_table: &str) -> String {
    let url = jdbc_url(src);
    let user = hocon_str(src.username.as_deref().unwrap_or(""));
    let password = hocon_str(src.password.as_deref().unwrap_or(""));
    // 表路径：用户传 `database.table` 则原样；仅表名时自动补连接库前缀。
    // 引擎 Jdbc 多表 source 的 table_path 需含库名（如 `dbx_src.t1_src`），
    // 否则 source 工厂初始化失败（API-06 Factory initialize failed）。
    let table_path = qualify_table(src.database.as_deref().unwrap_or(""), source_table);
    let source_table_name = table_short_name(source_table);
    format!(
        "Jdbc {{\n      url = {url}\n      driver = {driver}\n      user = {user}\n      password = {password}\n      table_list = [\n        {{\n          table_path = {table_path}\n        }}\n      ]\n      result_table_name = {sn}\n    }}",
        url = hocon_str(&url),
        driver = hocon_str(jdbc_driver(&src.conn_type)),
        user = user,
        password = password,
        table_path = hocon_str(&table_path),
        sn = hocon_str(source_table_name),
    )
}

fn render_sink(sink: &JdbcConn, sink_table: &str, source_table: &str) -> String {
    let url = jdbc_url(sink);
    let user = hocon_str(sink.username.as_deref().unwrap_or(""));
    let password = hocon_str(sink.password.as_deref().unwrap_or(""));
    // 目标库/表：用户传 `database.table` 则拆分；仅表名时 database 用连接库。
    let (db, table) = split_database_table(sink_table);
    let db = if db.is_empty() { sink.database.clone().unwrap_or_default() } else { db };
    let source_name = table_short_name(source_table);
    format!(
        "Jdbc {{\n      url = {url}\n      driver = {driver}\n      user = {user}\n      password = {password}\n      source_table_name = {sn}\n      generate_sink_sql = true\n      database = {db}\n      table = {table}\n      schema_save_mode = \"CREATE_SCHEMA_WHEN_NOT_EXIST\"\n      data_save_mode = \"APPEND_DATA\"\n    }}",
        url = hocon_str(&url),
        driver = hocon_str(jdbc_driver(&sink.conn_type)),
        user = user,
        password = password,
        sn = hocon_str(&source_name),
        db = hocon_str(&db),
        table = hocon_str(&table),
    )
}

/// 补全表路径：`db.table` 原样；仅表名 → `conn_db.table`；无连接库则原样（留给引擎/用户）。
fn qualify_table(conn_db: &str, table: &str) -> String {
    if table.contains('.') {
        table.to_string()
    } else if conn_db.is_empty() {
        table.to_string()
    } else {
        format!("{conn_db}.{table}")
    }
}

/// 取表名短名（`db.table` → `table`；无点则原样）。
fn table_short_name(s: &str) -> &str {
    s.rsplit('.').next().unwrap_or(s)
}

/// 拆分 `database.table` → (database, table)。缺库名时 database 为空串。
fn split_database_table(s: &str) -> (String, String) {
    match s.rsplit_once('.') {
        Some((db, t)) => (db.to_string(), t.to_string()),
        None => (String::new(), s.to_string()),
    }
}

/// 把表映射 JSON（来自请求体）解析为 Vec<TableMapping>。
pub fn parse_mappings(v: &Value) -> Result<Vec<TableMapping>, String> {
    let arr = v.as_array().ok_or("tableMapping 必须是数组")?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let src = item.get("sourceTable").and_then(Value::as_str).unwrap_or("");
        let sink = item.get("sinkTable").and_then(Value::as_str).unwrap_or("");
        out.push(TableMapping {
            source_table: src.to_string(),
            sink_table: sink.to_string(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mysql_conn() -> JdbcConn {
        JdbcConn {
            conn_type: "mysql".into(),
            host: "127.0.0.1".into(),
            port: Some(3306),
            database: Some("test".into()),
            username: Some("root".into()),
            password: Some("p@ss'\"word".into()),
        }
    }

    #[test]
    fn renders_valid_full_hocon() {
        let src = mysql_conn();
        let sink = mysql_conn();
        let maps = vec![TableMapping {
            source_table: "test.users".into(),
            sink_table: "test.users_copy".into(),
        }];
        let env = BTreeMap::new();
        let r = render_full_jdbc(&src, &sink, &maps, &env).unwrap();
        assert!(r.full_hocon.contains("env "), "HOCON 缺少 env 段: {}", r.full_hocon);
        assert!(r.full_hocon.contains("source {"), "HOCON 缺少 source 段: {}", r.full_hocon);
        assert!(r.full_hocon.contains("sink {"), "HOCON 缺少 sink 段: {}", r.full_hocon);
        assert!(r.full_hocon.contains("job.mode = \"BATCH\""));
        assert!(r.full_hocon.contains("table_path = \"test.users\""));
        assert!(r.full_hocon.contains("table = \"users_copy\""));
        // 密码转义正确（含单双引号；HOCON 串用 JSON 转义：单引号原样，双引号转义为 \"）
        assert!(
            r.full_hocon.contains("p@ss'\\\"word"),
            "密码未按 HOCON 转义: {}",
            r.full_hocon
        );
    }

    #[test]
    fn snapshot_hides_password() {
        let src = mysql_conn();
        let sink = mysql_conn();
        let maps = vec![TableMapping {
            source_table: "test.users".into(),
            sink_table: "test.users_copy".into(),
        }];
        let r = render_full_jdbc(&src, &sink, &maps, &BTreeMap::new()).unwrap();
        assert!(r.snapshot_hocon.contains("password = \"***\""));
        assert!(!r.snapshot_hocon.contains("p@ss"));
        assert!(r.full_hocon.contains("password"));
    }

    #[test]
    fn short_table_auto_qualified_with_conn_db() {
        // 表映射传短表名时，source table_path 自动补连接库前缀、sink database 补连接库。
        let src = mysql_conn(); // database = "test"
        let sink = mysql_conn(); // database = "test"
        let maps = vec![TableMapping {
            source_table: "users".into(),
            sink_table: "users_copy".into(),
        }];
        let r = render_full_jdbc(&src, &sink, &maps, &BTreeMap::new()).unwrap();
        assert!(
            r.full_hocon.contains("table_path = \"test.users\""),
            "source table_path 未补库前缀: {}",
            r.full_hocon
        );
        assert!(
            r.full_hocon.contains("database = \"test\""),
            "sink database 未补连接库: {}",
            r.full_hocon
        );
        assert!(r.full_hocon.contains("table = \"users_copy\""));
        // 快照同样含补全后的表路径（不含密码）
        assert!(r.snapshot_hocon.contains("table_path = \"test.users\""));
    }

    #[test]
    fn rejects_empty_mapping() {
        let src = mysql_conn();
        let sink = mysql_conn();
        let e = render_full_jdbc(&src, &sink, &[], &BTreeMap::new()).unwrap_err();
        assert!(e.contains("不能为空"));
    }

    #[test]
    fn rejects_non_jdbc_type() {
        let src = mysql_conn();
        let sink = JdbcConn {
            conn_type: "redis".into(),
            host: "127.0.0.1".into(),
            port: Some(6379),
            database: None,
            username: None,
            password: None,
        };
        let maps = vec![TableMapping {
            source_table: "t".into(),
            sink_table: "t".into(),
        }];
        assert!(render_full_jdbc(&src, &sink, &maps, &BTreeMap::new()).is_err());
    }

    #[test]
    fn parse_mappings_json() {
        let v = json!([{"sourceTable": "db.a", "sinkTable": "db.b"}]);
        let maps = parse_mappings(&v).unwrap();
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].source_table, "db.a");
        assert_eq!(maps[0].sink_table, "db.b");
    }

    #[test]
    fn jdbc_url_defaults() {
        assert_eq!(
            jdbc_url(&JdbcConn {
                conn_type: "mysql".into(),
                host: "h".into(),
                port: None,
                database: Some("db".into()),
                username: None,
                password: None,
            }),
            "jdbc:mysql://h:3306/db"
        );
        assert_eq!(
            jdbc_url(&JdbcConn {
                conn_type: "postgres".into(),
                host: "h".into(),
                port: Some(5433),
                database: None,
                username: None,
                password: None,
            }),
            "jdbc:postgresql://h:5433/"
        );
    }
}
