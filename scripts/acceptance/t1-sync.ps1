# ============================================================================
# T8 专项：同步成功验收（mysql -> mysql 全量，真实 FINISHED）
# 关键证据：任务提交返回 runId(=引擎 jobId)；轮询到 succeeded(dbloom <=> 引擎 FINISHED)；
#   目标表行数 / 抽样值 / 中文 HEX 与源一致；run 日志无 API-06。
# 落档：fail==0 写 scripts/acceptance-results/t1-sync-final.txt（完整输出块）
# 前置：dbloom-server(8081) + SeaTunnel 引擎(8080) + dbloom-mysql 容器(127.0.0.1:3306)
# ============================================================================
$ErrorActionPreference='Continue'
# 【T8】完整输出落档
$script:transcript = Join-Path $env:TEMP "t1-sync-$(Get-Date -Format yyyyMMddHHmmss).log"
Start-Transcript -Path $script:transcript -Force | Out-Null

$base='http://127.0.0.1:8081/api/v1'
$engineBase='http://127.0.0.1:8080'
$muser='root'; $mpass='dbloom_root_2026'
$pass=0; $fail=0
function Assert([string]$name,[bool]$cond,[string]$detail){
  if($cond){$script:pass++;Write-Host "  [PASS] $name"}else{$script:fail++;Write-Host "  [FAIL] $name :: $detail"}
}
function Invoke-Api([string]$method,[string]$path,$body,[string]$token){
  $h=@{}; if($token){$h['Authorization']="Bearer $token"}
  $p=@{Method=$method;Uri="$base$path";Headers=$h;TimeoutSec=30}
  if($null -ne $body){$p['ContentType']='application/json';$p['Body']=($body|ConvertTo-Json -Depth 10 -Compress)}
  try{ $r=Invoke-WebRequest @p -UseBasicParsing
       $obj=$null;if($r.Content){$obj=$r.Content|ConvertFrom-Json}
       return @{ok=$true;status=[int]$r.StatusCode;data=$obj;raw=$r.Content}
  }catch{
       $st=0;$txt=''
       if($_.Exception.Response){$st=[int]$_.Exception.Response.StatusCode
          try{$sr=New-Object IO.StreamReader($_.Exception.Response.GetResponseStream());$txt=$sr.ReadToEnd()}catch{}}
       return @{ok=$false;status=$st;body=$txt;msg=$_.Exception.Message}
  }
}
function Exec-MySql([string]$db,[string]$sql){
  $f = Join-Path $env:TEMP ("sql-"+[guid]::NewGuid().ToString('N')+".sql")
  [IO.File]::WriteAllText($f,$sql,(New-Object Text.UTF8Encoding($false)))
  cmd /c "wsl docker exec -i dbloom-mysql mysql --default-character-set=utf8mb4 -uroot -pdbloom_root_2026 $db < `"$f`"" 2>$null | Out-Null
  Remove-Item $f -ErrorAction SilentlyContinue
}
function Exec-MySqlQ([string]$db,[string]$sql){
  $f = Join-Path $env:TEMP ("sqlq-"+[guid]::NewGuid().ToString('N')+".sql")
  [IO.File]::WriteAllText($f,$sql,(New-Object Text.UTF8Encoding($false)))
  $out = (cmd /c "wsl docker exec -i dbloom-mysql mysql -N --default-character-set=utf8mb4 -uroot -pdbloom_root_2026 $db < `"$f`"" 2>$null | Out-String).Trim()
  Remove-Item $f -ErrorAction SilentlyContinue
  return $out
}

Write-Host "=== T1-SYNC 专项（同步成功）验收开始 $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ==="

# ---------- 0) health：引擎真实在线 ----------
$h = Invoke-RestMethod -Uri "$base/health" -TimeoutSec 5
Assert 'health code=0 且 engine=true（引擎真实在线）' ($h.code -eq 0 -and $h.data.engine) ($h | ConvertTo-Json -Compress)

# ---------- 1) 登录 admin ----------
$login = Invoke-Api 'POST' '/auth/login' @{username='admin';password='admin123456'} $null
Assert 'admin 登录成功' ($login.ok -and $login.data.data.accessToken) "$($login.status) $($login.body)"
$token = $login.data.data.accessToken

# ---------- 2) 创建源/目标连接（mysql -> mysql，参数来自已加密连接配置） ----------
$src = Invoke-Api 'POST' '/connections' @{name='t8_sync_src';connType='mysql';host='127.0.0.1';port=3306;databaseName='dbx_src';username=$muser;password=$mpass} $token
$dst = Invoke-Api 'POST' '/connections' @{name='t8_sync_dst';connType='mysql';host='127.0.0.1';port=3306;databaseName='dbx_dst';username=$muser;password=$mpass} $token
Assert '创建源连接' ($src.ok -and $src.data.data.id) $src.body
Assert '创建目标连接' ($dst.ok -and $dst.data.data.id) $dst.body
$srcId=$src.data.data.id; $dstId=$dst.data.data.id
Write-Host "  [INFO] 源连接  id=$srcId  mysql://root@127.0.0.1:3306/dbx_src"
Write-Host "  [INFO] 目标连接 id=$dstId  mysql://root@127.0.0.1:3306/dbx_dst"

# ---------- 3) 源表 6 行（含中文"北京"，hex 插入）+ 目标表清空 ----------
Exec-MySql 'dbx_src' "DROP TABLE IF EXISTS t_sync_src; CREATE TABLE t_sync_src(id INT PRIMARY KEY, name VARCHAR(64), score DOUBLE); INSERT INTO t_sync_src VALUES (1,'alpha',1.5),(2,'beta',2.5),(3,'gamma',3.5),(4,'delta',4.5),(5,'epsilon',5.5),(6,CONVERT(0xE58C97E4BAAC USING utf8mb4),6.5);"
Exec-MySql 'dbx_dst' "DROP TABLE IF EXISTS t_sync_dst; CREATE TABLE t_sync_dst(id INT PRIMARY KEY, name VARCHAR(64), score DOUBLE);"
$srcRows = Exec-MySqlQ 'dbx_src' "SELECT COUNT(*) FROM t_sync_src;"
$dstRows0 = Exec-MySqlQ 'dbx_dst' "SELECT COUNT(*) FROM t_sync_dst;"
Assert '源表就绪且行数>=2' ([int]$srcRows -ge 2) "rows=$srcRows"
Assert '目标表初始为空' ([int]$dstRows0 -eq 0) "rows=$dstRows0"
Write-Host "  [INFO] 源表 t_sync_src 行数=$srcRows；目标表 t_sync_dst 初始行数=$dstRows0"

# ---------- 4) 创建同步任务（full batch） ----------
$task = Invoke-Api 'POST' '/tasks' @{
  name='t8-sync-ok'; sourceConnectionId=$srcId; sinkConnectionId=$dstId
  syncMode='batch'; tableMapping=@(@{sourceTable='t_sync_src';sinkTable='t_sync_dst'})
  enabled=$true; timeoutSec=120; retryTimes=1 } $token
Assert '创建同步任务（返回 taskId）' ($task.ok -and $task.data.data.id) $task.body
$taskId = $task.data.data.id
Write-Host "  [INFO] taskId=$taskId  源表 t_sync_src -> 目标表 t_sync_dst（库名自动补前缀）"

# ---------- 5) HOCON 快照可回看 + 无明文密码 ----------
$tDetail = Invoke-Api 'GET' "/tasks/$taskId" $null $token
$hocon = $tDetail.data.data.configHocon
Assert 'HOCON 快照可回看（任务详情返回 configHocon）' (-not [string]::IsNullOrWhiteSpace($hocon)) ($hocon.Length)
Assert 'HOCON 不含明文密码' (-not ($hocon -match [regex]::Escape($mpass)))
Write-Host "  [INFO] HOCON 快照（$($hocon.Length) 字符）首 200：$($hocon.Substring(0,[Math]::Min(200,$hocon.Length)).Replace("`n",' '))"

# ---------- 6) 提交任务 -> runId(=引擎 jobId) ----------
$trig = Invoke-Api 'POST' "/tasks/$taskId/trigger" @{idempotencyKey="t8-sync-$(Get-Date -Format yyyyMMddHHmmssfff)"} $token
Assert '任务提交返回 runId（=引擎 jobId）' ($trig.ok -and $trig.data.data.runId) "$($trig.status) $($trig.body)"
$runId = $trig.data.data.runId
Write-Host "  [INFO] submit 返回 runId/jobId = $runId"

# ---------- 7) 轮询到终态；打印 run 原始 JSON（含 jobStatus 映射与 jobId 字段） ----------
$final=$null; $rdRaw=''
foreach($i in 1..60){
  Start-Sleep 3
  $rd = Invoke-Api 'GET' "/runs/$runId" $null $token
  if($rd.ok -and $null -ne $rd.data.data){
    $rdRaw=$rd.raw
    if($rd.data.data.status -in @('succeeded','failed','canceled')){ $final=$rd.data.data; break }
  } else { $rdRaw=$rd.body }
}
Assert 'run 到达终态（轮询成功）' ($null -ne $final) "$($rd.body)"
if($final){
  Write-Host "  [INFO] run 原始 JSON: $($rdRaw)"
  Assert "run status = succeeded（实际 $($final.status)）" ($final.status -eq 'succeeded') $final.status
}

# 引擎侧佐证：job-info 拿到 FINISHED（用引擎真实 jobId = seaTunnelJobId，非 dbloom 自增 runId）
$stJobId = $null
if($final){ $stJobId = $final.seaTunnelJobId }
$ji=$null
if($stJobId){ try{ $ji = Invoke-RestMethod -Uri "$engineBase/job-info/$stJobId" -TimeoutSec 5 }catch{} }
if($ji){
  Write-Host "  [INFO] 引擎 job-info: jobId=$stJobId jobStatus=$($ji.jobStatus) errorMsg=$($ji.errorMsg)"
  Assert "引擎 jobStatus = FINISHED（实际 $($ji.jobStatus)）" ($ji.jobStatus -eq 'FINISHED') "$($ji.jobStatus) $($ji.errorMsg)"
} else {
  Write-Host "  [WARN] 引擎 job-info($stJobId) 不可达——以 dbloom run status=succeeded(<=>引擎 FINISHED) 为准"
}

# ---------- 8) 数据一致性：行数 + SUM + 中文HEX 校验串 + 抽样 ----------
$ck_src = Exec-MySqlQ 'dbx_src' "SELECT CONCAT(SUM(id),'|',ROUND(SUM(score),1),'|',HEX(MIN(name)),'|',HEX(MAX(name)),'|',COUNT(*)) FROM t_sync_src;"
$ck_dst = Exec-MySqlQ 'dbx_dst' "SELECT CONCAT(SUM(id),'|',ROUND(SUM(score),1),'|',HEX(MIN(name)),'|',HEX(MAX(name)),'|',COUNT(*)) FROM t_sync_dst;"
Write-Host "  [INFO] 源一致性串  : $ck_src"
Write-Host "  [INFO] 目标一致性串: $ck_dst"
Assert '数据一致（行数/SUM/中文HEX/COUNT 全等）' ("$ck_src" -eq "$ck_dst") "src=[$ck_src] dst=[$ck_dst]"
$samp_d = Exec-MySqlQ 'dbx_dst' "SELECT GROUP_CONCAT(CONCAT(id,':',name,':',score) ORDER BY id) FROM t_sync_dst;"
Write-Host "  [INFO] 目标表抽样（id:name:score，中文以 HEX 证明一致）= $samp_d"

# ---------- 9) 无 API-06 / Factory initialize failed ----------
$logs = Invoke-Api 'GET' "/runs/$runId/logs" $null $token
$lt = "$($logs.data.data.log)"
$hasErr = ($lt -match 'API-06') -or ($lt -match 'Factory initialize failed')
Assert 'run 日志无 API-06 / Factory initialize failed' (-not $hasErr) $lt.Substring(0,[Math]::Min(200,$lt.Length))

Write-Host "`n=== T1-SYNC 汇总：PASS=$pass FAIL=$fail ==="
$suffix="$(Get-Date -Format 'yyyyMMdd-HHmmss')"
# 【T8】完整输出落档
Stop-Transcript | Out-Null -ErrorAction SilentlyContinue
$full=Get-Content $script:transcript -Raw -ErrorAction SilentlyContinue
$out="=== T1-SYNC 完整输出（$suffix）===`nPASS=$pass FAIL=$fail  exit_code=$fail`n`n$full`n"
if($fail -eq 0){
  if(-not (Test-Path 'scripts/acceptance-results')){New-Item -ItemType Directory -Force -Path 'scripts/acceptance-results'|Out-Null}
  Set-Content -Path 'scripts/acceptance-results/t1-sync-final.txt' -Value $out -Encoding UTF8
  Write-Host ("最终 PASS 完整输出已留档 -> scripts/acceptance-results/t1-sync-final.txt ({0} 字符)" -f $out.Length)
}
exit $fail
