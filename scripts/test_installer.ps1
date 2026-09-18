# 安装钩子隔离验证：用内存替身检查任务归属、运行保护与失败恢复，不操作真实系统任务。
$ErrorActionPreference = 'Stop'
$prepareScript = Join-Path $PSScriptRoot '../apps/desktop/src-tauri/installer/prepare.ps1'
$installPath = 'C:\Test Installation\拾文'

# 模拟任务查询，按场景注入查询失败或任务内容。
function Get-ScheduledTask {
    param($TaskPath, $TaskName)
    if ($global:chatvaultTest_queryFails) { throw '模拟任务查询失败' }
    if ($global:chatvaultTest_task) { return $global:chatvaultTest_task }
}

# 记录任务禁用操作，不调用 Windows 计划任务服务。
function Disable-ScheduledTask {
    param([Parameter(ValueFromPipeline = $true)]$InputObject)
    process { $global:chatvaultTest_disabled = $true }
}

# 记录失败后的恢复操作。
function Enable-ScheduledTask {
    param([Parameter(ValueFromPipeline = $true)]$InputObject)
    process { $global:chatvaultTest_enabled = $true }
}

# 模拟任务删除，验证只对本安装目录的任务执行。
function Unregister-ScheduledTask {
    param([Parameter(ValueFromPipeline = $true)]$InputObject, [switch]$Confirm)
    process {
        if ($global:chatvaultTest_deleteFails) { throw '模拟任务删除失败' }
        $global:chatvaultTest_removed = $true
    }
}

# 模拟正在运行的归档进程，不读取或终止真实进程。
function Get-Process {
    param($Name, $ErrorAction)
    if ($global:chatvaultTest_processPath) { [pscustomobject]@{ Path = $global:chatvaultTest_processPath } }
}

# 每个场景使用独立状态，检查退出码和所有任务副作用。
function Test-PrepareScenario {
    param([string]$Name, [string]$TaskExecutable, [string]$TaskArguments, [string]$ProcessExecutable,
          [bool]$QueryFails, [bool]$DeleteFails, [int]$ExpectedCode,
          [bool]$ExpectedRemoved, [bool]$ExpectedDisabled, [bool]$ExpectedEnabled)
    $global:chatvaultTest_queryFails = $QueryFails
    $global:chatvaultTest_deleteFails = $DeleteFails
    $global:chatvaultTest_processPath = $ProcessExecutable
    $global:chatvaultTest_removed = $false
    $global:chatvaultTest_disabled = $false
    $global:chatvaultTest_enabled = $false
    $global:chatvaultTest_task = $null
    if ($TaskExecutable -or $TaskArguments) {
        $global:chatvaultTest_task = [pscustomobject]@{
            TaskName = 'ChatVaultScheduledScan'; TaskPath = '\'; State = 'Ready'
            Actions = @([pscustomobject]@{ Execute = $TaskExecutable; Arguments = $TaskArguments })
        }
    }
    & $prepareScript -InstallDirectory $installPath | Out-Null
    if ($LASTEXITCODE -ne $ExpectedCode -or $global:chatvaultTest_removed -ne $ExpectedRemoved -or
        $global:chatvaultTest_disabled -ne $ExpectedDisabled -or $global:chatvaultTest_enabled -ne $ExpectedEnabled) {
        throw "验证失败：$Name（exit=$LASTEXITCODE removed=$global:chatvaultTest_removed disabled=$global:chatvaultTest_disabled enabled=$global:chatvaultTest_enabled）"
    }
    Write-Output "通过：$Name"
}

$ownCli = Join-Path $installPath 'chatvault-cli.exe'
$ownVbs = Join-Path $installPath 'chatvault-scheduled-run.vbs'
$otherCli = 'C:\Other\chatvault-cli.exe'
$otherVbs = 'C:\Other\chatvault-scheduled-run.vbs'
$ownVbsArgs = "//B //Nologo `"$ownVbs`""
$otherVbsArgs = "//B //Nologo `"$otherVbs`""
$ownPowerShellArgs = "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -Command `"& '$ownCli' scheduled-run --db 'C:\db'`""
$otherPowerShellArgs = "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -Command `"& '$otherCli' scheduled-run --db 'C:\db'`""

Test-PrepareScenario '任务不存在' '' '' '' $false $false 0 $false $false $false
Test-PrepareScenario '清理本目录任务（中文和空格路径）' $ownCli '' '' $false $false 0 $true $true $false
Test-PrepareScenario '保留其他安装目录任务' $otherCli '' '' $false $false 0 $false $false $false
Test-PrepareScenario '清理本目录 VBS 启动任务' 'C:\Windows\System32\wscript.exe' $ownVbsArgs '' $false $false 0 $true $true $false
Test-PrepareScenario '保留其他目录 VBS 启动任务' 'C:\Windows\System32\wscript.exe' $otherVbsArgs '' $false $false 0 $false $false $false
Test-PrepareScenario '清理历史 PowerShell 包装任务' 'powershell.exe' $ownPowerShellArgs '' $false $false 0 $true $true $false
Test-PrepareScenario '保留其他目录 PowerShell 包装任务' 'powershell.exe' $otherPowerShellArgs '' $false $false 0 $false $false $false
Test-PrepareScenario '本目录归档运行时阻止安装并恢复任务' $ownCli '' $ownCli $false $false 1 $false $true $true
Test-PrepareScenario '其他目录进程不阻止安装' $ownCli '' $otherCli $false $false 0 $true $true $false
Test-PrepareScenario '查询失败不当作不存在' '' '' '' $true $false 1 $false $false $false
Test-PrepareScenario '删除失败恢复任务' $ownCli '' '' $false $true 1 $false $true $true
exit 0
