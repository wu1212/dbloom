---
title: dbloom 持久化架构
updated_at_ms: 1791259484762
tags: [dbloom, 持久化, sqlite, volume]
---
结论：dbloom 不引入外部数据库，持久化采用「共享存储卷 + SQLite」方案。所有持久化数据（SQLite 文件、日志目录、上传下载文件、自定义 jar 包路径等）放在 docker volume 或 k8s PVC 中；多节点部署时这些数据必须共享，保证各节点看到同一份状态。

约束：
- 存储卷需支持多节点挂载（如 NFS、云盘），避免节点本地磁盘导致数据分裂。
- SQLite 文件放在共享卷上，需确认文件锁在目标存储上的行为，避免并发写冲突。