# 校验根 package.json、桌面包和 Cargo workspace 的版本，并可选校验 Git tag。
[CmdletBinding()]
param(
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

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
    Assert-VersionEqual 'apps/desktop/package.json' $rootVersion $desktopVersion
    Assert-VersionEqual 'Cargo.toml' $rootVersion $cargoVersion

    if ($Tag) {
        if ($Tag -notmatch '^v\d+\.\d+\.\d+$') {
            throw "发布 tag 必须符合 vX.Y.Z：$Tag"
        }
        Assert-VersionEqual 'Git tag' $rootVersion $Tag.Substring(1)
    }

    Write-Output "版本校验通过：$rootVersion"
}
finally {
    Pop-Location
}
