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

param(
    # Write the manifest for a build already made, without building again.
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$keyPath = Join-Path $env:USERPROFILE ".flashwave-keys\flashwave.key"

if (-not (Test-Path $keyPath)) {
    Write-Error "No signing key at $keyPath. Without it the build cannot be updated to. Restore it from your backup, or generate a new one and accept that everyone already on an old version has to reinstall by hand."
}
$key = (Get-Content $keyPath -Raw).Trim()

$version = (Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
Write-Host "Building Flashwave.tf $version (signed)" -ForegroundColor Cyan

# The league snapshot inside the app must be for this build's rating model,
# or new installs get no player lookup at all (docs/releasing.md, step 2b).
if (-not $SkipBuild) {
    Push-Location $root
    & cargo run -q -p hl-cli -- snapshot --check
    $checked = $LASTEXITCODE
    Pop-Location
    if ($checked -ne 0) { Write-Error "The league snapshot is out of date. Make a new one (docs/releasing.md, step 2b), then run this again." }
}

# The key has no password, but tauri still tries to decrypt it, and without
# being told "no password" it sits on a prompt a non-interactive build can
# never answer. Telling it means an environment variable that exists and is
# empty -- which `$env:X = ""` cannot make: in Windows PowerShell that
# *deletes* the variable. It hung the 0.5.0 build for five minutes with
# nothing on screen. ProcessStartInfo keeps an empty value, so the build is
# started through it.
if (-not $SkipBuild) {
    # By its full path: started by bare name, npm.cmd works out its own
    # folder as the working directory and looks for npm there (0.6.0).
    $npm = (Get-Command npm.cmd -ErrorAction Stop).Source
    $psi = New-Object System.Diagnostics.ProcessStartInfo $npm, "run build"
    $psi.WorkingDirectory = $root
    $psi.UseShellExecute = $false
    $psi.EnvironmentVariables["TAURI_SIGNING_PRIVATE_KEY"] = $key
    $psi.EnvironmentVariables["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = ""
    $build = [System.Diagnostics.Process]::Start($psi)
    $build.WaitForExit()
    if ($build.ExitCode -ne 0) { Write-Error "build failed" }
}

$nsis = Join-Path $root "target\release\bundle\nsis"
$setup = Get-ChildItem $nsis -Filter "*$version*-setup.exe" | Select-Object -First 1
$sig = Get-ChildItem $nsis -Filter "*$version*-setup.exe.sig" | Select-Object -First 1
if (-not $setup) { Write-Error "no installer for $version in $nsis" }
if (-not $sig) { Write-Error "no .sig beside the installer -- the build did not sign it, so the updater would reject it" }

# The notes shown in the update card: the release notes' first paragraph.
$notesFile = Join-Path $root "docs\release-$version.md"
# The summary is the first paragraph after the `---` that ends the install
# instructions; without a rule, the first paragraph that is not a heading.
# Read as UTF-8: Windows PowerShell otherwise reads the file in the ANSI
# code page and a "›" arrives as "â€º".
$notes = "See the release page."
if (Test-Path $notesFile) {
    $text = (Get-Content $notesFile -Raw -Encoding UTF8) -replace "`r`n", "`n"
    $rule = [regex]::Match($text, "(?m)^---\s*$")
    if ($rule.Success) { $text = $text.Substring($rule.Index + $rule.Length) }
    $para = ($text -split "`n`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -and $_ -notmatch '^(#|```)' } | Select-Object -First 1)
    if ($para) { $notes = ($para -replace "`n", " ") }
}

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
# Without a byte-order mark. `Set-Content -Encoding utf8` in Windows
# PowerShell writes one, and a JSON reader that meets it fails -- the updater
# then finds nothing, silently, for everyone. 0.4.1's manifest had none; the
# first 0.5.0 build's did, and was caught before upload.
[System.IO.File]::WriteAllText($out, ($manifest | ConvertTo-Json -Depth 6), (New-Object System.Text.UTF8Encoding $false))

Write-Host ""
Write-Host "signed installer : $($setup.FullName)" -ForegroundColor White
Write-Host "manifest         : $out" -ForegroundColor White
Write-Host ""
Write-Host "Upload both to the v$version release. The URL in the manifest must match" -ForegroundColor Yellow
Write-Host "the tag exactly, or the updater downloads a 404." -ForegroundColor Yellow

# After a release, the side worktrees merged into it and the test copies of
# the database are done with (scripts/tidy.mjs says what it keeps and why).
Write-Host ""
Write-Host "Tidying worktrees and scratch copies" -ForegroundColor Cyan
Push-Location $root
& node scripts/tidy.mjs
Pop-Location
