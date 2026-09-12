# 安装生命周期检查：只清理指向本安装目录的计划任务，运行中的归档任务必须先自然结束。
param([Parameter(Mandatory = $true)][string]$InstallDirectory)
$ErrorActionPreference = 'Stop'
$task = $null
$restoreEnabled = $false
try {
    $expectedCli = [System.IO.Path]::GetFullPath((Join-Path $InstallDirectory 'chatvault-cli.exe'))
    # 枚举成功后再判断不存在，不能将权限错误当成任务不存在。
    $candidate = Get-ScheduledTask | Where-Object { $_.TaskName -eq 'ChatVaultScheduledScan' -and $_.TaskPath -eq '\' }
    if ($candidate -and @($candidate.Actions | Where-Object { $_.Execute.Trim('"') -ieq $expectedCli }).Count -gt 0) {
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
