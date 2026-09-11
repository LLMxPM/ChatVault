<#
.SYNOPSIS
ChatVault Windows 环境一键编译与运行辅助脚本

.DESCRIPTION
自动加载 Visual Studio 2019 C++ 编译环境、配置本地 Windows API 链接库（win_libs）
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

$ErrorActionPreference = "Stop"

Write-Host "=== 配置 ChatVault Windows 编译与运行环境 ===" -ForegroundColor Cyan

# 1. 加载 Visual Studio MSVC 环境
$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (Test-Path $vcvars) {
    cmd /c "call `"$vcvars`" && set" | ForEach-Object {
        if ($_ -match '^(.*?)=(.*)$') {
            Set-Item -Path "env:\$($matches[1])" -Value $matches[2]
        }
    }
}

# 2. 配置专属 Windows API 库与 SQLite 预编译库路径
$winLibs = "C:\codetools\rust\win_libs"
$env:LIB = "$env:LIB;$winLibs"
$env:SQLITE3_LIB_DIR = $winLibs

# 3. 确保 codetools/rust 工具链路径优先生效
$env:CARGO_HOME = "C:\codetools\rust\cargo"
$env:RUSTUP_HOME = "C:\codetools\rust\rustup"
$env:PATH = "C:\codetools\rust\cargo\bin;C:\codetools\rust\llvm-mingw\bin;$env:PATH"

# 4. 执行 cargo 命令
switch ($Action) {
    "check" {
        cargo check --workspace
    }
    "test" {
        cargo test --workspace
    }
    "build" {
        cargo build --workspace
    }
    "run" {
        cargo run -p chatvault-cli -- $RemainingArgs
    }
    default {
        cargo $Action $RemainingArgs
    }
}
