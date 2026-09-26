# Build a signed release and the manifest the updater reads.
#
# The in-app updater will not install anything whose signature does not
# match the public key compiled into the app, so a release built without
# the private key is a release nobody can update to. This script makes that
# hard to get wrong: it refuses to build without the key, and it writes
# latest.json from the real file it just signed rather than from anything
# typed by hand.
#
#   powershell -File scripts/release.ps1
#
# Then upload BOTH to the GitHub release:
#   Flashwave.tf_<version>_x64-setup.exe
#   latest.json
#
# The updater points at .../releases/latest/download/latest.json, which
# GitHub keeps aimed at the newest release, so publishing is all it takes.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$keyPath = Join-Path $env:USERPROFILE ".flashwave-keys\flashwave.key"

if (-not (Test-Path $keyPath)) {
    Write-Error "No signing key at $keyPath. Without it the build cannot be updated to. Restore it from your backup, or generate a new one and accept that everyone already on an old version has to reinstall by hand."
}
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $keyPath -Raw).Trim()
# The key has no password, but tauri still tries to decrypt it and will sit
# waiting on a prompt that a non-interactive build can never answer -- the
# build then finishes without signing anything and nothing says why. Saying
# "no password" out loud is what stops that.
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""

$version = (Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
Write-Host "Building Flashwave.tf $version (signed)" -ForegroundColor Cyan

Push-Location $root
try {
    & npm.cmd run build
    if ($LASTEXITCODE -ne 0) { Write-Error "build failed" }
} finally {
    Pop-Location
}

$nsis = Join-Path $root "target\release\bundle\nsis"
$setup = Get-ChildItem $nsis -Filter "*$version*-setup.exe" | Select-Object -First 1
$sig = Get-ChildItem $nsis -Filter "*$version*-setup.exe.sig" | Select-Object -First 1
if (-not $setup) { Write-Error "no installer for $version in $nsis" }
if (-not $sig) { Write-Error "no .sig beside the installer -- the build did not sign it, so the updater would reject it" }

# The notes shown in the update card: the release notes' first paragraph.
$notesFile = Join-Path $root "docs\release-$version.md"
$notes = if (Test-Path $notesFile) {
    ((Get-Content $notesFile) | Where-Object { $_ -notmatch '^\s*$' -and $_ -notmatch '^#' -and $_ -notmatch '^\s*```' } | Select-Object -First 3) -join " "
} else { "See the release page." }

$manifest = [ordered]@{
    version   = $version
    notes     = $notes
    pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = [ordered]@{
        "windows-x86_64" = [ordered]@{
            signature = (Get-Content $sig.FullName -Raw).Trim()
            url       = "https://github.com/bartflk/Flashwave-HL-Performance-Rating/releases/download/v$version/$($setup.Name)"
        }
    }
}
$out = Join-Path $nsis "latest.json"
$manifest | ConvertTo-Json -Depth 6 | Set-Content $out -Encoding utf8

Write-Host ""
Write-Host "signed installer : $($setup.FullName)" -ForegroundColor White
Write-Host "manifest         : $out" -ForegroundColor White
Write-Host ""
Write-Host "Upload both to the v$version release. The URL in the manifest must match" -ForegroundColor Yellow
Write-Host "the tag exactly, or the updater downloads a 404." -ForegroundColor Yellow
