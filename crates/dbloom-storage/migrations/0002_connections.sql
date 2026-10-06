-- ============ M1: 连接管理（D6/D7/D17） ============
-- 设计：docs/design/01-data-model.md §2 connections；接口：02-api.md §2.4。
-- 凭据加密：password_enc = AES-256-GCM 密文（v1.<salt>.<iv>.<ct>），见 04-security.md §3.1。

CREATE TABLE connections (
  id            BIGINT AUTO_INCREMENT PRIMARY KEY,
  owner_user_id BIGINT NOT NULL,              -- 租户列
  name          VARCHAR(255) NOT NULL,
  conn_type     VARCHAR(32)  NOT NULL,        -- mysql|postgres|sqlserver|mongodb|redis|elasticsearch（D7）
  host          VARCHAR(255) NOT NULL,        -- 可为集群内部 service 名（D17）
  port          INT,
  database_name VARCHAR(255),                 -- 库/索引 名
  username      VARCHAR(255),
  password_enc  TEXT,                         -- AES-256-GCM 密文（base64），主密钥见 04-security §3
  ssl_mode      VARCHAR(32)  DEFAULT 'disable',-- disable|require|verify-ca|verify-full
  extra_params  JSON,                         -- 驱动专有参数/字符集/超时等
  is_production TINYINT(1)   NOT NULL DEFAULT 0,  -- 生产标记（D6）
  read_only_lock TINYINT(1)  NOT NULL DEFAULT 0,  -- 生产只读锁：1=禁止任何写（D6）
  created_at    BIGINT NOT NULL,
  updated_at    BIGINT NOT NULL,
  deleted_at    BIGINT,
  CONSTRAINT fk_conns_owner FOREIGN KEY (owner_user_id) REFERENCES users(id)
);

CREATE INDEX idx_connections_owner ON connections(owner_user_id, deleted_at);
CREATE INDEX idx_connections_type ON connections(conn_type);
