$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $RootDir

$results = @()

function Add-Result([string]$Name, [bool]$Ok, [string]$Info = "") {
    $results += [PSCustomObject]@{ Name = $Name; Status = if ($Ok) { "PASS" } else { "FAIL" }; Info = $Info }
}

Write-Host "Smriti local installer verification"
Write-Host "====================================="

if (Test-Path "staging") {
    Remove-Item -Recurse -Force "staging"
}

try {
    cargo build --release | Out-Null
    Add-Result "core build" $true
} catch {
    Add-Result "core build" $false $_.Exception.Message
}

try {
    powershell -ExecutionPolicy Bypass -File scripts\setup_assets.ps1 | Out-Null
    Add-Result "setup assets" $true
} catch {
    Add-Result "setup assets" $false $_.Exception.Message
}

try {
    cargo build --release --target x86_64-pc-windows-msvc | Out-Null
    if (Test-Path "smriti-x86_64-pc-windows-msvc.zip") { Remove-Item "smriti-x86_64-pc-windows-msvc.zip" -Force }
    New-Item -ItemType Directory -Path staging\smriti -Force | Out-Null
    Copy-Item target\x86_64-pc-windows-msvc\release\smriti.exe staging\smriti\smriti.exe -Force
    Compress-Archive -Path staging\smriti -DestinationPath smriti-x86_64-pc-windows-msvc.zip -Force
    Add-Result "windows zip" (Test-Path "smriti-x86_64-pc-windows-msvc.zip") "smriti-x86_64-pc-windows-msvc.zip"
} catch {
    Add-Result "windows zip" $false $_.Exception.Message
}

try {
    if (-not (Get-Command candle.exe -ErrorAction SilentlyContinue)) {
        choco install wixtoolset -y --no-progress | Out-Null
    }
    cargo install cargo-wix --locked | Out-Null
    cargo wix --target x86_64-pc-windows-msvc --output target\wix\Smriti-Setup-x64.msi | Out-Null
    $ok = Test-Path "target\wix\Smriti-Setup-x64.msi"
    Add-Result "windows msi" $ok "target\\wix\\Smriti-Setup-x64.msi"
} catch {
    Add-Result "windows msi" $false $_.Exception.Message
}

try {
    New-Item -ItemType Directory -Path assets-pack-local\libs\onnxruntime -Force | Out-Null
    New-Item -ItemType Directory -Path assets-pack-local\models -Force | Out-Null
    New-Item -ItemType Directory -Path assets-pack-local\data -Force | Out-Null

    Copy-Item libs\onnxruntime\onnxruntime.dll assets-pack-local\libs\onnxruntime\onnxruntime.dll -Force
    Copy-Item models\scrfd_10g_bnkps.onnx assets-pack-local\models\scrfd_10g_bnkps.onnx -Force
    Copy-Item models\adaface_ir101_webface12m.onnx assets-pack-local\models\adaface_ir101_webface12m.onnx -Force
    Copy-Item data\geonames.db assets-pack-local\data\geonames.db -Force

    if (Test-Path "Smriti-Assets-local.zip") { Remove-Item "Smriti-Assets-local.zip" -Force }
    Compress-Archive -Path assets-pack-local\* -DestinationPath Smriti-Assets-local.zip -Force
    Add-Result "assets pack" (Test-Path "Smriti-Assets-local.zip") "Smriti-Assets-local.zip"
} catch {
    Add-Result "assets pack" $false $_.Exception.Message
}

"" | Write-Host
$results | Format-Table -AutoSize

if ($results.Status -contains "FAIL") {
    Write-Host "Overall: FAIL"
    exit 1
}

Write-Host "Overall: PASS"
