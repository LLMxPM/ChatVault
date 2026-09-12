# 构建拾文 Windows x64 安装包：校验版本、编译同版本 CLI、打包 NSIS 并生成 SHA256。
param()
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows_environment.ps1')
Push-Location $repoRoot
try {
    $desktopPackage = Get-Content 'apps/desktop/package.json' -Raw | ConvertFrom-Json
    $rootPackage = Get-Content 'package.json' -Raw | ConvertFrom-Json
    $cargoManifest = Get-Content 'Cargo.toml' -Raw
    $cargoVersion = [regex]::Match($cargoManifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
    if ($desktopPackage.version -ne $rootPackage.version -or $desktopPackage.version -ne $cargoVersion) {
        throw 'Cargo workspace、根 package.json 和桌面 package.json 的版本必须一致'
    }
    $hostInfo = (& rustc -vV) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $hostInfo -notmatch 'host: x86_64-pc-windows-msvc') { throw '发行构建需要 Windows x64 MSVC 工具链' }
    pnpm install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw '前端依赖校验失败' }
    # CLI 静态链接 VCRuntime，UCRT 使用 Windows 系统组件；仅设置最终程序的链接参数。
    cargo rustc --release --locked -p chatvault-cli -- -C target-feature=+crt-static -C link-arg=/NODEFAULTLIB:libucrt.lib -C link-arg=/DEFAULTLIB:ucrt.lib
    if ($LASTEXITCODE -ne 0) { throw 'CLI 发行构建失败' }
    # Cargo 的实际输出目录可能由环境变量配置，通过 metadata 获取，避免复制旧产物。
    $metadataJson = cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw '读取 Cargo 输出目录失败' }
    $targetDirectory = ($metadataJson | ConvertFrom-Json).target_directory
    $binaries = Join-Path $repoRoot 'apps/desktop/src-tauri/binaries'
    New-Item -ItemType Directory -Force $binaries | Out-Null
    Copy-Item -LiteralPath (Join-Path $targetDirectory 'release/chatvault-cli.exe') -Destination (Join-Path $binaries 'chatvault-cli.exe')
    Push-Location (Join-Path $repoRoot 'apps/desktop')
    try {
        # 直接运行 pnpm 安装的 CLI，保留传给 Cargo 的参数分隔符。
        node node_modules/@tauri-apps/cli/tauri.js build --bundles nsis --config src-tauri/tauri.release.conf.json -- --locked
        if ($LASTEXITCODE -ne 0) { throw '桌面安装包构建失败' }
    } finally { Pop-Location }
    $installerDirectory = Join-Path $targetDirectory 'release/bundle/nsis'
    $installers = @(Get-ChildItem -LiteralPath $installerDirectory -Filter "*$($desktopPackage.version)*-setup.exe")
    if ($installers.Count -eq 0) { throw '构建未生成预期安装包' }
    foreach ($installer in $installers) {
        $stream = [System.IO.File]::OpenRead($installer.FullName)
        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            $hash = [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '').ToLowerInvariant()
        } finally {
            $stream.Dispose()
            $sha256.Dispose()
        }
        [System.IO.File]::WriteAllText("$($installer.FullName).sha256", "$hash  $($installer.Name)`n", [System.Text.UTF8Encoding]::new($false))
        Write-Host "安装包：$($installer.FullName)"
    }
} finally {
    Pop-Location
}
