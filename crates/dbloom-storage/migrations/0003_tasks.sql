-- ============ M3: 同步任务（D4/D5，最小垂直切片） ============
-- 设计：docs/design/01-data-model.md §2 tasks/task_runs；接口：02-api.md §2.6。
-- 幂等：task_runs 增加 idempotency_key 唯一索引（同请求重复提交不重复建 run）。

CREATE TABLE tasks (
  id            BIGINT AUTO_INCREMENT PRIMARY KEY,
  owner_user_id BIGINT NOT NULL,              -- 租户列
  name          VARCHAR(255) NOT NULL,
  description   VARCHAR(1024),
  source_connection_id BIGINT NOT NULL,
  sink_connection_id   BIGINT NOT NULL,
  sync_mode     VARCHAR(16)  NOT NULL DEFAULT 'batch',
                                        -- 'batch'|'increment'|'cdc'（D4：透传 ST 能力）
  config_hocon  TEXT NOT NULL,                -- 渲染后的 ST HOCON **脱敏快照**（不含密码，提交即锁定）
  table_mapping JSON NOT NULL,                -- 表映射（[{sourceTable, sinkTable}]；触发时据此重建全量 HOCON）
  schedule_cron VARCHAR(64),                  -- cron 表达式；NULL=仅手动
  enabled       TINYINT(1)   NOT NULL DEFAULT 1,   -- 调度开关
  timeout_sec   INT          NOT NULL DEFAULT 0,   -- 0=不限
  retry_times   INT          NOT NULL DEFAULT 0,   -- 失败重试次数（D5）
  concurrency_limit INT       NOT NULL DEFAULT 1,  -- 同任务并发实例上限
  created_by    BIGINT,
  created_at    BIGINT NOT NULL,
  updated_at    BIGINT NOT NULL,
  deleted_at    BIGINT,
  CONSTRAINT fk_tasks_source FOREIGN KEY (source_connection_id) REFERENCES connections(id),
  CONSTRAINT fk_tasks_sink   FOREIGN KEY (sink_connection_id)   REFERENCES connections(id),
  CONSTRAINT fk_tasks_owner  FOREIGN KEY (owner_user_id)        REFERENCES users(id)
);
CREATE INDEX idx_tasks_owner ON tasks(owner_user_id, deleted_at);

-- 任务运行实例
CREATE TABLE task_runs (
  id                BIGINT AUTO_INCREMENT PRIMARY KEY,
  task_id           BIGINT NOT NULL,
  owner_user_id     BIGINT NOT NULL,          -- 冗余租户列，加速过滤（D8）
  trigger_type      VARCHAR(16) NOT NULL,     -- 'manual'|'cron'|'dag'
  idempotency_key   VARCHAR(128) NOT NULL,    -- 幂等键（手动触发生成；重复提交命中同一 run）
  sea_tunnel_job_id VARCHAR(128),             -- 引擎侧 job id（提交后回填）
  status            VARCHAR(16)  NOT NULL DEFAULT 'pending',
                                -- 'pending'|'running'|'succeeded'|'failed'|'stopped'|'canceled'
  attempt           INT          NOT NULL DEFAULT 1,   -- 第几次尝试（重试 D5）
  start_time        BIGINT,
  end_time          BIGINT,
  error_message     TEXT,
  log_path          VARCHAR(1024),            -- 共享卷 logs/tasks/<runId>.log（D22）
  created_at        BIGINT NOT NULL,
  updated_at        BIGINT NOT NULL,
  CONSTRAINT uq_runs_idem   UNIQUE (idempotency_key),
  CONSTRAINT fk_runs_task FOREIGN KEY (task_id) REFERENCES tasks(id)
);
CREATE INDEX idx_runs_task ON task_runs(task_id, id);
CREATE INDEX idx_runs_owner ON task_runs(owner_user_id, id);
CREATE INDEX idx_runs_status ON task_runs(status);

-- 运行日志摘要/游标（WebSocket tail 用；预留在 T1 建表，避免 M4 再迁移）
CREATE TABLE task_run_logs (
  run_id       BIGINT PRIMARY KEY,
  last_line    BIGINT NOT NULL DEFAULT 0,     -- 已推送行号游标
  log_size     BIGINT NOT NULL DEFAULT 0,
  updated_at   BIGINT NOT NULL,
  CONSTRAINT fk_rlog_run FOREIGN KEY (run_id) REFERENCES task_runs(id)
);
