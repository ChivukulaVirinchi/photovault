$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

if (Test-Path target) {
  $before = (Get-ChildItem target -Recurse | Measure-Object -Property Length -Sum).Sum
  Write-Host "Before: $([math]::Round($before / 1GB, 1)) GB"
} else {
  Write-Host "Before: 0"
  exit 0
}

# Stale incremental caches
if (Test-Path target/debug/incremental) { Remove-Item -Recurse -Force target/debug/incremental -ErrorAction SilentlyContinue }
if (Test-Path target/release/incremental) { Remove-Item -Recurse -Force target/release/incremental -ErrorAction SilentlyContinue }

# Old bundles: keep the most recent of each format
if (Test-Path target/release/bundle) {
  foreach ($fmt in @("deb", "rpm", "appimage", "msi", "dmg")) {
    $files = Get-ChildItem -Path target/release/bundle -Filter "*.$fmt" -File -ErrorAction SilentlyContinue |
      Sort-Object LastWriteTime -Descending
    if ($files.Count -gt 1) {
      $files[1..($files.Count - 1)] | Remove-Item -Force -ErrorAction SilentlyContinue
    }
  }
}

# Build-script outputs can remain valid for months; age is not a stale-cache test.

if (Test-Path target) {
  $after = (Get-ChildItem target -Recurse | Measure-Object -Property Length -Sum).Sum
  Write-Host "After:  $([math]::Round($after / 1GB, 1)) GB"
}
