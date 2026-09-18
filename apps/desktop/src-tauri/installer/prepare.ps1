# 安装生命周期检查：只清理指向本安装目录的计划任务，运行中的归档任务必须先自然结束。
param([Parameter(Mandatory = $true)][string]$InstallDirectory)
$ErrorActionPreference = 'Stop'
$task = $null
$restoreEnabled = $false

# 判断任务动作是否属于本安装目录：直接 CLI、历史 PowerShell 包装或 VBS 启动器。
function Test-ActionBelongsToInstall {
    param($Action, [string]$InstallDirectory, [string]$ExpectedCli, [string]$ExpectedVbs)
    $exec = if ($null -ne $Action.Execute) { [string]$Action.Execute.Trim('"') } else { '' }
    $argText = if ($null -ne $Action.Arguments) { [string]$Action.Arguments } else { '' }
    if ($exec -ieq $ExpectedCli) { return $true }
    if ($argText) {
        if ($argText.IndexOf($ExpectedCli, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) { return $true }
        if ($argText.IndexOf($ExpectedVbs, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) { return $true }
    }
    # wscript/cscript 启动器：参数中出现本安装目录即视为归属。
    if ($exec -match '(?i)\\wscript(\.exe)?$' -or $exec -match '(?i)\\cscript(\.exe)?$') {
        if ($argText -and $argText.IndexOf($InstallDirectory, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
            return $true
        }
    }
    return $false
}

try {
    $expectedCli = [System.IO.Path]::GetFullPath((Join-Path $InstallDirectory 'chatvault-cli.exe'))
    $expectedVbs = [System.IO.Path]::GetFullPath((Join-Path $InstallDirectory 'chatvault-scheduled-run.vbs'))
    # 枚举成功后再判断不存在，不能将权限错误当成任务不存在。
    $candidate = Get-ScheduledTask | Where-Object { $_.TaskName -eq 'ChatVaultScheduledScan' -and $_.TaskPath -eq '\' }
    $actions = @()
    if ($candidate) { $actions = @($candidate.Actions) }
    if ($candidate -and @($actions | Where-Object { Test-ActionBelongsToInstall $_ $InstallDirectory $expectedCli $expectedVbs }).Count -gt 0) {
        $task = $candidate
        $restoreEnabled = $task.State -ne 'Disabled'
        $task | Disable-ScheduledTask | Out-Null
    }
    $running = Get-Process -Name 'chatvault-cli' -ErrorAction SilentlyContinue | Where-Object { $_.Path -ieq $expectedCli }
    if ($running -or ($task -and (Get-ScheduledTask -TaskName $task.TaskName -TaskPath $task.TaskPath).State -eq 'Running')) {
        throw '归档任务仍在运行，请等待任务完成后重试。'
    }
    if ($task) { $task | Unregister-ScheduledTask -Confirm:$false }
    exit 0
} catch {
    if ($task -and $restoreEnabled) { $task | Enable-ScheduledTask -ErrorAction Continue | Out-Null }
    Write-Output "安装准备失败：$($_.Exception.Message)"
    exit 1
}
