-- ============ M3 修复：tasks.table_mapping 以 TEXT 存储 JSON 文本 ============
-- 原因：0003 将 table_mapping 定义为 JSON 列，而 Rust DAO 以 String(VARCHAR/TEXT) 读写，
-- sqlx 在 MySQL 上对 JSON 列与 Rust String 类型不匹配，create/update 均报
-- "Rust type Option<String> is not compatible with SQL type JSON"。
-- 应用层本来就以 JSON 文本字符串存储（mappings_to_json），故列类型收敛为 TEXT。

ALTER TABLE tasks MODIFY table_mapping TEXT NOT NULL;
