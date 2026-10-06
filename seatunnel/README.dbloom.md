# seatunnel/ — SeaTunnel 引擎源码（复制进仓）

> ⭐ 本项目源码复用原则（最高优先级）：
> seatunnel / dbx 源码是经过无数场景验证的成熟代码，dbloom 是**融合**它们，**不是重写**。
> 本目录源码**复制自 `G:\work\seatunnel`（用户定制版）**，用不到就删、不合适就改源码。

## 来源与版本
- **来源路径**：`G:\work\seatunnel`（本机工作区，用户在此之上做了大量功能改动和增强，与官方源码有较大差异）
- **版本**：SeaTunnel **2.3.12**（`pom.xml`）
- **复制时间**：2026-10-06
- **复制方式**：`robocopy G:\work\seatunnel seatunnel /E`（排除 `.git` / `target` / IDE 杂项）
- ⚠️ **注意**：**不要**用官方发布包 / 重新 clone 覆盖此目录——用户定制改动只在 `G:\work\seatunnel`，未来同步定制改动需从该路径增量复制。

## 目录结构（简述）
| 模块 | 用途 | D 决策 |
|---|---|---|
| `seatunnel-engine/` | Zeta 引擎（master/worker/client/core/server/…） | **保留为主**，master+worker 模式核心 |
| `seatunnel-connectors-v2/` | 各类连接器（jdbc/kafka/…按需保留） | 按 D24a 裁剪，只留用到的 |
| `seatunnel-core/` | starter（`seatunnel-starter` 提供 master/worker 启动） | 保留 `seatunnel-starter`，其余裁 |
| `seatunnel-api/common/config/plugin-discovery/…` | 引擎支撑库 | 保留为依赖 |
| `seatunnel-transforms-v2/` `seatunnel-formats/` | 转换/格式 | 按需（M3 构建后决定） |
| `seatunnel-e2e/` `seatunnel-examples/` | 测试/示例 | **可删**（无用代码就删） |
| `seatunnel-ci-tools/` | CI 工具 | **可删** |
| `docs/` `release-note.md` | 官方文档/发布记录 | **可删**（保留 LICENSE/NOTICE 即可） |

## 构建
- 需要 **JDK 8（引擎）** + **Maven 3.6+**（工程自带 `mvnw.cmd`）。
- 常用命令（在 `seatunnel/` 下执行）：
  - 全量构建：`mvnw.cmd -T 2C clean package -DskipTests -Dcheckstyle.skip=true -Dspotless.check.skip=true`
  - 仅引擎发行：`mvnw.cmd clean package -DskipTests -pl seatunnel-dist -am`
  - 产出在 `seatunnel-dist/target/`（seataunnel-2.3.12/ 包含 bin/config/connectors）。
- 本机当前**未装 Maven**：构建需先装 Maven（或直接用随 dist 提供的 jar）。

## dbloom 接入方式（M3，见 docs/design/03-modules.md §5）
- `dbloom-sync` 生成 HOCON → 调用引擎 REST API（8800）submit job / 轮询状态；`-r master` 内置 master 守护、worker 由 `-r worker` 拉起。
- 编排后 engine 进程由 dbloom 控制面管理（D57「引擎也算一个 worker 角色」），不再用 seatunnel reload。
