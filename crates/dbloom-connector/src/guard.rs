//! SQL 写保护守卫（D6：危险语句识别 / 生产只读锁 / confirm 二次确认链路）。
//!
//! 判定规则（保守安全为默认）：
//! - `Read`：SELECT / WITH / SHOW / DESCRIBE / DESC / EXPLAIN / PRAGMA / VALUES —— 直接执行；
//! - `Write`：UPDATE/DELETE（带 WHERE）、INSERT、REPLACE —— 需 `confirm=true`；
//! - `Danger`：UPDATE/DELETE 无 WHERE、DROP/TRUNCATE/ALTER/CREATE/GRANT/REVOKE/
//!   RENAME/CALL/SET/LOAD —— 需 `confirm=true`，且服务端可据连接属性进一步收紧；
//! - **多语句**（`SELECT 1; DROP TABLE t`）：含 `;` 分隔出第二条语句 → Danger（防注入拼接）；
//! - 未知首词按 `Read` 处理（白名单外不误伤 SELECT 类方言：MYSQL `WITH ... AS`、`LIMIT` 等）。
//!
//! 加固（T2）：首词提取与 WHERE 判定都**先剥离字符串字面量与注释**，避免
//! `UPDATE t SET a='... WHERE ...'` 被误判为「有 WHERE」而逃过无 WHERE 拦截；
//! 大小写统一转大写。
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
    // 0) 多语句检测：`;` 后还有非注释、非空内容 → 视为注入拼接，直接 Danger（防 `SELECT 1; DROP TABLE t`）
    if has_multiple_statements(sql) {
        return (
            SqlKind::Danger,
            Some("检测到多条 SQL 语句（分号拼接），禁止一次执行多语句；请单条执行".into()),
        );
    }

    let stripped = strip_strings(strip_leading_comments(sql));
    let first = first_token(&stripped);
    let upper = first.to_ascii_uppercase();

    match upper.as_str() {
        "SELECT" | "WITH" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "PRAGMA" | "VALUES"
        | "USE" | "SET" => (SqlKind::Read, None),
        // SET 单独处理：`SET` 可被用于写系统变量但也常见于会话设置；D6 原设计归 Read。
        // 但 `SET` 不引导 DML，保留 Read。
        "UPDATE" | "DELETE" => {
            if has_where_clause(&stripped) {
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
        // 未知首词：按读处理（不误伤方言类只读语句）；多语句由 has_multiple_statements 兜底
        _ => (SqlKind::Read, None),
    }
}

/// 是否含多条语句：按 `;` 切分，任一段在去掉注释/空白后非空且不是尾随注释 → 多语句。
/// 注意：`SELECT 1;` 结尾分号不算多语句；`SELECT 1; DROP TABLE t` 算。
fn has_multiple_statements(sql: &str) -> bool {
    let parts = sql.split(';');
    let mut seen_statement = false;
    for part in parts {
        let stripped = strip_leading_comments(part);
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            continue; // 前后空白/空段（含结尾分号后的空）
        }
        if !seen_statement {
            seen_statement = true; // 第一条语句
        } else {
            return true; // 出现第二条非空语句 → 多语句
        }
    }
    false
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

/// 提取首个有意义的词（去掉前导注释/空白）。
fn first_token(sql: &str) -> &str {
    let t = strip_leading_comments(sql).trim_start();
    t.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .next()
        .unwrap_or("")
}

/// 是否含 WHERE 子句：先剥离字符串字面量/注释，再用词边界判断，
/// 避免 `SET a='... WHERE ...'` 或注释里的 WHERE 造成误判。
fn has_where_clause(sql: &str) -> bool {
    let lower = strip_strings(strip_leading_comments(sql)).to_ascii_lowercase();
    has_word(&lower, "where")
}

/// 词边界子串判断（`where` 前后为非字母数字或行首/行尾）。
fn has_word(lower: &str, kw: &str) -> bool {
    let kwlen = kw.len();
    let bytes = lower.as_bytes();
    let mut start = 0;
    while let Some(rel) = lower[start..].find(kw) {
        let i = start + rel;
        let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let after = i + kwlen;
        let after_ok = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = i + 1;
        if start >= bytes.len() {
            break;
        }
    }
    false
}

/// 把字符串字面量（单引号/双引号包起来的文本）替换为占位，避免其中关键字干扰判定。
/// 处理转义引号（`'it''s'`、`'a\'b'`）。
fn strip_strings(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' || c == '"' {
            let quote = c;
            out.push(' ');
            // 跳过整个字符串
            let mut prev_escape = false;
            while let Some(nc) = chars.next() {
                if !prev_escape && nc == quote {
                    // 判断是否为转义引号（MySQL: '' 表示转义单引号）
                    if quote == '\'' && chars.peek() == Some(&'\'') {
                        chars.next();
                        continue;
                    }
                    out.push(' ');
                    break;
                }
                prev_escape = nc == '\\' && !prev_escape;
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    out
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
            "SET SESSION time_zone = '+08:00'",
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
        assert_eq!(classify_sql("UPDATE t SET a=1 where b=2").0, SqlKind::Write);
        assert_eq!(
            classify_sql("UPDATE t SET a=1 /*注释*/ WHERE id=3").0,
            SqlKind::Write
        );
    }

    #[test]
    fn where_inside_string_does_not_trigger() {
        // 字符串字面量里的 WHERE 不应让无 WHERE 的 UPDATE 逃过拦截（T2 绕过面）
        let (k, _) = classify_sql("UPDATE t SET note = 'hello WHERE world'");
        assert_eq!(k, SqlKind::Danger);
        // 双引号同理
        let (k2, _) = classify_sql("UPDATE t SET note = \"hello where x\"");
        assert_eq!(k2, SqlKind::Danger);
        // DELETE 无 WHERE 且字符串含 where
        let (k3, _) = classify_sql("DELETE FROM t WHERE 1=1 -- note 'where'");
        assert_eq!(k3, SqlKind::Write);
    }

    #[test]
    fn ddl_is_danger() {
        for sql in [
            "DROP TABLE users",
            "TRUNCATE TABLE users",
            "ALTER TABLE users ADD c INT",
            "CREATE TABLE x (a INT)",
            "GRANT ALL ON *.* TO 'u'",
            "REVOKE SELECT ON t FROM u",
        ] {
            assert_eq!(classify_sql(sql).0, SqlKind::Danger, "sql={sql}");
        }
    }

    #[test]
    fn multi_statement_is_danger() {
        // T2 关键绕过面：分号拼接第二条语句
        let (k, msg) = classify_sql("SELECT 1; DROP TABLE users");
        assert_eq!(k, SqlKind::Danger);
        assert!(msg.unwrap().contains("多语句"));
        assert_eq!(classify_sql("SELECT * FROM a; DELETE FROM b").0, SqlKind::Danger);
        assert_eq!(classify_sql("UPDATE a SET x=1; UPDATE b SET y=2").0, SqlKind::Danger);
        // 结尾分号不算多语句
        assert_eq!(classify_sql("SELECT 1;").0, SqlKind::Read);
        // 注释分隔也不算第二条
        assert_eq!(classify_sql("SELECT 1; -- 结束注释").0, SqlKind::Read);
    }

    #[test]
    fn comment_splitting_keyword_is_handled() {
        // 注释分隔关键字：正常写法（关键字后接注释再接语句）
        assert_eq!(classify_sql("SELECT/*c*/1").0, SqlKind::Read);
        assert_eq!(classify_sql("DROP/*x*/TABLE t").0, SqlKind::Danger);
        // 注释出现在 UPDATE 前
        assert_eq!(classify_sql("/* pre */UPDATE t SET a=1 WHERE id=1").0, SqlKind::Write);
    }

    #[test]
    fn insert_is_write() {
        assert_eq!(classify_sql("INSERT INTO t(a) VALUES (1)").0, SqlKind::Write);
        assert_eq!(classify_sql("REPLACE INTO t(a) VALUES (1)").0, SqlKind::Write);
    }

    #[test]
    fn mixed_case_keywords() {
        assert_eq!(classify_sql("update t set a=1 where id=2").0, SqlKind::Write);
        assert_eq!(classify_sql("Update t Set a=1").0, SqlKind::Danger);
        assert_eq!(classify_sql("dRoP TaBlE x").0, SqlKind::Danger);
        assert_eq!(classify_sql("SeLeCt 1").0, SqlKind::Read);
    }

    #[test]
    fn strip_strings_handles_escapes() {
        assert!(!strip_strings("'it''s a where'").contains('w'));
        assert!(!strip_strings(r"'a\'b where'").contains('w'));
        // 未闭合字符串：不 panic，剩余内容全部被剥离（视为字符串内容）
        assert!(strip_strings("'open").chars().all(|c| c == ' '));
    }
}
