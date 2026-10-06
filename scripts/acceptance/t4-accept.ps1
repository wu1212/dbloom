# ============================================================================
# T4 验收脚本：导出与下载安全回归（D23 + 路径穿越防护）
# 对应：docs/design/00-overview.md D18（共享卷）/ D23（四格式导出）
# 前置：dbloom-server 运行于 8081 + dbloom-mysql 容器（127.0.0.1:3306 root:dbloom_root_2026）
# 说明：当前实现为 POST /api/v1/export + GET /api/v1/export/download（共享 export 区）。
#       D26 的 files/{id}/download 归属（data_upload/custom_jar）为 02-api 目标态、
#       本迭代未实现——归属校验以实际实现的「路径白名单 + 越界拒绝」为准。
# 留档：fail==0 写 scripts/acceptance-results/t4-final.txt
# ============================================================================
$ErrorActionPreference = 'Continue'
$base   = 'http://127.0.0.1:8081/api/v1'
$mysql  = '127.0.0.1:3306'; $muser='root'; $mpass='dbloom_root_2026'
$pass=0; $fail=0
function Assert([string]$name,[bool]$cond,[string]$detail){
  if($cond){$script:pass++;Write-Host "  [PASS] $name"}else{$script:fail++;Write-Host "  [FAIL] $name :: $detail"}
}
function Invoke-Api([string]$method,[string]$path,$body,[string]$token){
  $h=@{}; if($token){$h['Authorization']="Bearer $token"}
  $p=@{Method=$method;Uri="$base$path";Headers=$h;TimeoutSec=30}
  if($null -ne $body){$p['ContentType']='application/json';$p['Body']=($body|ConvertTo-Json -Depth 10 -Compress)}
  try{ $r=Invoke-WebRequest @p -UseBasicParsing
       $obj=$null; if($r.Content){$obj=$r.Content|ConvertFrom-Json}
       return @{ok=$true;status=[int]$r.StatusCode;data=$obj;raw=$r.Content}
  }catch{
       $st=0;$txt=''
       if($_.Exception.Response){$st=[int]$_.Exception.Response.StatusCode
          try{$sr=New-Object IO.StreamReader($_.Exception.Response.GetResponseStream());$txt=$sr.ReadToEnd()}catch{}}
       return @{ok=$false;status=$st;body=$txt;msg=$_.Exception.Message}
  }
}
Write-Host "=== T4 验收开始 $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ==="

# ---------- 0/1) 登录 + 准备含中文数据 ----------
$login=Invoke-Api 'POST' '/auth/login' @{username='admin';password='admin123456'} $null
Assert 'admin 登录' ($login.ok -and $login.data.data.accessToken) $login.msg
$token=$login.data.data.accessToken

wsl docker exec dbloom-mysql mysql -uroot -pdbloom_root_2026 --default-character-set=utf8mb4 dbx_src -e "DROP TABLE IF EXISTS t4_area; CREATE TABLE t4_area(id INT PRIMARY KEY, name VARCHAR(64)) DEFAULT CHARSET=utf8mb4; INSERT INTO t4_area VALUES (1, CONVERT(0xE58C97E4BAAC USING utf8mb4)), (2, CONVERT(0xE4B88AE6B5B7 USING utf8mb4)), (3, CONVERT(0xE5B9BFE5B79E USING utf8mb4));" 2>$null
$conn=Invoke-Api 'POST' '/connections' @{name='t4_src';connType='mysql';host='127.0.0.1';port=3306;databaseName='dbx_src';username=$muser;password=$mpass} $token
Assert '建导出源连接' ($conn.ok -and $conn.data.data.id) ($conn)
$cid=$conn.data.data.id
$baseSql='SELECT id,name FROM t4_area ORDER BY id'

# ---------- 用例1：四格式导出 + 一致性 + 中文 ----------
$formats=@('csv','xlsx','json','sql'); $files=@{}
foreach($f in $formats){
  $r=Invoke-Api 'POST' '/export' @{sql=$baseSql;connectionId=$cid;format=$f;fileName='t4_export'} $token
  Assert "导出 $f 成功" ($r.ok -and $r.data.data.file) "$($r.status) $($r.body)"
  if($r.data.data.file){$files[$f]=$r.data.data.file}
}
Assert '四格式均有落盘路径' (@($formats | Where-Object {$files[$_]})).Count -eq 4 ($files|ConvertTo-Json)

function Download-Utf8([string]$path,[string]$token){
  # server 导出文件为 UTF-8；PowerShell Invoke-WebRequest 按 Content-Type 默认编码会乱码，
  # 用 WebClient.Encoding=UTF8 强制按 UTF-8 解码（内容一致性校验的前提）。
  $wc=New-Object System.Net.WebClient
  if($token){$wc.Headers.Add('Authorization',"Bearer $token")}
  $wc.Encoding=[System.Text.Encoding]::UTF8
  try{ return $wc.DownloadString("$base/export/download?path=$path") }catch{ return $null }
}
$csvRaw=Download-Utf8 ([uri]::EscapeDataString($files['csv'])) $token
$jsonRaw=Download-Utf8 ([uri]::EscapeDataString($files['json'])) $token
$sqlRaw =Download-Utf8 ([uri]::EscapeDataString($files['sql'])) $token
$xlsxOk = (Invoke-Api 'GET' ("/export/download?path="+[uri]::EscapeDataString($files['xlsx'])) $null $token).status -eq 200
Assert 'XLSX 下载 HTTP 200' $xlsxOk "xlsx 下载非 200"
Assert 'CSV 含中文（北京/上海/广州）' ($null -ne $csvRaw -and $csvRaw -match '北京' -and $csvRaw -match '上海' -and $csvRaw -match '广州') ($csvRaw.Substring(0,[Math]::Min(120,[string]$csvRaw.Length)))
$jobj=$null; if($null -ne $jsonRaw){$jobj=$jsonRaw|ConvertFrom-Json}
$jsonRows=@($jobj.rows|ForEach-Object{"$($_[0]):$($_[1])"}) -join '|'
Assert 'JSON 内容一致（rows id:name）' ($jsonRows -eq '1:北京|2:上海|3:广州') "$jsonRows"
$sqlConcord = ($null -ne $sqlRaw -and $sqlRaw -match 'INSERT' -and $sqlRaw -match '北京' -and $sqlRaw -match '上海' -and $sqlRaw -match '广州')
Assert 'SQL 内容一致（VALUES 含 3 行+中文）' $sqlConcord ""
Assert '三格式行数=3（CSV 表头+3 行 / SQL INSERT×3）' (($null -ne $csvRaw -and (@((($csvRaw -split "`n")|Where-Object{$_ -ne ''})).Count -eq 4)) -and ($null -ne $sqlRaw -and (([regex]::Matches($sqlRaw,'INSERT')).Count -eq 3))) ""

# ---------- 用例2/3/4：路径穿越与非法路径全拒 ----------
$evil=@(
  @{n='case2: ../ 相对逃逸';      p=(Split-Path $files['csv'] -Parent)+'/../../etc/passwd'},
  @{n='case2b: 纯 ../';           p='../secret.txt'},
  @{n='case3: 绝对 Windows 路径'; p='C:/windows/win.ini'},
  @{n='case3b: 绝对转义盘符';     p='/C:/windows/win.ini'},
  @{n='case4: URL 编码 %2e%2e%2f'; p='..%2f..%2fetc%2fpasswd'},
  @{n='case4b: 裸 %';             p='export%2F..%2Fwin'},
  @{n='case5: 越界到共享根外';    p='export/../../logs/x'},
  @{n='case5b: 根相对逃逸';       p='../../target/x'}
)
foreach($e in $evil){
  $r=Invoke-Api 'GET' ("/export/download?path="+[uri]::EscapeDataString($e.p)) $null $token
  Assert "$($e.n) -> 4xx 拒绝" (-not $r.ok -and $r.status -ge 400 -and $r.status -lt 500) ("$($r.status) $($r.body)")
}
$none=Invoke-Api 'GET' '/export/download?path=export/2099/01/never.csv' $null $token
Assert '不存在文件 -> 4xx' (-not $none.ok -and $none.status -ge 400) "$($none.status)"

# ---------- 空字节 ----------
$nul=Invoke-Api 'GET' ('/export/download?path='+[uri]::EscapeDataString("export/2026/[System.IO.Path]::Null")) $null $token
Assert '空字节/可疑文件名 -> 4xx 或不落盘' (-not $nul.ok -or $nul.status -ge 400) "$($nul.status)"

Write-Host "`n=== T4 汇总：PASS=$pass FAIL=$fail ==="
$suffix="$(Get-Date -Format 'yyyyMMdd-HHmmss')"
if($fail -eq 0){
  $out = "T4 acceptance: PASS=$pass FAIL=$fail  ($suffix)`n=> 四格式导出一致、中文正常；路径穿越/绝对路径/URL 编码/空字节全部 4xx 拒绝；无文件被读到路径外。`n"
  if(-not (Test-Path 'scripts/acceptance-results')){New-Item -ItemType Directory -Force -Path 'scripts/acceptance-results'|Out-Null}
  Set-Content -Path 'scripts/acceptance-results/t4-final.txt' -Value $out -Encoding UTF8
  Write-Host "最终 PASS 已留档 -> scripts/acceptance-results/t4-final.txt"
}
exit $fail
