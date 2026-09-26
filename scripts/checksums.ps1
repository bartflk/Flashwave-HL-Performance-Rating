# SHA-256 for everything the last build produced.
#
# The installer is not code-signed, so testers are asked to tick "Unblock" to
# get past SmartScreen -- which is only a reasonable thing to ask if they can
# first check the file is the one that was built here. That is what this is
# for. Paste the output into the release notes; testers compare with
#
#   Get-FileHash .\Flashwave.tf_x.y.z_x64-setup.exe -Algorithm SHA256
#
# Run after `npm run build`:  powershell -File scripts/checksums.ps1

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$bundle = Join-Path $root "target\release\bundle"

if (-not (Test-Path $bundle)) {
    Write-Error 'No bundle directory. Run: npm run build'
}

# The version being released, so an old build lying in the folder is not
# quietly passed off as the current one.
$version = (Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
Write-Host "Flashwave.tf $version" -ForegroundColor Cyan

$files = Get-ChildItem $bundle -Recurse -Include *.exe, *.msi |
    Where-Object { $_.Name -like "*$version*" } |
    Sort-Object Name

if (-not $files) {
    Write-Error "Nothing in the bundle folder matches version $version - is the build current?"
}

foreach ($f in $files) {
    $hash = (Get-FileHash $f.FullName -Algorithm SHA256).Hash
    $mb = "{0:N2} MB" -f ($f.Length / 1MB)
    Write-Host ""
    Write-Host $f.Name -ForegroundColor White
    Write-Host "  $mb"
    Write-Host "  $hash"
}
Write-Host ""
