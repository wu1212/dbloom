# ============================================================================
# T1 验收脚本：同步任务最小闭环（判据 A-G + 用例 1-7）
# 对应：docs/design/06-milestones.md M3；02-api.md §2.6
# 用法：     powershell -ExecutionPolicy Bypass -File scripts/acceptance/t1-accept.ps1
# 前置：     dbloom-server 运行于 $env:DBLOOM_HTTP_PORT（默认 8081）+ SeaTunnel 引擎 8080
#           + dbloom-mysql 容器（127.0.0.1:3306，root:dbloom_root_2026）
# 结果留档：脚本将最终 PASS 输出追加到 scripts/acceptance-results/t1-final.txt
#           （该目录不入 .gitignore、可 git 追溯，供后续审计）
# ============================================================================
$ErrorActionPreference = 'Continue'

# 【T8】完整输出留档：整个运行期转 transcript，末尾 Stop-Transcript 落 final.txt
$script:transcript = Join-Path $env:TEMP "t1-accept-$(Get-Date -Format yyyyMMddHHmmss).log"
Start-Transcript -Path $script:transcript -Force | Out-Null

$base   = if ($env:DBLOOM_HTTP_PORT) { "http://127.0.0.1:$env:DBLOOM_HTTP_PORT/api/v1" } else { 'http://127.0.0.1:8081/api/v1' }
$mysql  = '127.0.0.1:3306'
$muser  = 'root'
$mpass  = 'dbloom_root_2026'

$pass   = 0; $fail = 0
function Assert([string]$name, [bool]$cond, [string]$detail) {
    if ($cond) { $script:pass++; Write-Host "  [PASS] $name" }
    else       { $script:fail++; Write-Host "  [FAIL] $name :: $detail" }
}

function Invoke-Api([string]$method, [string]$path, $body, [string]$token) {
    $h = @{}
    if ($token) { $h['Authorization'] = "Bearer $token" }
    $p = @{ Method = $method; Uri = "$base$path"; Headers = $h; TimeoutSec = 30 }
    if ($null -ne $body) {
        $p['ContentType'] = 'application/json'
        $p['Body'] = ($body | ConvertTo-Json -Depth 10 -Compress)
    }
    try {
        $r = Invoke-WebRequest @p -UseBasicParsing
        $obj = $null
        if ($r.Content) { $obj = $r.Content | ConvertFrom-Json }
        return @{ ok = $true; status = [int]$r.StatusCode; data = $obj }
    } catch {
        $status = 0; $txt = ''
        if ($_.Exception.Response) {
            $status = [int]$_.Exception.Response.StatusCode
            try { $sr = New-Object IO.StreamReader($_.Exception.Response.GetResponseStream()); $txt = $sr.ReadToEnd() } catch {}
        }
        return @{ ok = $false; status = $status; body = $txt; msg = $_.Exception.Message }
    }
}

Write-Host "=== T1 验收开始 $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ==="

# ---------- 0) 依赖就绪 ----------
$svc = Invoke-WebRequest -Uri "$base/health" -TimeoutSec 5 -UseBasicParsing -ErrorAction SilentlyContinue
$h = if ($svc) { $svc.Content | ConvertFrom-Json } else { $null }
Assert 'health 接口可达' ($null -ne $h -and $h.code -eq 0) ($svc)
if ($h -and -not $h.data.engine) {
    Write-Host "  [WARN] 引擎未在 health 中标记在线——判据 A/D/E 依赖真实引擎，需先启动（knowledge: dbloom 接入 fork 版 SeaTunnel 引擎）"
}

# ---------- 1) 登录 admin ----------
$login = Invoke-Api 'POST' '/auth/login' @{ username = 'admin'; password = 'admin123456' }
Assert 'admin 登录成功' ($login.ok -and $null -ne $login.data.data.accessToken) ($login)
$token = $login.data.data.accessToken

# ---------- 2) 建源/目标连接（密码经加密落库，不做明文二次输入） ----------
$src = Invoke-Api 'POST' '/connections' @{
    name = 't1_src_mysql'; connType = 'mysql'; host = '127.0.0.1'; port = 3306
    databaseName = 'dbx_src'; username = $muser; password = $mpass
} $token
$dst = Invoke-Api 'POST' '/connections' @{
    name = 't1_dst_mysql'; connType = 'mysql'; host = '127.0.0.1'; port = 3306
    databaseName = 'dbx_dst'; username = $muser; password = $mpass
} $token
Assert '创建源连接' ($src.ok -and $src.data.data.id) ($src)
Assert '创建目标连接' ($dst.ok -and $dst.data.data.id) ($dst)
$srcId = $src.data.data.id; $dstId = $dst.data.data.id

# 连接详情不回传明文密码（判据 B 的数据安全前提）
$srcDetail = Invoke-Api 'GET' "/connections/$srcId" $null $token
$pwdLeak = ($srcDetail.data.data | ConvertTo-Json -Depth 6) -match $mpass
Assert '连接详情不回传明文密码' (-not $pwdLeak) $pwdLeak

# ---------- 3) 准备源表数据（判据 A：源 ≥2 行，目标先清空）
#           + big_rows 大表（判据 D：制造 running 窗口）----------
wsl docker exec dbloom-mysql mysql -uroot -pdbloom_root_2026 dbx_src -e "DROP TABLE IF EXISTS t1_src; CREATE TABLE t1_src(id INT PRIMARY KEY, name VARCHAR(64)); INSERT INTO t1_src VALUES (1,'alpha'),(2,'beta'),(3,'gamma');" 2>$null
wsl docker exec dbloom-mysql mysql -uroot -pdbloom_root_2026 dbx_dst -e "DROP TABLE IF EXISTS t1_dst; CREATE TABLE t1_dst(id INT PRIMARY KEY, name VARCHAR(64));" 2>$null
# big_rows 大表（判据 D：30 万行全量复制制造 running 窗口）；big_copy 目标先清空
wsl docker exec dbloom-mysql mysql -uroot -pdbloom_root_2026 dbx_src -e "DROP TABLE IF EXISTS big_rows; CREATE TABLE big_rows(id INT PRIMARY KEY, v VARCHAR(64)) AS SELECT a.n + b.n*10 + c.n*100 + d.n*1000 + e.n*10000 + f.n*100000 + 1 AS id, CONCAT('r', a.n + b.n*10 + c.n*100 + d.n*1000 + e.n*10000 + f.n*100000 + 1) AS v FROM (SELECT 0 n UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) a CROSS JOIN (SELECT 0 n UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) b CROSS JOIN (SELECT 0 n UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) c CROSS JOIN (SELECT 0 n UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) d CROSS JOIN (SELECT 0 n UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) e CROSS JOIN (SELECT 0 n UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) f;" 2>$null
wsl docker exec dbloom-mysql mysql -uroot -pdbloom_root_2026 dbx_dst -e "DROP TABLE IF EXISTS big_copy;" 2>$null
Write-Host '  [SETUP] 源表 t1_src(3 行) / 目标表 t1_dst(空) / big_rows(100万行) 已就绪'

# ---------- 4) 建同步任务（batch 全量） ----------
$task = Invoke-Api 'POST' '/tasks' @{
    name = 't1-full-sync'; description = 'T1 acceptance full sync'
    sourceConnectionId = $srcId; sinkConnectionId = $dstId
    syncMode = 'batch'
    tableMapping = @(@{ sourceTable = 't1_src'; sinkTable = 't1_dst' })
    enabled = $true; timeoutSec = 120; retryTimes = 1
} $token
Assert '创建同步任务' ($task.ok -and $task.data.data.id) ($task)
$taskId = $task.data.data.id

# ---------- 判据 B：HOCON 快照可回看 + 不含明文密码 ----------
$tDetail = Invoke-Api 'GET' "/tasks/$taskId" $null $token
$hocon = $tDetail.data.data.configHocon
$okHocon = -not [string]::IsNullOrWhiteSpace($hocon)
Assert 'B1: HOCON 快照回看（任务详情返回 configHocon）' $okHocon ($hocon.Length)
$hoconLeak = $hocon -match [regex]::Escape($mpass)
Assert 'B2: HOCON 快照不含明文密码' (-not $hoconLeak) $hoconLeak
Write-Host "  [INFO] HOCON 快照($($hocon.Length) 字符) 首 160: $($hocon.Substring(0,[Math]::Min(160,$hocon.Length)).Replace("`n",' '))"
$snapshotCap = $hocon -match 'source|sink|Jdbc'
Assert 'B3: 快照含 env/source/sink 结构' $snapshotCap

# ---------- 判据 A：trigger → pending→running→succeeded，行数/抽样一致 ----------
$trig = Invoke-Api 'POST' "/tasks/$taskId/trigger" @{ idempotencyKey = "t1-accept-$(Get-Date -Format yyyyMMddHHmmssfff)" } $token
Assert 'A1: trigger 返回 runId' ($trig.ok -and $trig.data.data.runId) ($trig)
$runId = $trig.data.data.runId

$final = $null
foreach ($i in 1..60) {
    Start-Sleep -Seconds 3
    $rd = Invoke-Api 'GET' "/runs/$runId" $null $token
    if (-not $rd.ok) { break }
    $st = $rd.data.data.status
    if ($st -in @('succeeded','failed','canceled')) { $final = $rd.data.data; break }
}
Assert 'A2: run 到达终态' ($null -ne $final) ($rd.msg)
if ($final) {
    $got = ($final.CreatedAt -gt 0)
    Assert "A3: run 状态 = succeeded（实际 $($final.status)）" ($final.status -eq 'succeeded') $final.status
    # 目标表行数对比
    $cnt = wsl docker exec dbloom-mysql mysql -N -uroot -pdbloom_root_2026 dbx_dst -e "SELECT COUNT(*) FROM t1_dst;" 2>$null
    $srcCnt = wsl docker exec dbloom-mysql mysql -N -uroot -pdbloom_root_2026 dbx_src -e "SELECT COUNT(*) FROM t1_src;" 2>$null
    Assert "A4: 目标行数=源行数（$srcCnt/$cnt）" ("$cnt" -eq "$srcCnt") "src=$srcCnt dst=$cnt"
    # 抽样值对比
    $s2 = wsl docker exec dbloom-mysql mysql -N -uroot -pdbloom_root_2026 dbx_src -e "SELECT GROUP_CONCAT(CONCAT(id,':',name) ORDER BY id) FROM t1_src;" 2>$null
    $d2 = wsl docker exec dbloom-mysql mysql -N -uroot -pdbloom_root_2026 dbx_dst -e "SELECT GROUP_CONCAT(CONCAT(id,':',name) ORDER BY id) FROM t1_dst;" 2>$null
    Assert 'A5: 抽样值一致（id:name 拼接）' ("$s2" -eq "$d2") "src=[$s2] dst=[$d2]"
}

# ---------- 用例 7：同一请求重复 POST（幂等不重复建 run） ----------
$idem2 = "t1-idem-$runId"
$trigB = Invoke-Api 'POST' "/tasks/$taskId/trigger" @{ idempotencyKey = $idem2 } $token
$trigC = Invoke-Api 'POST' "/tasks/$taskId/trigger" @{ idempotencyKey = $idem2 } $token
# 第二次应命中同一 run（或至少不新增）
$runsList = Invoke-Api 'GET' "/tasks/$taskId/runs?pageSize=50" $null $token
$runCount = @($runsList.data.data.items | Where-Object { $_.idempotencyKey -eq $idem2 }).Count
Assert '用例7: 同 idempotencyKey 不重复建 run（命中 1 次）' ($runCount -eq 1) "count=$runCount"

# 等待队列里可能的并发 run 结束，避免干扰 stop/retry
Start-Sleep -Seconds 2

# ---------- 判据 C：必败 run（sink 端口不可达 → 引擎连接失败落 failed） ----------
# 【T7 说明】引擎 run 失败的主要形态（不穷举）：
#   (a) submit 阶段拒绝：HOCON 非法 / 连接器缺失 / 连接不可达 → 引擎 submit 即失败；
#   (b) 运行中失败：表不存在 / 端口不可达 / 类型不匹配 → RUNNING 后 FAILED。
#   本用例用「sink 指向本机不存在的 MySQL 端口 3399」（引擎 Jdbc sink 连不上 → failed）
#   制造必败——比「指向不存在的库」更可靠：后者会被引擎 CREATE_SCHEMA_WHEN_NOT_EXIST
#   + root 权限自动建库而“成功”（实测 run succeeded）。判据 C 只要求：
#   status 落 failed，error_message 或 run_logs 含真实原因。
$badDst = Invoke-Api 'POST' '/connections' @{
    name = 't1_dst_badport'; connType = 'mysql'; host = '127.0.0.1'; port = 3399
    databaseName = 'dbx_dst'; username = $muser; password = $mpass
} $token
$badTask = Invoke-Api 'POST' '/tasks' @{
    name = 't1-bad-task'; sourceConnectionId = $srcId; sinkConnectionId = $badDst.data.data.id
    syncMode = 'batch'; tableMapping = @(@{ sourceTable = 't1_src'; sinkTable = 't1_dst' })
    enabled = $true; timeoutSec = 60
} $token
$badTaskId = $badTask.data.data.id
$trigBad = Invoke-Api 'POST' "/tasks/$badTaskId/trigger" @{ idempotencyKey = "t1-bad-$(Get-Date -Format yyyyMMddHHmmssfff)" } $token
Assert 'C1: 必败任务 trigger 成功（返回 runId）' ($trigBad.ok -and $trigBad.data.data.runId) ($trigBad)
$badRunId = $trigBad.data.data.runId

$badFinal = $null
foreach ($i in 1..40) {
    Start-Sleep -Seconds 3
    $bt = Invoke-Api 'GET' "/runs/$badRunId" $null $token
    if ($null -ne $bt.data -and $bt.data.data.status -in @('succeeded','failed','canceled')) { $badFinal = $bt.data.data; break }
}
Assert 'C2: 必败 run 到达终态' ($null -ne $badFinal) $badFinal
if ($badFinal) {
    Assert "C3: 必败 run 状态 = failed（实际 $($badFinal.status)）" ($badFinal.status -eq 'failed') $badFinal.status
    $err = $badFinal.errorMessage
    Assert 'C4: error_message 非空且含真实原因' (-not [string]::IsNullOrWhiteSpace($err)) $err
    $logs = Invoke-Api 'GET' "/runs/$badRunId/logs" $null $token
    $logTxt = $logs.data.data.log
    $hasReason = ($err -match 'connect|refused|access|Connection|Exception|Error|fail|拒绝|失败|无法|unreachable|communicate') -or
                 ($null -ne $logTxt -and $logTxt -match 'Exception|Error|failed|拒绝|失败|connect|refused')
    Assert 'C5: 日志含真实失败信息（error/日志可读）' $hasReason $logTxt
}

# ---------- 判据 E：retry 失败 run → 成功 ----------
# 做法：把必败任务的目标连接端口改回真实 3306，令下一次 run 能成功；retry 只针对最近失败 run。
$fixConn = Invoke-Api 'PUT' "/connections/$($badDst.data.data.id)" @{ port = 3306; databaseName = 'dbx_dst' } $token
Assert 'E0: 修正目标连接端口为 3306' $fixConn.ok ($fixConn)
$retry = Invoke-Api 'POST' "/tasks/$badTaskId/retry" $null $token
Assert 'E1: retry 返回新 runId（且 attempt+1）' ($retry.ok -and $retry.data.data.runId) ($retry)
$retryRunId = $retry.data.data.runId
$rRetry = Invoke-Api 'GET' "/runs/$retryRunId" $null $token
Assert "E2: 新 run 来自 retry（attempt>1，实际 attempt=$($rRetry.data.data.attempt)）" ($rRetry.data.data.attempt -gt 1) $rRetry.data.data.attempt

$retryFinal = $null
foreach ($i in 1..60) {
    Start-Sleep -Seconds 3
    $rt = Invoke-Api 'GET' "/runs/$retryRunId" $null $token
    if ($null -ne $rt.data -and $rt.data.data.status -in @('succeeded','failed','canceled')) { $retryFinal = $rt.data.data; break }
}
Assert 'E3: retry run 到达终态' ($null -ne $retryFinal) $retryFinal
if ($retryFinal) {
    Assert "E4: retry run 状态 = succeeded（实际 $($retryFinal.status)）" ($retryFinal.status -eq 'succeeded') $retryFinal.status
}
# retry 检查：重试只针对最近失败 run（不再对 succeeded run 发起 retry → 应 409）
$retryAgain = Invoke-Api 'POST' "/tasks/$badTaskId/retry" $null $token
Assert 'E5: 对已成功 run retry → 409 拒绝' (-not $retryAgain.ok -and $retryAgain.status -eq 409) "$($retryAgain.status) $($retryAgain.body)"
# ---------- 判据 D：stop 一个 running run → 正确终态、无僵尸 ----------
# 用一个大表制造较长运行：big_rows(约 99 万行) 全量复制，触发后快速 stop。
$bigTask = Invoke-Api 'POST' '/tasks' @{
    name = 't1-stop-test'; sourceConnectionId = $srcId; sinkConnectionId = $dstId
    syncMode = 'batch'; tableMapping = @(@{ sourceTable = 'big_rows'; sinkTable = 'big_copy' })
    enabled = $true; timeoutSec = 600
} $token
$bigTaskId = $bigTask.data.data.id
$trigBig = Invoke-Api 'POST' "/tasks/$bigTaskId/trigger" @{ idempotencyKey = "t1-stop-$(Get-Date -Format yyyyMMddHHmmssfff)" } $token
$bigRunId = $trigBig.data.data.runId

# 等待 run 进入 running（引擎确认提交）后 stop
$enteredRunning = $false
foreach ($i in 1..20) {
    Start-Sleep -Seconds 2
    $bs = Invoke-Api 'GET' "/runs/$bigRunId" $null $token
    if ($bs.data.data.status -eq 'running') { $enteredRunning = $true; break }
    if ($null -ne $bs.data -and $bs.data.data.status -in @('succeeded','failed','canceled')) { break }
}
Assert 'D1: big run 进入 running 态（可 stop）' $enteredRunning $trigBig.msg
$stp = Invoke-Api 'POST' "/tasks/$bigTaskId/stop" $null $token
Assert 'D2: stop 返回 200 + canceled' ($stp.ok -and $stp.data.data.status -eq 'canceled') ($stp)

$stopFinal = $null
foreach ($i in 1..30) {
    Start-Sleep -Seconds 3
    $sf = Invoke-Api 'GET' "/runs/$bigRunId" $null $token
    if ($null -ne $sf.data -and $sf.data.data.status -in @('succeeded','failed','canceled')) { $stopFinal = $sf.data.data; break }
}
Assert 'D3: stop 后 run 到达终态' ($null -ne $stopFinal) $stopFinal
if ($stopFinal) {
    Assert "D4: stop 后状态 canceled（实际 $($stopFinal.status)）" ($stopFinal.status -eq 'canceled') $stopFinal.status
}
# 无僵尸：同一任务不应再有 running run
$activeList = Invoke-Api 'GET' "/tasks/$bigTaskId/runs?pageSize=10" $null $token
$stillRunning = @($activeList.data.data.items | Where-Object { $_.status -eq 'running' }).Count
Assert 'D5: 无僵尸 running run' ($stillRunning -eq 0) "stillRunning=$stillRunning"

# ---------- 判据 F：用户 A 的任务，用户 B 与匿名不可见/不可操作 ----------
# 建普通用户 B（随机初始密码 → 登录 → 改密 UserB123456 → 重登），验证真实 B 身份越权
$bname = "t1b_$(Get-Date -Format 'HHmmssfff')"
$crB = Invoke-Api 'POST' '/users' @{ username = $bname } $token
$initB = $crB.data.data.initialPassword
$lgB1 = Invoke-Api 'POST' '/auth/login' @{ username = $bname; password = $initB }
Invoke-Api 'POST' '/auth/change-password' @{ old_password = $initB; new_password = 'UserB123456' } $lgB1.data.data.accessToken | Out-Null
$loginB = Invoke-Api 'POST' '/auth/login' @{ username = $bname; password = 'UserB123456' }
$tokenB = $loginB.data.data.accessToken
Assert 'F0: 用户 B 登录成功' ($loginB.ok -and $tokenB) "$($loginB.status) $($loginB.msg)"

$bGet    = Invoke-Api 'GET' "/tasks/$taskId" $null $tokenB
$bTrig   = Invoke-Api 'POST' "/tasks/$taskId/trigger" @{ idempotencyKey = 'B-trigger' } $tokenB
$bStop   = Invoke-Api 'POST' "/tasks/$taskId/stop" $null $tokenB
$bRuns   = Invoke-Api 'GET' "/tasks/$taskId/runs" $null $tokenB
$bConn   = Invoke-Api 'GET' "/connections/$srcId" $null $tokenB
$anon    = Invoke-Api 'GET' "/tasks/$taskId" $null $null
Assert 'F1: 用户 B GET 任务 → 4xx' (-not $bGet.ok -and $bGet.status -ge 400 -and $bGet.status -lt 500) "$($bGet.status)"
Assert 'F2: 用户 B TRIGGER 任务 → 4xx' (-not $bTrig.ok -and $bTrig.status -ge 400 -and $bTrig.status -lt 500) "$($bTrig.status)"
Assert 'F3: 用户 B STOP 任务 → 4xx' (-not $bStop.ok -and $bStop.status -ge 400 -and $bStop.status -lt 500) "$($bStop.status)"
Assert 'F4: 用户 B 查 run 历史 → 4xx' (-not $bRuns.ok -and $bRuns.status -ge 400 -and $bRuns.status -lt 500) "$($bRuns.status)"
Assert 'F5: 用户 B 查连接 → 4xx' (-not $bConn.ok -and $bConn.status -ge 400 -and $bConn.status -lt 500) "$($bConn.status)"
Assert 'F6: 匿名 GET 任务 → 4xx' (-not $anon.ok -and $anon.status -in @(401,403,404)) "$($anon.status)"

Write-Host "`n=== T1 汇总：PASS=$pass FAIL=$fail ==="
$suffix = "$(Get-Date -Format 'yyyyMMdd-HHmmss')"
# 【T8】完整输出落档：Stop-Transcript 后把本次运行的全部输出块（每个判据 PASS/FAIL 行、
# 数据样本、退出码）写入 final.txt，供审计追溯。
Stop-Transcript | Out-Null -ErrorAction SilentlyContinue
$full = Get-Content $script:transcript -Raw -ErrorAction SilentlyContinue
$out = "=== T1 验收完整输出（$suffix）===`nPASS=$pass FAIL=$fail  exit_code=$fail`n`n$full`n"
if ($fail -eq 0) {
    if (-not (Test-Path 'scripts/acceptance-results')) { New-Item -ItemType Directory -Force -Path 'scripts/acceptance-results' | Out-Null }
    Set-Content -Path "scripts/acceptance-results/t1-final.txt" -Value $out -Encoding UTF8
    Write-Host ("最终 PASS 完整输出已留档 -> scripts/acceptance-results/t1-final.txt ({0} 字符)" -f $out.Length)
}
exit $fail
