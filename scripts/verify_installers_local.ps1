$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RootDir = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $RootDir

$results = [System.Collections.Generic.List[object]]::new()

function Add-Result([string]$Name, [string]$Status, [string]$Info = "") {
    $results.Add([PSCustomObject]@{ Name = $Name; Status = $Status; Info = $Info }) | Out-Null
}

function Invoke-Native([scriptblock]$Command) {
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE"
    }
}

function Invoke-Step([string]$Name, [scriptblock]$Command) {
    try {
        & $Command
        Add-Result $Name "PASS"
    } catch {
        Add-Result $Name "FAIL" $_.Exception.Message
    }
}

Write-Host "Smriti local installer verification"
Write-Host "====================================="

Invoke-Step "frontend" {
    Invoke-Native { npm ci --prefix src-ui }
    Invoke-Native { npm run build --prefix src-ui }
}

Invoke-Step "Tauri MSI" {
    Push-Location src-tauri
    try {
        Invoke-Native { cargo tauri build --ci --bundles msi }
    } finally {
        Pop-Location
    }
    $msi = Get-ChildItem target\release\bundle\msi\*.msi -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $msi) { throw "Tauri did not produce target\release\bundle\msi\*.msi" }
}

Invoke-Step "setup assets" {
    Invoke-Native { powershell -ExecutionPolicy Bypass -File scripts\setup_assets.ps1 }
}

Invoke-Step "asset pack" {
    $stage = Join-Path $RootDir "assets-pack-local"
    $zip = Join-Path $RootDir "Smriti-Assets-local.zip"
    if (Test-Path $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
    if (Test-Path $zip) { Remove-Item -LiteralPath $zip -Force }

    New-Item -ItemType Directory -Path "$stage\libs\onnxruntime\windows" -Force | Out-Null
    New-Item -ItemType Directory -Path "$stage\models" -Force | Out-Null
    New-Item -ItemType Directory -Path "$stage\data" -Force | Out-Null
    Copy-Item "libs\onnxruntime\onnxruntime.dll" "$stage\libs\onnxruntime\windows\onnxruntime.dll" -Force
    Copy-Item "models\scrfd_10g_bnkps.onnx" "$stage\models\scrfd_10g_bnkps.onnx" -Force
    Copy-Item "models\adaface_ir101_webface12m.onnx" "$stage\models\adaface_ir101_webface12m.onnx" -Force
    Copy-Item "data\geonames.db" "$stage\data\geonames.db" -Force
    Compress-Archive -Path "$stage\*" -DestinationPath $zip -Force
    if (-not (Test-Path $zip) -or (Get-Item $zip).Length -lt 1MB) {
        throw "Asset pack was not created or is unexpectedly small"
    }
}

Write-Host ""
$results | Format-Table -AutoSize

if ($results.Status -contains "FAIL") {
    Write-Host "Overall: FAIL"
    exit 1
}

Write-Host "Overall: PASS"
