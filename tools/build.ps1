# Builds BlinkLoupe (release) and copies it to the Desktop as BlinkLoupe.exe. Close a running BlinkLoupe first.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location (Join-Path $root "src-tauri")
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}
$exe = Join-Path $root "src-tauri\target\release\blinkloupe.exe"
$dest = Join-Path ([Environment]::GetFolderPath("Desktop")) "BlinkLoupe.exe"
if (Get-Process blinkloupe -ErrorAction SilentlyContinue) {
    Write-Host "BlinkLoupe is running - close it, then copy $exe to $dest"
    exit 1
}
Copy-Item $exe $dest -Force
Write-Host ("Built {0:N1} MB -> {1}" -f ((Get-Item $exe).Length / 1MB), $dest)
