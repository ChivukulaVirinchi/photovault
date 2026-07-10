# scripts/release.ps1
# One-command release: bump manifests, refresh Cargo.lock, commit,
# push, tag, push tag. The pushed tag triggers .github/workflows/release.yml
# which builds the cross-OS artifacts and the asset pack.
#
# Usage:
#   .\scripts\release.ps1 v0.2.0-rc.3
#   .\scripts\release.ps1 v0.2.0-rc.3 -DryRun
#   .\scripts\release.ps1 v0.2.0-rc.3 -NoTag
#   .\scripts\release.ps1 v0.2.0-rc.3 -AllowDirty
#   .\scripts\release.ps1 v0.2.0    (stable)
#
# See docs/RELEASE_CHECKLIST.md for the broader release workflow and
# pre-release flag handling.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Version,
    [switch]$DryRun,
    [switch]$NoTag,
    [switch]$AllowDirty,
    [string]$Branch = "master"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# --------------------------- helpers ---------------------------

function Say([string]$msg)  { Write-Host "==> $msg" -ForegroundColor Cyan }
function Note([string]$msg) { Write-Host "    $msg" -ForegroundColor DarkGray }
function Warn([string]$msg) { Write-Host "!!  $msg" -ForegroundColor Yellow }
function Die([string]$msg)  { Write-Host "x   $msg" -ForegroundColor Red; exit 1 }

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Die "Required command not found on PATH: $Name"
    }
}

# Normalise to (tagName, manifestVersion):
#   v0.2.0-rc.3  ->  ("v0.2.0-rc.3", "0.2.0-rc.3")
#   0.2.0-rc.3   ->  ("v0.2.0-rc.3", "0.2.0-rc.3")
function Normalize-Version([string]$raw) {
    if ([string]::IsNullOrWhiteSpace($raw)) { Die "Version cannot be empty." }
    $tag = if ($raw.StartsWith("v")) { $raw } else { "v$raw" }
    $manifest = $tag.Substring(1)
    # Strict-SemVer-ish: major.minor.patch with optional pre-release
    # (dots and dashes inside the suffix are fine).
    if ($tag -notmatch '^v\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$') {
        Die ("Version must be SemVer-shaped: vX.Y.Z or vX.Y.Z-suffix " +
             "(e.g. v0.2.0, v0.2.0-rc.3). Got: $raw")
    }
    return ,@($tag, $manifest)
}

# Replace the [package] version line in a Cargo.toml without touching
# the many dependency `version = "..."` entries. Returns $true if a
# change was made.
function Update-CargoTomlVersion([string]$path, [string]$newVersion) {
    if (-not (Test-Path $path)) { Die "File not found: $path" }
    $lines = Get-Content $path
    $inPackage = $false
    $changed = $false
    for ($i = 0; $i -lt $lines.Length; $i++) {
        $line = $lines[$i]
        if ($line -match '^\[package\]\s*$') { $inPackage = $true; continue }
        if ($inPackage -and $line -match '^\[') { break }   # next section ends [package]
        if ($inPackage -and $line -match '^version\s*=\s*"[^"]*"\s*$') {
            $new = ('version = "{0}"' -f $newVersion)
            if ($line -ne $new) {
                $lines[$i] = $new
                $changed = $true
            }
            break
        }
    }
    if ($changed -and -not $DryRun) {
        Set-Content -Path $path -Value $lines -NoNewline:$false
    }
    return $changed
}

# Replace the top-level "version" field in tauri.conf.json. The file
# has exactly one such field (sibling of "productName"); regex replace
# preserves formatting better than ConvertTo-Json round-trip.
function Update-TauriConfVersion([string]$path, [string]$newVersion) {
    if (-not (Test-Path $path)) { Die "File not found: $path" }
    $content = Get-Content $path -Raw
    $pattern = '("version"\s*:\s*)"[^"]+"'
    $replacement = '$1"' + $newVersion + '"'
    $updated = [regex]::Replace($content, $pattern, $replacement, 1)
    if ($updated -eq $content) {
        return $false
    }
    if (-not $DryRun) {
        Set-Content -Path $path -Value $updated -NoNewline
    }
    return $true
}

# ------------------------- preconditions -------------------------

$normalized = Normalize-Version $Version
$tagName = $normalized[0]
$manifestVersion = $normalized[1]

Say "Releasing $tagName (manifest version: $manifestVersion)"
if ($DryRun) { Warn "DRY RUN - no files written, no git operations" }

Require-Command git
Require-Command cargo

# Move to repo root so all paths are absolute-stable.
$repoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $repoRoot
Note "Repo root: $repoRoot"

# Branch + sync checks.
$current = (& git rev-parse --abbrev-ref HEAD).Trim()
if ($current -ne $Branch) {
    Die "Current branch is '$current'; expected '$Branch'. Switch with: git checkout $Branch"
}

if (-not $AllowDirty) {
    $dirty = (& git status --porcelain)
    if ($dirty) {
        Warn "Working tree is dirty:"
        $dirty | ForEach-Object { Note $_ }
        Die "Commit or stash before releasing. Use -AllowDirty to override."
    }
}

# Fetch + check we're up-to-date with origin.
Say "Fetching origin..."
& git fetch origin $Branch --tags
if ($LASTEXITCODE -ne 0) { Die "git fetch failed" }

$localHead  = (& git rev-parse HEAD).Trim()
$remoteHead = (& git rev-parse "origin/$Branch").Trim()
if ($localHead -ne $remoteHead) {
    Die "Local $Branch ($($localHead.Substring(0,7))) is not in sync with origin/$Branch ($($remoteHead.Substring(0,7))). Pull or push first."
}

# Tag must not already exist (local or remote).
$existsLocal = (& git rev-parse -q --verify "refs/tags/$tagName" 2>$null)
if ($existsLocal) { Die "Tag already exists locally: $tagName" }

& git ls-remote --exit-code --tags origin "refs/tags/$tagName" *> $null
if ($LASTEXITCODE -eq 0) { Die "Tag already exists on origin: $tagName" }

# --------------------------- bump ---------------------------

Say "Bumping Cargo.toml -> $manifestVersion"
$cargoChanged = Update-CargoTomlVersion "Cargo.toml" $manifestVersion
if ($cargoChanged) { Note "Updated" } else { Warn "Cargo.toml [package] version was already $manifestVersion" }

Say "Bumping src-tauri/tauri.conf.json -> $manifestVersion"
$tauriChanged = Update-TauriConfVersion "src-tauri/tauri.conf.json" $manifestVersion
if ($tauriChanged) { Note "Updated" } else { Warn "tauri.conf.json version was already $manifestVersion" }

if (-not ($cargoChanged -or $tauriChanged)) {
    Die "Nothing to bump; both files already at $manifestVersion. Tagging the existing commit? Use scripts/release_publish.ps1 instead."
}

Say "Refreshing Cargo.lock"
if (-not $DryRun) {
    & cargo update -p smriti --quiet
    if ($LASTEXITCODE -ne 0) { Die "cargo update -p smriti failed" }
}

# --------------------------- diff preview ---------------------------

Say "Diff to commit:"
if (-not $DryRun) {
    & git --no-pager diff -- Cargo.toml Cargo.lock src-tauri/tauri.conf.json
}

Write-Host ""
Write-Host "About to:"
Write-Host "  1) git commit -m 'chore(release): $tagName'"
Write-Host "  2) git push origin $Branch"
if (-not $NoTag) {
    Write-Host "  3) git tag -a $tagName -m 'Smriti $tagName'"
    Write-Host "  4) git push origin $tagName"
}
Write-Host ""

if ($DryRun) {
    Say "DRY RUN complete; nothing changed."
    exit 0
}

$confirm = Read-Host "Proceed? [y/N]"
if ($confirm -notmatch '^[Yy]') {
    Warn "Cancelled. Reverting uncommitted changes."
    & git checkout -- Cargo.toml Cargo.lock src-tauri/tauri.conf.json
    exit 1
}

# --------------------------- commit + push ---------------------------

Say "Committing"
& git add Cargo.toml Cargo.lock src-tauri/tauri.conf.json
if ($LASTEXITCODE -ne 0) { Die "git add failed" }
& git commit -m "chore(release): $tagName"
if ($LASTEXITCODE -ne 0) { Die "git commit failed" }

Say "Pushing to origin/$Branch"
& git push origin $Branch
if ($LASTEXITCODE -ne 0) { Die "git push failed" }

if ($NoTag) {
    Say "Done. Skipping tag (per -NoTag)."
    exit 0
}

# --------------------------- tag + push tag ---------------------------

Say "Tagging $tagName"
& git tag -a $tagName -m "Smriti $tagName"
if ($LASTEXITCODE -ne 0) { Die "git tag failed" }

Say "Pushing tag"
& git push origin $tagName
if ($LASTEXITCODE -ne 0) {
    Warn "Tag push failed. Cleaning up local tag."
    & git tag -d $tagName 2>$null | Out-Null
    Die "git push origin $tagName failed"
}

# --------------------------- done ---------------------------

# Best-effort remote URL -> GitHub Actions URL.
$remoteUrl = (& git config --get remote.origin.url).Trim()
$ghPath = $null
if ($remoteUrl -match 'github\.com[:/](.+?)(\.git)?$') {
    $ghPath = $Matches[1]
}

Write-Host ""
Say "Released $tagName"
Note "Workflow:  https://github.com/$ghPath/actions/workflows/release.yml"
Note "Releases:  https://github.com/$ghPath/releases"
if ($tagName -match '-(rc|beta|alpha)') {
    Write-Host ""
    Warn "PRE-RELEASE - after the workflow finishes, manually toggle"
    Warn "'Set as a pre-release' on the draft. release.yml hardcodes"
    Warn "prerelease: false so /releases/latest/ continues to point at"
    Warn "the previous stable until you uncheck this manually."
}
