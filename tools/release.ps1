# Builds a BlinkLoupe release: the exe (cargo), the installer (NSIS) and latest.json (what the in-app updater reads).
#   powershell -ExecutionPolicy Bypass -File tools\release.ps1 -Notes "What changed"
#   ... -Publish   also uploads both files as release v<version> of mifunetron/blinkloupe-releases (needs gh, signed in)
# The version comes from src-tauri\tauri.conf.json; bump it there and in src-tauri\Cargo.toml first.
param(
    [string]$Notes = "",
    [switch]$NoBuild,
    [switch]$Publish
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$repo = "mifunetron/blinkloupe-releases"
$version = (Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
$cargoVersion = (Select-String -Path (Join-Path $root "src-tauri\Cargo.toml") -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if ($cargoVersion -ne $version) { throw "tauri.conf.json says $version but Cargo.toml says $cargoVersion" }

if (-not $NoBuild) {
    Push-Location (Join-Path $root "src-tauri")
    try {
        # cargo reports progress on stderr; only its exit code means failure
        $ErrorActionPreference = "Continue"
        & cargo build --release
        $code = $LASTEXITCODE
        $ErrorActionPreference = "Stop"
        if ($code -ne 0) { throw "cargo build failed ($code)" }
    } finally {
        Pop-Location
    }
}

$candidates = New-Object System.Collections.Generic.List[string]
if ($env:NSIS_HOME) { $candidates.Add((Join-Path $env:NSIS_HOME "makensis.exe")) }
$onPath = Get-Command makensis.exe -ErrorAction SilentlyContinue
if ($onPath) { $candidates.Add($onPath.Source) }
$candidates.Add((Join-Path $env:LOCALAPPDATA "tauri\NSIS\makensis.exe"))   # where Tauri's bundler keeps its copy
$candidates.Add((Join-Path ${env:ProgramFiles(x86)} "NSIS\makensis.exe"))
$candidates.Add((Join-Path $env:ProgramFiles "NSIS\makensis.exe"))
$makensis = $null
foreach ($c in $candidates) {
    if (Test-Path -LiteralPath $c) { $makensis = $c; break }
}
if (-not $makensis) { throw "makensis.exe not found - install NSIS 3 or set NSIS_HOME" }

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force $dist | Out-Null
$ErrorActionPreference = "Continue"
& $makensis /V2 "/DVERSION=$version" (Join-Path $root "tools\installer.nsi")
$code = $LASTEXITCODE
$ErrorActionPreference = "Stop"
if ($code -ne 0) { throw "makensis failed ($code)" }

$setup = Join-Path $dist "BlinkLoupe-$version-setup.exe"
$item = Get-Item $setup
$manifest = [ordered]@{
    version = $version
    notes   = $Notes
    url     = "https://github.com/$repo/releases/download/v$version/BlinkLoupe-$version-setup.exe"
    sha256  = (Get-FileHash $setup -Algorithm SHA256).Hash.ToLower()
    size    = $item.Length
    page    = "https://github.com/$repo/releases/tag/v$version"
}
$json = $manifest | ConvertTo-Json
[IO.File]::WriteAllText((Join-Path $dist "latest.json"), $json, (New-Object Text.UTF8Encoding $false))
Write-Host ("BlinkLoupe {0}: {1} ({2:N1} MB), sha256 {3}" -f $version, $setup, ($item.Length / 1MB), $manifest.sha256)

if ($Publish) {
    $title = "BlinkLoupe $version"
    $body = if ($Notes) { $Notes } else { $title }
    $ErrorActionPreference = "Continue"
    & gh release create "v$version" $setup (Join-Path $dist "latest.json") --repo $repo --title $title --notes $body
    $code = $LASTEXITCODE
    $ErrorActionPreference = "Stop"
    if ($code -ne 0) { throw "gh release create failed ($code)" }
    Write-Host "Published https://github.com/$repo/releases/tag/v$version"
}
