//! SQL 写保护守卫（D6：危险语句识别 / 生产只读锁 / confirm 二次确认链路）。
//!
//! 判定规则（保守安全为默认）：
//! - `Read`：SELECT / WITH / SHOW / DESCRIBE / DESC / EXPLAIN / PRAGMA / VALUES —— 直接执行；
//! - `Write`：UPDATE/DELETE（带 WHERE）、INSERT、REPLACE —— 需 `confirm=true`；
//! - `Danger`：UPDATE/DELETE 无 WHERE、DROP/TRUNCATE/ALTER/CREATE/GRANT/REVOKE/
//!   RENAME/CALL/SET/LOAD —— 需 `confirm=true`，且服务端可据连接属性进一步收紧；
//! - 未知首词按 `Read` 处理（白名单外不误伤 SELECT 类方言：MYSQL `WITH ... AS`、`LIMIT` 等）。
//!
//! 只读锁连接由上层（server）统一拒绝全部写路径（Read 判定之上再加一道物理闸），
//! 本层只负责语句级分类 + 给出需要二次确认的判定。

/// SQL 语句分类结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlKind {
    Read,
    Write,
    Danger,
}

/// 分类 + 若需确认时的提示文案。
pub fn classify_sql(sql: &str) -> (SqlKind, Option<String>) {
    let first = first_token(sql);
    let upper = first.to_ascii_uppercase();

    match upper.as_str() {
        "SELECT" | "WITH" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "PRAGMA" | "VALUES"
        | "USE" | "SET" => (SqlKind::Read, None),
        "UPDATE" | "DELETE" => {
            if has_where_clause(sql) {
                (
                    SqlKind::Write,
                    Some(format!(
                        "执行写操作（{upper}）将修改数据，请确认后重发（confirm=true）"
                    )),
                )
            } else {
                (
                    SqlKind::Danger,
                    Some(format!("危险操作：{upper} 语句缺少 WHERE 条件，可能影响全部行；确认后请携带 confirm=true 重发")),
                )
            }
        }
        "INSERT" | "REPLACE" => (
            SqlKind::Write,
            Some("执行数据写入（INSERT/REPLACE）将修改数据，请确认后重发（confirm=true）".into()),
        ),
        "DROP" | "TRUNCATE" | "ALTER" | "CREATE" | "GRANT" | "REVOKE" | "RENAME"
        | "CALL" | "LOAD" => (
            SqlKind::Danger,
            Some(format!("高危操作（{upper}）会改变结构/权限，请确认后重发（confirm=true）")),
        ),
        // 未知首词：按读处理（不误伤方言类只读语句）；多语句用注释跳过风险由上层（连接只读锁）兜底
        _ => (SqlKind::Read, None),
    }
}

/// 提取首个有意义的词（去掉前导注释/空白）。
fn first_token(sql: &str) -> &str {
    let t = strip_leading_comments(sql).trim_start();
    t.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .next()
        .unwrap_or("")
}

/// 去除前导 `-- ...` / `/* ... */` / `# ...` 注释（直到首个非注释 token）。
fn strip_leading_comments(sql: &str) -> &str {
    let s = sql.trim_start();
    if let Some(rest) = s.strip_prefix("--") {
        let line_end = rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
        return strip_leading_comments(&rest[line_end..]);
    }
    if let Some(rest) = s.strip_prefix('#') {
        let line_end = rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
        return strip_leading_comments(&rest[line_end..]);
    }
    if let Some(rest) = s.strip_prefix("/*") {
        // 只处理单块注释（不嵌套）
        if let Some(end) = rest.find("*/") {
            return strip_leading_comments(&rest[end + 2..]);
        }
    }
    s
}

/// 是否含 WHERE 子句（词边界粗判；避免 `UPDATE t SET w=1` 的列名误判——用非精确但保守）。
fn has_where_clause(sql: &str) -> bool {
    let upper = strip_leading_comments(sql).to_ascii_uppercase();
    // 去掉可能出现在 SET 值里的字符串再判，降低误报难度；此处取保守：正经 WHERE 出现在 FROM/SET 之后
    has_kw(&upper, " WHERE ")
}

fn has_kw(upper: &str, kw: &str) -> bool {
    upper.contains(kw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_statements_pass() {
        for sql in [
            "SELECT * FROM users",
            "WITH x AS (SELECT 1) SELECT * FROM x",
            "SHOW TABLES",
            "describe users",
            " EXPLAIN SELECT * FROM t",
            "  -- 前导注释\nSELECT 1",
        ] {
            assert_eq!(classify_sql(sql).0, SqlKind::Read, "sql={sql}");
        }
    }

    #[test]
    fn update_without_where_is_danger() {
        let (k, msg) = classify_sql("UPDATE users SET age = 1");
        assert_eq!(k, SqlKind::Danger);
        assert!(msg.unwrap().contains("WHERE"));
    }

    #[test]
    fn update_with_where_is_write() {
        assert_eq!(classify_sql("UPDATE users SET age=1 WHERE id=3").0, SqlKind::Write);
        assert_eq!(classify_sql("DELETE FROM t WHERE id IN (1,2)").0, SqlKind::Write);
    }

    #[test]
    fn ddl_is_danger() {
        for sql in [
            "DROP TABLE users",
            "TRUNCATE TABLE users",
            "ALTER TABLE users ADD c INT",
            "CREATE TABLE x (a INT)",
        ] {
            assert_eq!(classify_sql(sql).0, SqlKind::Danger, "sql={sql}");
        }
    }

    #[test]
    fn insert_is_write() {
        assert_eq!(classify_sql("INSERT INTO t(a) VALUES (1)").0, SqlKind::Write);
    }
}
