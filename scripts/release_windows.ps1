# 构建拾文 Windows x64 安装包：校验版本、编译同版本 CLI、打包 NSIS 并生成 SHA256。
[CmdletBinding()]
param(
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows_environment.ps1')
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

function Invoke-CheckedCommand {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$Description
    )

    # 执行发行阶段外部命令并统一检查退出码，保证失败时不会继续打包旧产物。
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Description失败，退出码：$LASTEXITCODE"
    }
}

function Write-Sha256File {
    param([System.IO.FileInfo]$File)

    # 为安装包生成无 BOM 的 GNU 风格校验文件，便于本地和 GitHub Release 校验。
    $stream = [System.IO.File]::OpenRead($File.FullName)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $stream.Dispose()
        $sha256.Dispose()
    }
    [System.IO.File]::WriteAllText(
        "$($File.FullName).sha256",
        "$hash  $($File.Name)`n",
        [System.Text.UTF8Encoding]::new($false)
    )
}

Push-Location $repoRoot
try {
    $versionScript = Join-Path $repoRoot 'scripts/validate_versions.ps1'
    $versionArguments = @()
    if ($Tag) { $versionArguments = @('-Tag', $Tag) }
    & $versionScript @versionArguments
    if ($LASTEXITCODE -ne 0) {
        throw '版本校验失败'
    }

    $rootPackage = Get-Content 'package.json' -Raw | ConvertFrom-Json
    Invoke-CheckedCommand 'pnpm' @('install', '--frozen-lockfile') '前端依赖校验'

    # CLI 与桌面端使用同一版本；仅对最终 CLI 静态链接 VCRuntime，UCRT 使用 Windows 系统组件。
    $cargoArguments = @(
        'rustc', '--release', '--locked', '-p', 'chatvault-cli', '--',
        '-C', 'target-feature=+crt-static',
        '-C', 'link-arg=/NODEFAULTLIB:libucrt.lib',
        '-C', 'link-arg=/DEFAULTLIB:ucrt.lib'
    )
    Invoke-CheckedCommand 'cargo' $cargoArguments 'CLI 发行构建'

    $metadataJson = & cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) {
        throw '读取 Cargo 输出目录失败'
    }
    $targetDirectory = ($metadataJson | ConvertFrom-Json).target_directory
    $binaries = Join-Path $repoRoot 'apps/desktop/src-tauri/binaries'
    New-Item -ItemType Directory -Force $binaries | Out-Null
    $cliPath = Join-Path $targetDirectory 'release/chatvault-cli.exe'
    if (-not (Test-Path -LiteralPath $cliPath)) {
        throw "发行构建未生成 CLI：$cliPath"
    }
    Copy-Item -LiteralPath $cliPath -Destination (Join-Path $binaries 'chatvault-cli.exe') -Force

    Push-Location (Join-Path $repoRoot 'apps/desktop')
    try {
        Invoke-CheckedCommand 'pnpm' @(
            'exec', 'tauri', 'build',
            '--bundles', 'nsis',
            '--config', 'src-tauri/tauri.release.conf.json',
            '--', '--locked'
        ) '桌面安装包构建'
    }
    finally {
        Pop-Location
    }

    $installerDirectory = Join-Path $targetDirectory 'release/bundle/nsis'
    $installers = @(Get-ChildItem -LiteralPath $installerDirectory -Filter "*$($rootPackage.version)*-setup.exe" -File)
    if ($installers.Count -eq 0) {
        throw '构建未生成预期安装包'
    }
    foreach ($installer in $installers) {
        Write-Sha256File $installer
        Write-Output "安装包：$($installer.FullName)"
        Write-Output "校验文件：$($installer.FullName).sha256"
    }
}
finally {
    Pop-Location
}
