#Requires -Version 7
# Builds (unless -NoBuild) and packages the release zip: dist/Texalith-<version>-win-x64.zip + SHA256SUMS.txt.
# Run from anywhere. The zip holds texalith.exe, cs-shaders beside it, and the named files only.
#   tools\package-release.ps1                  build, stage, zip
#   tools\package-release.ps1 -Suffix alpha.1  names it Texalith-0.1.0-alpha.1-win-x64.zip
#   tools\package-release.ps1 -NoBuild -TargetDir <dir holding texalith.exe>
param(
    [switch]$NoBuild,
    [switch]$AllowDirty,
    [string]$TargetDir,
    [string]$Suffix = ''
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $AllowDirty -and (git status --porcelain)) {
    throw 'git tree is dirty; commit or pass -AllowDirty'
}
$m = [regex]::Match((Get-Content Cargo.toml -Raw -Encoding utf8), '(?m)^version\s*=\s*"([^"]+)"')
if (-not $m.Success) { throw 'no version in Cargo.toml' }
$version = $m.Groups[1].Value
if ($Suffix) { $version = "$version-$Suffix" }

if (-not $TargetDir) { $TargetDir = Join-Path $root 'target\x86_64-pc-windows-msvc\release' }
if (-not $NoBuild) {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    if ($env:CARGO_TARGET_DIR) { $TargetDir = Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc\release' }
}

$name = "Texalith-$version"
$dist = Join-Path $root 'dist'
$stage = Join-Path $dist $name
$zip = Join-Path $dist "$name-win-x64.zip"
Remove-Item -Recurse -Force $stage, $zip -ErrorAction SilentlyContinue
New-Item -ItemType Directory (Join-Path $stage 'docs') | Out-Null

$exe = Join-Path $TargetDir 'texalith.exe'
if (-not (Test-Path $exe)) { throw "missing $exe" }
Copy-Item $exe $stage
foreach ($f in 'README.md', 'CHANGELOG.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_LICENSES.md') {
    Copy-Item (Join-Path $root $f) $stage
}
Copy-Item (Join-Path $root 'docs\Guide.md') (Join-Path $stage 'docs')
Copy-Item (Join-Path $root 'docs\assets') (Join-Path $stage 'docs\assets') -Recurse # the emblem README.md shows
Copy-Item (Join-Path $root 'LICENSES') (Join-Path $stage 'LICENSES') -Recurse
# texalith.exe reads its shaders from cs-shaders beside it (src/bundle.rs).
Copy-Item (Join-Path $root 'assets\cs-shaders') (Join-Path $stage 'cs-shaders') -Recurse
$tracked = @(git ls-files assets/cs-shaders).Count
$staged = @(Get-ChildItem (Join-Path $stage 'cs-shaders') -Recurse -File).Count
if ($tracked -ne $staged) { throw "cs-shaders: $staged files staged, $tracked tracked" }
# The source this build corresponds to (GPL): this repo's commit and the Tessalith commit it was built against.
$tess = [regex]::Match((Get-Content Cargo.toml -Raw), 'Tessalith", rev = "([0-9a-f]{40})"').Groups[1].Value
if (-not $tess) { throw 'no Tessalith rev in Cargo.toml' }
"Texalith $version`nTexalith commit: $(git rev-parse HEAD)`nTessalith commit: $tess" |
    Set-Content (Join-Path $stage 'BUILD.txt') -Encoding ascii

Compress-Archive -Path $stage -DestinationPath $zip
$sums = foreach ($f in $zip, (Join-Path $stage 'texalith.exe')) {
    $h = (Get-FileHash $f -Algorithm SHA256).Hash.ToLower()
    "$h  $(Split-Path -Leaf $f)"
}
$sums | Set-Content (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii
$sums
