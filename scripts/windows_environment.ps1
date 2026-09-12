# 配置 Windows x64 构建环境：探测 Visual Studio，使用当前 Rust 工具链及仓库 SQLite。
param([string]$ToolchainRoot = $env:CHATVAULT_TOOLCHAIN_ROOT)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

# 当前机器的独立工具链可通过参数或环境变量指定；正常安装的工具链直接使用 PATH。
if (-not $ToolchainRoot -and -not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    if (Test-Path -LiteralPath 'C:\codetools\rust\cargo\bin\cargo.exe') {
        $ToolchainRoot = 'C:\codetools\rust'
    }
}
if ($ToolchainRoot) {
    $env:CARGO_HOME = Join-Path $ToolchainRoot 'cargo'
    $env:RUSTUP_HOME = Join-Path $ToolchainRoot 'rustup'
    $env:PATH = "$(Join-Path $env:CARGO_HOME 'bin');$env:PATH"
    $resourceCompiler = Join-Path $ToolchainRoot 'llvm-mingw\bin\rc.exe'
    if (Test-Path -LiteralPath $resourceCompiler) {
        # LLVM RC 的默认代码页不能读取中文产品名，统一按 UTF-8 编译资源。
        $env:CHATVAULT_RESOURCE_COMPILER = $resourceCompiler
        $env:RC = Join-Path $PSScriptRoot 'resource_compiler.cmd'
        $env:PATH = "$(Split-Path -Parent $resourceCompiler);$env:PATH"
    }
    $windowsLibs = Join-Path $ToolchainRoot 'win_libs'
    if (Test-Path -LiteralPath $windowsLibs) { $env:LIB = "$windowsLibs;$env:LIB" }
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (Test-Path -LiteralPath $vswhere) {
    $visualStudio = & $vswhere -all -products '*' -property installationPath | Where-Object {
        Test-Path -LiteralPath (Join-Path $_ 'VC\Auxiliary\Build\vcvars64.bat')
    } | Select-Object -First 1
    if ($visualStudio) {
        $vcvars = Join-Path $visualStudio 'VC\Auxiliary\Build\vcvars64.bat'
        cmd /d /c "call `"$vcvars`" >nul && set" | ForEach-Object {
            if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "env:$($matches[1])" -Value $matches[2] }
        }
        if ($LASTEXITCODE -ne 0) { throw '加载 Visual Studio 编译环境失败' }
    }
}

$sqliteLibs = Join-Path $repoRoot 'libs\win_x64'
if (-not (Test-Path -LiteralPath (Join-Path $sqliteLibs 'sqlite3.lib'))) { throw '缺少 libs/win_x64/sqlite3.lib' }
$env:SQLITE3_LIB_DIR = $sqliteLibs
$env:LIB = "$sqliteLibs;$env:LIB"
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw '请安装 Rust MSVC 工具链，或设置 CHATVAULT_TOOLCHAIN_ROOT' }
