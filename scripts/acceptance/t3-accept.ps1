# ============================================================================
# T3 验收脚本：租户隔离与 API Key 生命周期穿透测试（D8/D9/D21 + M0 存量）
# 前置：dbloom-server 运行于 8081（admin/admin123456）
# 说明：用户/连接/任务的 token = 身份 token；签发/撤销/更新 API Key 与查审计
#       必须用 admin token（D9/D21 admin 域）。key 调用方身份 = key 明文本身。
# 留档：fail==0 写 scripts/acceptance-results/t3-final.txt
# ============================================================================
$ErrorActionPreference='Continue'
# 【T8】完整输出落档
$script:transcript = Join-Path $env:TEMP "t3-accept-$(Get-Date -Format yyyyMMddHHmmss).log"
Start-Transcript -Path $script:transcript -Force | Out-Null
$base='http://127.0.0.1:8081/api/v1'
$pass=0;$fail=0
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
function New-LoginUser([string]$u,[string]$desired,[string]$adminTok){
  # admin 创建普通用户（随机初始密码）→ 初始密码登录 → change-password(改已知)
  # → 重新登录。返回 @{ok;id;token;err}
  $cr=Invoke-Api 'POST' '/users' @{username=$u} $adminTok
  if(-not $cr.ok){return @{ok=$false;id=$null;token=$null;err=$cr.body}}
  $id=$cr.data.data.id
  $init=$cr.data.data.initialPassword
  $lg=Invoke-Api 'POST' '/auth/login' @{username=$u;password=$init}
  if(-not $lg.ok){return @{ok=$false;id=$id;token=$null;err="initlogin $($lg.status)"}}
  Invoke-Api 'POST' '/auth/change-password' @{old_password=$init;new_password=$desired} $lg.data.data.accessToken | Out-Null
  $lg2=Invoke-Api 'POST' '/auth/login' @{username=$u;password=$desired}
  @{ok=$lg2.ok;id=$id;token=$lg2.data.data.accessToken;err=$lg2.body}
}
$nowMs=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
Write-Host "=== T3 验收开始 $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ==="

# ---------- 0) 三账号（admin 恒定：$tAdmin；A/B 普通用户身份 token） ----------
$admin=Invoke-Api 'POST' '/auth/login' @{username='admin';password='admin123456'} $null
Assert 'admin 登录' ($admin.ok -and $admin.data.data.accessToken) $admin.msg
$tAdmin=$admin.data.data.accessToken
$stamp=Get-Date -Format 'HHmmssfff'
$a=New-LoginUser "t3a_$stamp" 'UserA123456' $tAdmin
$b=New-LoginUser "t3b_$stamp" 'UserB123456' $tAdmin
Assert '用户 A/B 建并登录' ($a.ok -and $b.ok) "A=$($a.err) B=$($b.err)"
$uAid=$a.id; $uBid=$b.id
$tkA=$a.token; $tkB=$b.token
Assert 'A/B token 就绪' ($tkA -and $tkB) "A=$($null -ne $tkA) B=$($null -ne $tkB)"

# ---------- 1) A 建连接 + 任务 ----------
$connA=Invoke-Api 'POST' '/connections' @{name='t3a_conn';connType='mysql';host='127.0.0.1';port=3306;databaseName='dbx_src';username='root';password='dbloom_root_2026'} $tkA
Assert 'A 建连接' ($connA.ok -and $connA.data.data.id) ($connA)
$ca=$connA.data.data.id
$taskA=Invoke-Api 'POST' '/tasks' @{name='t3a_task';sourceConnectionId=$ca;sinkConnectionId=$ca;syncMode='batch';tableMapping=@(@{sourceTable='t1_src';sinkTable='t1_dst'});enabled=$false;timeoutSec=60} $tkA
Assert 'A 建任务' ($taskA.ok -and $taskA.data.data.id) ($taskA)
$ta=$taskA.data.data.id

# ---------- 判据 1：B 与匿名对 A 资源全部 4xx/空 ----------
$bList=Invoke-Api 'GET' '/connections' $null $tkB
$leakInBList=@($bList.data.data.items|Where-Object{$_.id -eq $ca}).Count -gt 0
Assert 'B 连接列表不含 A 连接' ($bList.ok -and -not $leakInBList) "leak=$leakInBList status=$($bList.status)"
$bGetConn=Invoke-Api 'GET' "/connections/$ca" $null $tkB
Assert 'B GET A 连接 -> 4xx' (-not $bGetConn.ok -and $bGetConn.status -ge 400 -and $bGetConn.status -lt 500) "$($bGetConn.status)"
$bTest=Invoke-Api 'POST' "/connections/$ca/test" @{} $tkB
Assert 'B test A 连接 -> 4xx' (-not $bTest.ok -and $bTest.status -ge 400 -and $bTest.status -lt 500) "$($bTest.status)"
$bQuery=Invoke-Api 'POST' '/query' @{connectionId=$ca;sql='SELECT 1'} $tkB
Assert 'B query A 连接 -> 4xx' (-not $bQuery.ok -and $bQuery.status -ge 400 -and $bQuery.status -lt 500) "$($bQuery.status)"
$bExp=Invoke-Api 'POST' '/export' @{sql='SELECT 1';connectionId=$ca;format='csv'} $tkB
Assert 'B export A 连接 -> 4xx' (-not $bExp.ok -and $bExp.status -ge 400 -and $bExp.status -lt 500) "$($bExp.status)"
$bGetTask=Invoke-Api 'GET' "/tasks/$ta" $null $tkB
Assert 'B GET A 任务 -> 4xx' (-not $bGetTask.ok -and $bGetTask.status -ge 400 -and $bGetTask.status -lt 500) "$($bGetTask.status)"
$bTrig=Invoke-Api 'POST' "/tasks/$ta/trigger" @{idempotencyKey="t3-B-$(Get-Date -Format HHmmssfff)"} $tkB
Assert 'B TRIGGER A 任务 -> 4xx' (-not $bTrig.ok -and $bTrig.status -ge 400 -and $bTrig.status -lt 500) "$($bTrig.status)"
$bPut=Invoke-Api 'PUT' "/tasks/$ta" @{description='hacked'} $tkB
Assert 'B PUT A 任务 -> 4xx' (-not $bPut.ok -and $bPut.status -ge 400 -and $bPut.status -lt 500) "$($bPut.status)"
$bDel=Invoke-Api 'DELETE' "/tasks/$ta" $null $tkB
Assert 'B DELETE A 任务 -> 4xx' (-not $bDel.ok -and $bDel.status -ge 400 -and $bDel.status -lt 500) "$($bDel.status)"
$anon=Invoke-Api 'GET' "/tasks/$ta" $null $null
Assert '匿名 GET A 任务 -> 401' (-not $anon.ok -and $anon.status -eq 401) "$($anon.status)"

# ---------- 判据 2：API Key 生命周期时序（签发/更新/撤销=admin；调用=key 明文） ----------
# 2a 签发即用
$k1=Invoke-Api 'POST' "/users/$uAid/api-keys" @{name='t3-key-now'} $tAdmin
Assert '签发 key(立即有效)' ($k1.ok -and $k1.data.data.secret) ($k1)
$k1secret=$k1.data.data.secret; $k1id=$k1.data.data.key.id
$useK1=Invoke-Api 'GET' "/connections/$ca" $null $k1secret
Assert '签发后立即可用（key GET A 连接 200）' ($useK1.ok -and $useK1.status -eq 200) "$($useK1.status) $($useK1.msg)"

# 2b valid_from=未来 -> 不可用
$kF=Invoke-Api 'POST' "/users/$uAid/api-keys" @{name='t3-key-future';validFrom=($nowMs+3600000)} $tAdmin
$kFsec=$kF.data.data.secret
$useF=Invoke-Api 'GET' "/connections/$ca" $null $kFsec
Assert 'valid_from=未来 -> 401/403' (-not $useF.ok -and $useF.status -in @(401,403)) "$($useF.status)"

# 2c valid_until=过去 -> 不可用
$kP=Invoke-Api 'POST' "/users/$uAid/api-keys" @{name='t3-key-past';validUntil=($nowMs-3600000)} $tAdmin
$kPsec=$kP.data.data.secret
$useP=Invoke-Api 'GET' "/connections/$ca" $null $kPsec
Assert 'valid_until=过去 -> 401/403' (-not $useP.ok -and $useP.status -in @(401,403)) "$($useP.status)"

# 2d 撤销 -> 立即失效（同一秒再调用）
$kRev=Invoke-Api 'POST' "/users/$uAid/api-keys" @{name='t3-key-revoke'} $tAdmin
$kRevsec=$kRev.data.data.secret
$okRev=Invoke-Api 'GET' "/connections/$ca" $null $kRevsec
Assert '撤销前可用' $okRev.ok "$($okRev.status)"
$rev=Invoke-Api 'DELETE' "/api-keys/$($kRev.data.data.key.id)" $null $tAdmin
Assert '撤销返回成功' ($rev.ok) "$($rev.status)"
$afterRev=Invoke-Api 'GET' "/connections/$ca" $null $kRevsec
Assert '撤销后立即失效 -> 401/403（同一秒）' (-not $afterRev.ok -and $afterRev.status -in @(401,403)) "$($afterRev.status)"

# 2e 停用 status=disabled -> 即时
$kDis=Invoke-Api 'POST' "/users/$uAid/api-keys" @{name='t3-key-disable'} $tAdmin
$kDissec=$kDis.data.data.secret
$okDis=Invoke-Api 'GET' "/connections/$ca" $null $kDissec
Assert '停用前可用' $okDis.ok "$($okDis.status)"
$upd=Invoke-Api 'PUT' "/api-keys/$($kDis.data.data.key.id)" @{status='disabled'} $tAdmin
Assert '停用更新成功' ($upd.ok) "$($upd.status)"
$afterDis=Invoke-Api 'GET' "/connections/$ca" $null $kDissec
Assert '停用后即时 401/403' (-not $afterDis.ok -and $afterDis.status -in @(401,403)) "$($afterDis.status)"

# ---------- 判据 3：审计必达 + admin only（查审计用 admin token） ----------
$auditLogin=Invoke-Api 'GET' '/audit-logs?action=login&pageSize=50' $null $tAdmin
Assert 'audit: login 可查' ($auditLogin.ok -and $auditLogin.data.data.total -gt 0) "$($auditLogin.status) total=$($auditLogin.data.data.total)"
$auditConn=Invoke-Api 'GET' '/audit-logs?action=conn_create&pageSize=50' $null $tAdmin
Assert 'audit: conn_create 可查' ($auditConn.ok -and $auditConn.data.data.total -gt 0) "$($auditConn.status) total=$($auditConn.data.data.total)"
$auditKey=Invoke-Api 'GET' '/audit-logs?action=apikey_issue&pageSize=50' $null $tAdmin
Assert 'audit: apikey_issue 可查' ($auditKey.ok -and $auditKey.data.data.total -gt 0) "$($auditKey.status) total=$($auditKey.data.data.total)"
$auditRev=Invoke-Api 'GET' '/audit-logs?action=apikey_revoke&pageSize=50' $null $tAdmin
Assert 'audit: apikey_revoke 可查' ($auditRev.ok -and $auditRev.data.data.total -gt 0) "$($auditRev.status) total=$($auditRev.data.data.total)"
$auditItems=Invoke-Api 'GET' '/audit-logs?pageSize=5' $null $tAdmin
$hasActor=@($auditItems.data.data.items|Where-Object{$_.actorUserId -gt 0}).Count -gt 0
$hasAction=@($auditItems.data.data.items|Where-Object{$_.action}).Count -gt 0
Assert 'audit: 条目含 actor/action 字段' ($auditItems.ok -and $hasActor -and $hasAction) "actor=$hasActor action=$hasAction"
$nonAdmin=Invoke-Api 'GET' '/audit-logs' $null $tkB
Assert '普通用户 B 查 audit-logs -> 403' (-not $nonAdmin.ok -and $nonAdmin.status -eq 403) "$($nonAdmin.status)"
$anonAudit=Invoke-Api 'GET' '/audit-logs' $null $null
Assert '匿名查 audit-logs -> 401' (-not $anonAudit.ok -and $anonAudit.status -eq 401) "$($anonAudit.status)"

Write-Host "`n=== T3 汇总：PASS=$pass FAIL=$fail ==="
$suffix="$(Get-Date -Format 'yyyyMMdd-HHmmss')"
# 【T8】完整输出落档
Stop-Transcript | Out-Null -ErrorAction SilentlyContinue
$full=Get-Content $script:transcript -Raw -ErrorAction SilentlyContinue
$out = "=== T3 验收完整输出（$suffix）===`nPASS=$pass FAIL=$fail  exit_code=$fail`n`n$full`n"
if($fail -eq 0){
  if(-not (Test-Path 'scripts/acceptance-results')){New-Item -ItemType Directory -Force -Path 'scripts/acceptance-results'|Out-Null}
  Set-Content -Path 'scripts/acceptance-results/t3-final.txt' -Value $out -Encoding UTF8
  Write-Host ("最终 PASS 完整输出已留档 -> scripts/acceptance-results/t3-final.txt ({0} 字符)" -f $out.Length)
}
exit $fail
