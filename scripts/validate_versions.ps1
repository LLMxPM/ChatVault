# 校验根 package.json、桌面包和 Cargo workspace 的版本，并可选校验 Git tag。
# 支持正式版 vX.Y.Z 与预发布 vX.Y.Z-<pre>（如 v0.1.0-alpha.1）。
[CmdletBinding()]
param(
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

# 预发布段允许 alpha/test/rc.1 等；版本与 tag 去掉 v 后必须完全一致。
$script:SemVerTagPattern = '^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$'
$script:SemVerVersionPattern = '^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$'

function Get-CargoWorkspaceVersion {
    param([string]$ManifestPath)

    # 读取 Cargo workspace 的版本字段；成员 crate 通过 workspace 继承该版本。
    $manifest = Get-Content -LiteralPath $ManifestPath -Raw
    $match = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $match.Success) {
        throw "未找到 Cargo workspace 版本：$ManifestPath"
    }
    return $match.Groups[1].Value
}

function Assert-VersionEqual {
    param(
        [string]$Name,
        [string]$Expected,
        [string]$Actual
    )

    # 将所有发行版本校验集中到一个失败点，避免不同脚本各自实现比较逻辑。
    if ([string]::IsNullOrWhiteSpace($Actual) -or $Expected -ne $Actual) {
        throw "版本不一致：$Name=$Actual，期望=$Expected"
    }
}

function Test-PrereleaseVersion {
    param([string]$Version)

    # 含预发布段（主版本号后的 -alpha / -rc.1 等）视为预发布。
    return $Version -match '-'
}

Push-Location $repoRoot
try {
    $rootPackage = Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json
    $desktopPackage = Get-Content -LiteralPath 'apps/desktop/package.json' -Raw | ConvertFrom-Json
    $rootVersion = [string]$rootPackage.version
    $desktopVersion = [string]$desktopPackage.version
    $cargoVersion = Get-CargoWorkspaceVersion (Join-Path $repoRoot 'Cargo.toml')

    if ([string]::IsNullOrWhiteSpace($rootVersion)) {
        throw '根 package.json 未定义有效版本'
    }
    if ($rootVersion -notmatch $script:SemVerVersionPattern) {
        throw "发行版本必须符合 SemVer（可选 -预发布段）：$rootVersion"
    }
    Assert-VersionEqual 'apps/desktop/package.json' $rootVersion $desktopVersion
    Assert-VersionEqual 'Cargo.toml' $rootVersion $cargoVersion

    if ($Tag) {
        if ($Tag -notmatch $script:SemVerTagPattern) {
            throw "发布 tag 必须符合 vX.Y.Z 或 vX.Y.Z-<pre>：$Tag"
        }
        Assert-VersionEqual 'Git tag' $rootVersion $Tag.Substring(1)
    }

    $channel = if (Test-PrereleaseVersion $rootVersion) { '预发布' } else { '正式' }
    Write-Output "版本校验通过：$rootVersion（$channel）"
}
finally {
    Pop-Location
}
