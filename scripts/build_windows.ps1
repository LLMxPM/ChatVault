<#
.SYNOPSIS
ChatVault Windows 环境一键编译与运行辅助脚本

.DESCRIPTION
探测 Visual Studio C++ 编译环境、配置 Windows API 链接库
与包含 FTS5 的预编译 SQLite 静态库路径，并执行指定的 cargo 命令（如 build、test、run）。

.EXAMPLE
.\scripts\build_windows.ps1 check
.\scripts\build_windows.ps1 test
.\scripts\build_windows.ps1 run -- detect
.\scripts\build_windows.ps1 run -- scan --target wechat
.\scripts\build_windows.ps1 run -- search "WorkBuddy"
#>

param(
    [Parameter(Position = 0)]
    [string]$Action = "check",
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)

. (Join-Path $PSScriptRoot 'windows_environment.ps1')
Set-Location -LiteralPath $repoRoot

# 4. 执行 cargo 命令
switch ($Action) {
    "check" {
        cargo check --workspace
    }
    "test" {
        cargo test --workspace @RemainingArgs
    }
    "build" {
        cargo build --workspace
    }
    "run" {
        cargo run -p chatvault-cli -- $RemainingArgs
    }
    "cli-run" {
        cargo run -p chatvault-cli -- $RemainingArgs
    }
    "desktop-build" {
        Write-Host ">>> 构建桌面端前端资源 (pnpm build) <<<" -ForegroundColor Green
        Push-Location "apps\desktop"
        try {
            pnpm build
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
        Write-Host ">>> 编译桌面端 Rust 原生二进制 (cargo build -p chatvault-desktop) <<<" -ForegroundColor Green
        cargo build -p chatvault-desktop $RemainingArgs
    }
    "desktop-run" {
        Write-Host ">>> 启动 ChatVault 桌面端应用 <<<" -ForegroundColor Green
        cargo run -p chatvault-desktop $RemainingArgs
    }
    default {
        cargo $Action $RemainingArgs
    }
}

# 将实际编译/测试失败传递给调用方，避免脚本错误地报告成功。
exit $LASTEXITCODE
