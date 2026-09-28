$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

# ---------------------------------------------------------------------------
# Resolve the real target directory.
#
# This script used to hard-code `target/` relative to the repo root. When
# CARGO_TARGET_DIR is set (it is on this machine:
# C:\rust-targets\photovault), every build writes somewhere else entirely, so
# the script reported "Before: 0" and exited without cleaning anything. The
# cache grew to 40 GB before anyone noticed.
#
# Cargo's own precedence is CARGO_TARGET_DIR, then `build.target-dir` in
# .cargo/config.toml, then `target/` next to the manifest.
# ---------------------------------------------------------------------------
function Resolve-TargetDir {
  if ($env:CARGO_TARGET_DIR) { return $env:CARGO_TARGET_DIR }

  $config = Join-Path (Get-Location) ".cargo\config.toml"
  if (Test-Path $config) {
    $match = Select-String -Path $config -Pattern '^\s*target-dir\s*=\s*"([^"]+)"' |
      Select-Object -First 1
    if ($match) { return $match.Matches[0].Groups[1].Value }
  }
  return (Join-Path (Get-Location) "target")
}

$TargetDir = Resolve-TargetDir

if (-not (Test-Path $TargetDir)) {
  Write-Host "Target directory does not exist: $TargetDir"
  exit 0
}

Write-Host "Target directory: $TargetDir"
$before = (Get-ChildItem $TargetDir -Recurse -File -ErrorAction SilentlyContinue |
  Measure-Object -Property Length -Sum).Sum
Write-Host "Before: $([math]::Round($before / 1GB, 2)) GB"

# --- Stale incremental caches ---------------------------------------------
# Purely a build-speed cache; regenerated on the next compile.
foreach ($profile in @("debug", "release")) {
  $inc = Join-Path $TargetDir "$profile\incremental"
  if (Test-Path $inc) {
    Remove-Item -Recurse -Force $inc -ErrorAction SilentlyContinue
    Write-Host "  removed $profile/incremental"
  }
}

# --- Superseded debug symbols ---------------------------------------------
# Rust writes one .pdb per crate per build, and each is 100-250 MB on this
# project. Every rebuild after a source change adds another set, and nothing
# ever removes the old ones. They are only needed to symbolise a crash in
# that exact binary, so keeping the newest per crate is enough and reclaims
# an order of magnitude more space than the incremental caches do.
foreach ($profile in @("debug", "release")) {
  $deps = Join-Path $TargetDir "$profile\deps"
  if (-not (Test-Path $deps)) { continue }

  $groups = Get-ChildItem $deps -File -Filter *.pdb -ErrorAction SilentlyContinue |
    Group-Object { $_.BaseName -replace '-[0-9a-f]{16}$', '' }

  $freed = 0
  foreach ($group in $groups) {
    $sorted = $group.Group | Sort-Object LastWriteTime -Descending
    if ($sorted.Count -le 1) { continue }
    $older = $sorted[1..($sorted.Count - 1)]
    $freed += ($older | Measure-Object Length -Sum).Sum
    $older | Remove-Item -Force -ErrorAction SilentlyContinue
  }
  if ($freed -gt 0) {
    Write-Host "  removed $([math]::Round($freed / 1GB, 2)) GB of superseded $profile symbols"
  }
}

# --- Old bundles: keep the most recent of each format ----------------------
$bundleDir = Join-Path $TargetDir "release\bundle"
if (Test-Path $bundleDir) {
  foreach ($fmt in @("deb", "rpm", "appimage", "msi", "dmg")) {
    $files = Get-ChildItem -Path $bundleDir -Filter "*.$fmt" -File -ErrorAction SilentlyContinue |
      Sort-Object LastWriteTime -Descending
    if ($files.Count -gt 1) {
      $files[1..($files.Count - 1)] | Remove-Item -Force -ErrorAction SilentlyContinue
    }
  }
}

# Build-script outputs can remain valid for months; age is not a stale-cache test.

$after = (Get-ChildItem $TargetDir -Recurse -File -ErrorAction SilentlyContinue |
  Measure-Object -Property Length -Sum).Sum
Write-Host "After:  $([math]::Round($after / 1GB, 2)) GB  (freed $([math]::Round(($before - $after) / 1GB, 2)) GB)"
