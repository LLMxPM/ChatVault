# 初始化并校验 ChatVault 的标准 Windows x64 MSVC 构建环境。
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

function Require-Command {
    param([string]$Name)

    # 在执行 Cargo 或 Tauri 之前检查命令是否已安装，避免进入长时间编译后才失败。
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "缺少命令 $Name。请安装标准 Node/pnpm、Rust MSVC 或 Visual Studio Build Tools。"
    }
}

function Resolve-ProgramFilesRoot {
    # 某些精简宿主进程会丢失 ProgramFiles* 环境变量；按标准路径回退，保证本机与 CI 都能找到 VS。
    param([ValidateSet('ProgramFiles', 'ProgramFilesX86')][string]$Which)

    $envName = if ($Which -eq 'ProgramFilesX86') { 'ProgramFiles(x86)' } else { 'ProgramFiles' }
    $fromEnv = [Environment]::GetEnvironmentVariable($envName)
    if ([string]::IsNullOrWhiteSpace($fromEnv)) {
        $fromEnv = if ($Which -eq 'ProgramFilesX86') { ${env:ProgramFiles(x86)} } else { $env:ProgramFiles }
    }
    if ($fromEnv -and (Test-Path -LiteralPath $fromEnv)) {
        return $fromEnv
    }

    $fallbacks = if ($Which -eq 'ProgramFilesX86') {
        @('C:\Program Files (x86)', 'C:\Program Files')
    } else {
        @('C:\Program Files')
    }
    foreach ($candidate in $fallbacks) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }
    return $null
}

function Import-VisualStudioEnvironment {
    # 查找完整的 Visual Studio C++ 工具链，并把 vcvars64 的环境导入当前 PowerShell 进程。
    $vswhereCandidates = @()
    foreach ($root in @((Resolve-ProgramFilesRoot 'ProgramFilesX86'), (Resolve-ProgramFilesRoot 'ProgramFiles'))) {
        if ($root) {
            $vswhereCandidates += (Join-Path $root 'Microsoft Visual Studio\Installer\vswhere.exe')
        }
    }
    $vswhere = $vswhereCandidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
    if (-not $vswhere) {
        throw '未找到 vswhere.exe。请安装 Visual Studio Build Tools，并勾选 C++ 构建工具。'
    }

    # 先接收 JSON 数组，再逐项筛选；外层 @() 会在 Windows PowerShell 中产生嵌套数组，拼接多套安装路径。
    $installations = & $vswhere -all -products '*' -requires 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' -format json | ConvertFrom-Json
    $visualStudio = $installations | Where-Object {
        $_.isComplete -eq $true -and $_.installationPath
    } | Select-Object -First 1
    if (-not $visualStudio) {
        throw '未找到完整的 Visual Studio C++ 安装。请安装 Visual Studio Build Tools、C++ 工具和 Windows SDK。'
    }

    $vcvars = Join-Path $visualStudio.installationPath 'VC\Auxiliary\Build\vcvars64.bat'
    if (-not (Test-Path -LiteralPath $vcvars)) {
        throw "Visual Studio 缺少 x64 环境脚本：$vcvars"
    }

    $envDump = cmd.exe /d /c "call `"$vcvars`" >nul && set"
    if ($LASTEXITCODE -ne 0) {
        throw "加载 Visual Studio x64 编译环境失败（退出码 $LASTEXITCODE）：$vcvars"
    }
    foreach ($line in $envDump) {
        if ($line -match '^([^=]+)=(.*)$') {
            Set-Item -Path "env:$($matches[1])" -Value $matches[2]
        }
    }

    return $visualStudio.installationPath
}

function Assert-WindowsSdk {
    # 校验链接器真正需要的 Windows SDK 库，而不是只检查 cl.exe 是否存在。
    if ([string]::IsNullOrWhiteSpace($env:WindowsSdkDir) -or [string]::IsNullOrWhiteSpace($env:WindowsSDKVersion)) {
        throw '未加载 Windows SDK 环境。请在 Visual Studio Installer 中安装 Windows 10/11 SDK。'
    }

    $sdkRoot = $env:WindowsSdkDir.TrimEnd('\')
    $sdkVersion = $env:WindowsSDKVersion.Trim('\')
    $sdkLibRoot = Join-Path $sdkRoot "Lib\$sdkVersion"
    $umLib = Join-Path $sdkLibRoot 'um\x64'
    $ucrtLib = Join-Path $sdkLibRoot 'ucrt\x64'
    foreach ($library in @('kernel32.lib', 'OleAut32.lib')) {
        $libraryPath = Join-Path $umLib $library
        if (-not (Test-Path -LiteralPath $libraryPath)) {
            throw "Windows SDK 缺少 $library：$libraryPath"
        }
    }
    if (-not (Test-Path -LiteralPath (Join-Path $ucrtLib 'ucrt.lib'))) {
        throw "Windows SDK 缺少 ucrt.lib：$ucrtLib"
    }

    $env:LIB = "$umLib;$ucrtLib;$env:LIB"
}

Push-Location $repoRoot
try {
    $visualStudioPath = Import-VisualStudioEnvironment
    foreach ($command in @('cargo', 'rustc', 'node', 'pnpm', 'cl', 'link', 'rc')) {
        Require-Command $command
    }

    $rustInfo = (& rustc -vV) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $rustInfo -notmatch 'host: x86_64-pc-windows-msvc') {
        throw '当前 Rust 工具链不是 x86_64-pc-windows-msvc。请使用 stable MSVC 工具链。'
    }
    $nodeMajor = [int]((& node --version).Trim().TrimStart('v').Split('.')[0])
    if ($LASTEXITCODE -ne 0 -or $nodeMajor -ne 22) {
        throw '项目要求 Node.js 22.x。'
    }
    $pnpmVersion = (& pnpm --version).Trim()
    if ($LASTEXITCODE -ne 0 -or $pnpmVersion -ne '10.30.3') {
        throw "项目要求 pnpm 10.30.3，当前为 $pnpmVersion。"
    }

    Assert-WindowsSdk

    $sqliteLibs = Join-Path $repoRoot 'libs\win_x64'
    $sqliteLibrary = Join-Path $sqliteLibs 'sqlite3.lib'
    if (-not (Test-Path -LiteralPath $sqliteLibrary)) {
        throw "缺少仓库 SQLite 静态库：$sqliteLibrary"
    }
    $env:SQLITE3_LIB_DIR = $sqliteLibs
    $env:LIB = "$sqliteLibs;$env:LIB"

    Write-Output "Windows 构建环境通过：VS=$visualStudioPath Rust=MSVC Node=22 pnpm=$pnpmVersion"
    Write-Output "SQLite 静态库：$sqliteLibrary"
}
finally {
    Pop-Location
}
