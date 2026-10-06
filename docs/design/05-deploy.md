# dbloom 目标态设计 · 部署（Deploy，v1.1）

> 决策：D1（master+worker / 三形态 / 无 reload）、D2（统一镜像）、D10（外部元数据库 + 共享文件卷，v1.1）、D11（master 与 server 同节点）、D14、D15（HTTP only）。
> **v1.1（2026-10-06，D10 变更）**：元数据用**外部关系库（MySQL/PG）**，部署形态自带编排或连接外部托管实例；共享卷只剩非结构化数据（日志/上传下载文件/自定义 jar/checkpoint）。
> 统一镜像：多阶段构建 = 前端静态资源 + Rust 控制面二进制 + JVM 引擎运行时；同一镜像按角色启动 master/worker。
> 共享文件存储根：**`/dbloom-data`**（所有节点同路径，D10）；**元数据库经 `DB_DSN` 独立于共享卷**。

---

## 0. 元数据库（D10 v1.1）

- **两种连接方式**：① 部署形态自带编排（compose/k8s 内置一个 MySQL 或直接连外部实例）；② 生产常连**外部托管 MySQL/PG**（云 RDS 等），此时三形态只消费 `DB_DSN` 即可，无需自带。
- **连接串**：`DB_DSN=mysql://dbloom:pass@mysql:3306/dbloom`（PG 同理），dbloom-server 启动/迁移用它。
- 高可用由数据库层负责（托管实例/主从），dbloom-server 无状态、可多副本。

## 1. 统一镜像内容与入口

```Dockerfile
# ==== 阶段1：前端 ====
# node → pnpm install → pnpm build → dist/（静态资源）
# ==== 阶段2：Rust 控制面 ====
# rust → cargo build --release → 产出 dbloom-server 二进制
# ==== 阶段3：引擎运行时 ====
# eclipse-temurin:8-jre（或按本地定制分支要求）→ 拷贝 seatunnel/ 构建产物(seatunnel 解包目录)
# ==== 最终镜像 ====
# jre 基础 + dbloom-server + engine 运行时 + apps/web dist + entrypoint 脚本
```

- **entrypoint**（`deploy/docker/entrypoint.sh`）按 `DBLOOM_ROLE` 启动：
  - `DBLOOM_ROLE=master`：先起引擎 `seatunnel-cluster.sh -r master`（后台，等 8080 ready）→ 再起 `dbloom-server`（多副本可并行）；控主进程。
  - `DBLOOM_ROLE=worker`：只起 `seatunnel-cluster.sh -r worker`。
  - 健康检查：master 检查 `dbloom-server` `/api/v1/health`（含引擎 8080、**元数据库连通**、共享卷可写）；worker 检查 Hazelcast。
- 多处挂载共享卷均写成绝对路径 `/dbloom-data/...`（D10 同路径）。

---

## 2. 三种形态

### 2.1 docker（本地/演示）
```text
docker run --name dbloom-master \
  -p 8080:8080 -p 8081:8081 \          # 8081 为 dbloom-server HTTP（对外）；8080 引擎 REST 仅内部
  -e DBLOOM_ROLE=master \
  -e DB_DSN=mysql://dbloom:pass@mysql-host:3306/dbloom \   # 或外部托管 DB
  -e DBLOOM_SECRET_KEY_FILE=/run/secrets/dbloom_secret \
  -e DBLOOM_JWT_SECRET=<随机> \
  -v dbloom-data:/dbloom-data \
  你的镜像:dbloom
# worker（可选，本地单机可只跑 master 自带 worker；worker 不连元数据库）
docker run --name dbloom-worker -e DBLOOM_ROLE=worker -v dbloom-data:/dbloom-data ... 镜像
```

### 2.2 docker compose（推荐开发/轻量生产）
```yaml
services:
  dbloom-db:                     # 可选：自带的元数据库（也可去掉改用外部托管，DB_DSN 指外部）
    image: mysql:8
    environment:
      MYSQL_ROOT_PASSWORD: ${MYSQL_ROOT_PASSWORD}
      MYSQL_DATABASE: dbloom
      MYSQL_USER: dbloom
      MYSQL_PASSWORD: ${DB_PASSWORD}
    volumes:
      - dbloom-db:/var/lib/mysql
    healthcheck: { test: ["CMD","mysqladmin","ping","-h","localhost"], interval: 10s }
  dbloom-master:
    image: dbloom
    depends_on: [dbloom-db]
    environment:
      DBLOOM_ROLE: master
      DB_DSN: mysql://dbloom:${DB_PASSWORD}@dbloom-db:3306/dbloom
      DBLOOM_SECRET_KEY_FILE: /run/secrets/dbloom_secret
      DBLOOM_JWT_SECRET: ${DBLOOM_JWT_SECRET}
    volumes:
      - dbloom-data:/dbloom-data
    secrets: [dbloom_secret]
    ports: ["8081:8081"]
    healthcheck: { test: ["CMD","curl","-f","http://localhost:8081/api/v1/health"], interval: 15s }
  dbloom-worker:
    image: dbloom
    environment: { DBLOOM_ROLE: worker }
    volumes:
      - dbloom-data:/dbloom-data
    deploy: { replicas: 1 }            # 可 scale=N
volumes:
  dbloom-data:
    # 单机：本地命名卷
    # 多机集群：NFS 卷驱动，所有节点挂同一共享盘（D10）——只放 日志/文件/jar/checkpoint
    # driver_opts:
    #   type: nfs
    #   o: "addr=192.168.1.10,nfsvers=4"
    #   device: ":/exports/dbloom-data"
  dbloom-db:
    # 元数据库数据卷（如连外部托管 DB 则无需）
```
- 对外只暴露 `dbloom-master:8081`（HTTP，TLS 由外部网关，D15）；引擎 8080 不对外；`dbloom-db` 不暴露到公网。
- 多机 compose：`dbloom-data` 对 master/worker 各节点同一份（NFS 卷驱动）；**元数据库**单机卷或外部实例均可。

### 2.3 k8s（生产）
基于 SeaTunnel 官方 Helm chart 改造（镜像换 dbloom、无 reload、无独立 web UI ingress）：
- **一个 RWX PVC `dbloom-data`** 挂到 master/worker 全部 Pod（路径 `/dbloom-data`）——只放 日志/文件/custom-jar/checkpoint。
- **元数据库**两种选择：连外部托管 MySQL/PG（推荐，`DB_DSN` 放 Secret）；或 chart 内可选 `db` StatefulSet（MySQL，自带 PVC）。
- `dbloom-master` Deployment（replicas=1，可多副本）：entrypoint 起引擎 master + dbloom-server；Service + Ingress 暴露 8081（HTTPS 由 Ingress 证书终止，D15）。
- `dbloom-worker` Deployment（replicas=可调 / 配 HPA by CPU）：entrypoint 起引擎 worker。
- ConfigMap：集群配置（cluster-name 一致、引擎参、checkpoint 路径=/dbloom-data/checkpoint、history-job-expire-minutes）。
- Secret：`dbloom-secret`（`DBLOOM_SECRET_KEY_FILE` + `DBLOOM_JWT_SECRET` + `DB_DSN`，安装时随机生成）。
- RBAC：只读访问 k8s API（若轮询 Pod/节点用于 worker 状态可视化，可选）。

```text
helm install dbloom ./deploy/kubernetes/dbloom \
  --set dbloomData.storageClass=managed-nfs-storage \
  --set db.mode=external --set db.dsn=mysql://... \    # 或 db.mode=bundled
  --set master.replicas=1 \
  --set worker.replicas=3
```

---

## 3. 多副本与故障转移（D10 v1.1 落地）

- **dbloom-server 无状态、可多副本**：写操作并发由外部数据库事务/行锁保证；调度防重用 `scheduler_jobs.lock_until` 行锁 CAS（`04-security.md` §5）。
- **master（引擎）故障**：Hazelcast 自动重选（引擎原生能力）；dbloom-server 任意副本可接管对外服务。
- **元数据库故障**：由数据库层高可用承担（托管实例/主从自动切换）；迁移用 DB 级锁防并发。
- 备份：**元数据库**用其原生备份（mysqldump/pg_dump 或云快照）+ **共享卷**快照（checkpoint + 文件 + jar 一体），k8s 用存储快照 / Velero；单机 docker 用 `docker run --rm -v dbloom-data:/dbloom-data ... tar` 存档。

---

## 4. 环境变量清单

| 变量 | 必填 | 说明 | 决策 |
| --- | --- | --- | --- |
| `DBLOOM_ROLE` | 是 | `master` \| `worker` | D1/D11 |
| `DB_DSN` | 是（master） | 元数据库连接串 `mysql://...` 或 `postgres://...`（D10 v1.1） | D10 |
| `DBLOOM_SECRET_KEY_FILE` | 是（生产） | 主密钥文件路径 | D 数据保护 |
| `DBLOOM_JWT_SECRET` | 是（生产） | JWT 签名密钥 | D19 |
| `DBLOOM_DATA_ROOT` | 否 | 共享挂载根（默认 `/dbloom-data`） | D10 |
| `DBLOOM_HTTP_PORT` | 否 | dbloom-server 对外 HTTP 端口（默认 8081） | — |
| `SEATUNNEL_HTTP_PORT` | 否 | 引擎 REST（默认 8080，仅本地） | — |
| `RUST_LOG` | 否 | 日志级别 | D22 |

---

## 5. 网络与可达性要求（统一转发 D17 的前提）

- **dbloom-server / 引擎 worker 必须能访问所有用户配置的数据库地址**（含集群内部 service 名）——部署文档要着重声明：
  - 集群内：同命名空间/跨命名空间用 FQDN，需在 K8s 网络策略放行。
  - 混合网络：数据库在 VPC/专线网段时，worker/master 所在节点需加入该网段。
- 浏览器只需可达 dbloom-server 的 8081（或外部网关）——对数据库服务零可达要求（这正是 D17 价值）。
- 引擎 master/worker 之间：Hazelcast 集群组网（5801/5701 等端口依官方配置），同一 K8s service / 同一 compose 网络即默认互通。

---

## 6. 可观测性（最小集）
- 健康：`/api/v1/health`（HTTP + 元数据库连通 + 共享卷可写 + 引擎 8080）。
- 指标：dbloom-server 暴露 Prometheus 风格 `/metrics`（HTTP 计数/函数延迟/连接池 in-use），供 k8s HPA/告警用（不引入外部存储，仅时序窗 sha 可选；或由外部 Prometheus 抓取）。
- 日志：应用日志 → stdout（容器收集）+ 共享卷 `logs/app/`（14 天滚动）；任务日志 → 共享卷 `logs/tasks/`。
