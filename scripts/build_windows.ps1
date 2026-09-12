<#
.SYNOPSIS
ChatVault Windows x64 Rust 与桌面端构建入口。

.DESCRIPTION
统一初始化标准 Visual Studio、Windows SDK、Rust MSVC、Node/pnpm 和仓库 SQLite
环境，再执行检查、格式、测试、Lint、桌面编译或 Tauri 开发命令。
#>

param(
    [Parameter(Position = 0)]
    [ValidateSet('check', 'lint', 'test', 'desktop-build')]
    [string]$Action = 'check',
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)

. (Join-Path $PSScriptRoot 'windows_environment.ps1')
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

function Invoke-CheckedCommand {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$Description
    )

    # 执行外部命令并显式传递退出码，避免 PowerShell 忽略 Cargo/pnpm 失败。
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Description失败，退出码：$LASTEXITCODE"
    }
}

Push-Location $repoRoot
try {
    switch ($Action) {
        'check' {
            Invoke-CheckedCommand 'cargo' @('check', '--workspace', '--locked') 'Rust workspace 检查'
        }
        'lint' {
            Invoke-CheckedCommand 'cargo' @('clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings') 'Rust Clippy 检查'
        }
        'test' {
            $arguments = @('test', '--workspace', '--locked') + @($RemainingArgs)
            Invoke-CheckedCommand 'cargo' $arguments 'Rust workspace 测试'
        }
        'desktop-build' {
            # 前端构建逻辑由桌面 package.json 维护，脚本只负责编排前端与 Rust 两个阶段。
            Invoke-CheckedCommand 'pnpm' @('--filter', 'chatvault-desktop', 'build') '桌面端前端构建'
            $arguments = @('build', '-p', 'chatvault-desktop', '--locked') + @($RemainingArgs)
            Invoke-CheckedCommand 'cargo' $arguments '桌面端 Rust 编译'
        }
    }
}
finally {
    Pop-Location
}
