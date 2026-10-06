-- dbloom 元数据库迁移 v1（MySQL 8）
-- 对应 docs/design/01-data-model.md §2 —— M0 落地 4 张表：users/sessions/api_keys/audit_logs。
-- 其余业务表（connections/tasks/...）随 M1+ 各自迁移文件落地。
-- 时间统一 BIGINT Unix 毫秒（UTC）。

-- ============================ 用户与认证 ============================
CREATE TABLE IF NOT EXISTS users (
  id                   BIGINT AUTO_INCREMENT PRIMARY KEY,
  username             VARCHAR(128) NOT NULL,
  password_hash        VARCHAR(255) NOT NULL,          -- argon2 hash
  role                 VARCHAR(16)  NOT NULL DEFAULT 'user',  -- 'admin'|'user'
  status               VARCHAR(16)  NOT NULL DEFAULT 'active', -- 'active'|'locked'|'disabled'
  display_name         VARCHAR(255),
  must_change_password TINYINT(1)   NOT NULL DEFAULT 0,  -- 1=首登/被重置后强制改密 D20
  failed_login_count   INT          NOT NULL DEFAULT 0,  -- 连续失败次数 D19
  locked_until         BIGINT,                           -- 锁到该时间（ms，NULL=未锁）
  created_by           BIGINT,                           -- 管理员 id（内置 admin 为 NULL）
  last_login_at        BIGINT,
  created_at           BIGINT NOT NULL,
  updated_at           BIGINT NOT NULL,
  deleted_at           BIGINT,
  CONSTRAINT uq_users_username UNIQUE (username)         -- 软删时用户名匿名化保证唯一
);

CREATE TABLE IF NOT EXISTS sessions (
  id           BIGINT AUTO_INCREMENT PRIMARY KEY,
  user_id      BIGINT NOT NULL,
  refresh_hash VARCHAR(128) NOT NULL,                   -- 只存哈希，不存明文
  expires_at   BIGINT NOT NULL,
  revoke_reason VARCHAR(64),                            -- NULL=未吊销；'logout'|'password_reset'|'expired'...
  created_at   BIGINT NOT NULL,
  last_used_at BIGINT,
  INDEX idx_sessions_user (user_id),
  INDEX idx_sessions_expiry (expires_at)
);

-- ============================ API Key（D9）============================
CREATE TABLE IF NOT EXISTS api_keys (
  id           BIGINT AUTO_INCREMENT PRIMARY KEY,
  user_id      BIGINT NOT NULL,                          -- 所属普通用户
  created_by   BIGINT NOT NULL,                          -- 签发管理员
  name         VARCHAR(255) NOT NULL,                    -- 用途备注
  key_hash     VARCHAR(128) NOT NULL,                    -- SHA-256(key 明文) D9
  prefix       VARCHAR(32)  NOT NULL,                    -- 展示前缀 dbk_xxxx…
  status       VARCHAR(16)  NOT NULL DEFAULT 'enabled',  -- 'enabled'|'disabled'
  valid_from   BIGINT,                                   -- 生效时间（NULL=即刻）
  valid_until  BIGINT,                                   -- 失效时间（NULL=永久）
  last_used_at BIGINT,
  created_at   BIGINT NOT NULL,
  updated_at   BIGINT NOT NULL,
  deleted_at   BIGINT,
  INDEX idx_api_keys_user (user_id, deleted_at)
);

-- ============================ 审计（D21）============================
CREATE TABLE IF NOT EXISTS audit_logs (
  id            BIGINT AUTO_INCREMENT PRIMARY KEY,
  actor_user_id BIGINT,                                  -- 操作者（机器=Key 绑定用户）
  actor_type    VARCHAR(16)  NOT NULL DEFAULT 'user',    -- 'user'|'api_key'
  action        VARCHAR(64)  NOT NULL,                   -- login|logout|user_create|apikey_issue|...
  resource_type VARCHAR(64),
  resource_id   VARCHAR(128),
  detail_json   JSON,                                    -- 额外上下文（摘要）
  ip            VARCHAR(64),
  created_at    BIGINT NOT NULL,
  INDEX idx_audit_actor (actor_user_id, id),
  INDEX idx_audit_created (created_at)
);
