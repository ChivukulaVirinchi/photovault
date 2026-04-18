# Windows Handoff (MSI + local verification)

Use this file from a Windows PowerShell session in the same repo.

## Ground rules

- Do **not** commit.
- Do **not** push.
- Work only in local workspace and verify packaging/install flow.

## Current situation

- Repo has large uncommitted changes (asset-pack split, startup install prompt, website/docs/workflow updates).
- MSI generation from Windows now works only with explicit WiX v3 tool path and explicit target bin dir.
- Building from `\\wsl.localhost\...` UNC works for `cargo build`, but `light.exe` (WiX linker) can fail with `LGHT0001` on UNC-backed paths.
- Build artifacts under UNC/WSL path can look inconsistent if you check the wrong `target` folder.

## Validated status (2026-04-18)

- `cargo build --release --target x86_64-pc-windows-msvc --target-dir .\\target-win` succeeded from Windows PowerShell on UNC repo path.
- MSI build succeeded after moving WiX packaging step to a local NTFS staging folder and using `--target-bin-dir`.
- MSI was produced and copied back to `target\\wix\\PhotoVault-Setup-x64.msi` in the repo.
- User install flow works as expected when launching MSI from a local path (for example Downloads) with UAC elevation.
- Silent install from a non-elevated shell fails with `Error 1925` (expected for per-machine install).

## New polish updates (2026-04-18)

- MSI UI upgraded from minimal flow to full installer UI (`WixUI_InstallDir`) so users see the product name and standard setup pages.
- Added branded installer visuals:
  - `packaging/wix-banner.bmp`
  - `packaging/wix-dialog.bmp`
- Added installer license page input:
  - `packaging/wix-license.rtf` (generated from project `LICENSE`)
- `wix/main.wxs` now sets WiX UI variables (`WixUIBannerBmp`, `WixUIDialogBmp`, `WixUILicenseRtf`) and `WIXUI_INSTALLDIR`.

This should address the previous issues where setup looked generic and did not visibly present PhotoVault branding.

## Primary objective

Verify locally on Windows, end-to-end:

1. Core app builds and runs.
2. MSI builds.
3. MSI installs/uninstalls.
4. Installed app launches.
5. Optional assets flow is usable.

---

## Exact commands (PowerShell)

Run from repo root:

```powershell
cd \\wsl.localhost\Ubuntu-24.04\home\virinchi\code\rust\photovault
```

### 1) Verify Windows toolchain identity

```powershell
where.exe cargo
cargo -Vv
rustc -Vv
```

Expected host: `x86_64-pc-windows-msvc`

### 2) Build core app to dedicated Windows target folder

```powershell
$TargetDir = ".\target-win"
cargo build --release --target x86_64-pc-windows-msvc --target-dir $TargetDir

$exe = Join-Path $TargetDir "x86_64-pc-windows-msvc\release\photovault.exe"
if (-not (Test-Path $exe)) {
  $exe = (Get-ChildItem -Recurse -Filter photovault.exe $TargetDir | Select-Object -First 1).FullName
}

$exe
Test-Path $exe
```

Expected: `True` for exe path.

### 3) Run core app

```powershell
& $exe
```

Expected:

- Window launches.
- If optional assets are missing, startup prompt appears.

### 4) Ensure WiX v3 binaries exist (required by cargo-wix)

```powershell
$wixBin = "C:\tools\wix311"
if (-not (Test-Path "$wixBin\candle.exe")) {
  $wixZip = "$env:TEMP\wix311-binaries.zip"
  Invoke-WebRequest "https://github.com/wixtoolset/wix3/releases/download/wix3112rtm/wix311-binaries.zip" -OutFile $wixZip
  if (Test-Path $wixBin) { Remove-Item $wixBin -Recurse -Force }
  New-Item -ItemType Directory -Path $wixBin | Out-Null
  Expand-Archive -Path $wixZip -DestinationPath $wixBin -Force
}

Test-Path "$wixBin\candle.exe"
Test-Path "$wixBin\light.exe"
```

Both must be `True`.

### 5) Build MSI (no rebuild of binary)

Important: run WiX packaging from a local NTFS folder, not directly from UNC repo path.

```powershell
cargo install cargo-wix --locked

$RepoUNC = "\\wsl.localhost\Ubuntu-24.04\home\virinchi\code\rust\photovault"
$LocalRepo = Join-Path $env:TEMP "photovault-localwix"

if (Test-Path $LocalRepo) { Remove-Item $LocalRepo -Recurse -Force }
New-Item -ItemType Directory -Path $LocalRepo | Out-Null

# Copy only packaging-relevant files; exclude heavy/generated dirs
robocopy $RepoUNC $LocalRepo /MIR /XD .git target target-win .github website .cache libs models data /NFL /NDL /NJH /NJS /NP | Out-Null

# Reuse already built EXE from UNC build output
$srcExe = Join-Path $RepoUNC "target-win\x86_64-pc-windows-msvc\release\photovault.exe"
$dstBinDir = Join-Path $LocalRepo "target-win\x86_64-pc-windows-msvc\release"
New-Item -ItemType Directory -Path $dstBinDir -Force | Out-Null
Copy-Item $srcExe (Join-Path $dstBinDir "photovault.exe") -Force

Set-Location $LocalRepo

cargo wix --no-build --target x86_64-pc-windows-msvc --target-bin-dir "$dstBinDir" --bin-path "$wixBin" --output target\wix\PhotoVault-Setup-x64.msi

# Copy MSI back to repo target folder
$repoWix = Join-Path $RepoUNC "target\wix"
New-Item -ItemType Directory -Path $repoWix -Force | Out-Null
Copy-Item .\target\wix\PhotoVault-Setup-x64.msi (Join-Path $repoWix "PhotoVault-Setup-x64.msi") -Force

Test-Path (Join-Path $repoWix "PhotoVault-Setup-x64.msi")
Get-Item (Join-Path $repoWix "PhotoVault-Setup-x64.msi")
```

Expected: MSI exists.

### 6) Install (real user flow) + verify + launch

```powershell
$msiUNC = "\\wsl.localhost\Ubuntu-24.04\home\virinchi\code\rust\photovault\target\wix\PhotoVault-Setup-x64.msi"
$msiLocal = Join-Path $env:USERPROFILE "Downloads\PhotoVault-Setup-x64.msi"
Copy-Item $msiUNC $msiLocal -Force

# Optional: remove Mark-of-the-Web if present
Unblock-File $msiLocal -ErrorAction SilentlyContinue

# Open local folder and double-click MSI in Explorer (recommended user test)
explorer.exe /select,$msiLocal

Test-Path "$env:ProgramFiles\PhotoVault\photovault.exe"
& "$env:ProgramFiles\PhotoVault\photovault.exe"
```

Expected:

- Installer launches from double-click.
- UAC prompt appears (per-machine install).
- Installed exe path is `True`.
- App launches from installed path.

### 7) Validate Start Menu + uninstall entry

```powershell
Test-Path "$env:ProgramData\Microsoft\Windows\Start Menu\Programs\PhotoVault\PhotoVault.lnk"

Get-ChildItem "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall" |
  ForEach-Object { Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue } |
  Where-Object { $_.DisplayName -eq "PhotoVault" } |
  Select-Object DisplayName, DisplayVersion, Publisher
```

### 8) Uninstall + verify cleanup (user flow)

Recommended user test:

- Open `Settings -> Apps -> Installed apps`.
- Uninstall `PhotoVault`.

CLI equivalent:

```powershell
Start-Process msiexec.exe -ArgumentList "/x `"$msiLocal`" /qn /norestart" -Wait -PassThru
Test-Path "$env:ProgramFiles\PhotoVault\photovault.exe"
Test-Path "$env:ProgramData\Microsoft\Windows\Start Menu\Programs\PhotoVault\PhotoVault.lnk"
```

Expected final value: `False`.

---

## If anything fails, gather this debug bundle

```powershell
Get-Location
where.exe cargo
cargo -Vv
rustc -Vv

Get-ChildItem -Recurse -Filter photovault.exe .\target, .\target-win -ErrorAction SilentlyContinue | Select-Object FullName,Length,LastWriteTime
Get-ChildItem -Recurse -Filter PhotoVault-Setup-x64.msi .\target, .\target-win -ErrorAction SilentlyContinue | Select-Object FullName,Length,LastWriteTime

Test-Path "C:\tools\wix311\candle.exe"
Test-Path "C:\tools\wix311\light.exe"
```

Share this output back to the assistant in Windows session.

---

## Notes for the Windows assistant session

- Prefer `target-win` for deterministic output paths.
- Do not use `cargo clean` unless absolutely required.
- Never assume MSI missing until recursive search confirms.
- No commits or pushes.
