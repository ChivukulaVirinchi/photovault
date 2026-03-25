# PhotoVault asset setup for Windows
# Downloads ONNX Runtime (Windows), face models, and GeoNames data.
# Idempotent: skips files that already exist.

$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$DataDir = Join-Path $RootDir "data"
$ModelsDir = Join-Path $RootDir "models"
$CacheDir = Join-Path $RootDir ".cache\downloads"

$GeonamesCitiesUrl = "https://download.geonames.org/export/dump/cities1000.zip"
$GeonamesCountryUrl = "https://download.geonames.org/export/dump/countryInfo.txt"
$ScrfdModelUrl = if ($env:SCRFD_MODEL_URL) { $env:SCRFD_MODEL_URL } else { "https://huggingface.co/MonsterMMORPG/tools/resolve/main/scrfd_10g_bnkps.onnx" }
$GlintrModelUrl = if ($env:GLINTR_MODEL_URL) { $env:GLINTR_MODEL_URL } else { "https://huggingface.co/MonsterMMORPG/tools/resolve/main/glintr100.onnx" }
$OrtUrl = if ($env:ORT_URL) { $env:ORT_URL } else { "https://github.com/microsoft/onnxruntime/releases/download/v1.23.0/onnxruntime-win-x64-1.23.0.zip" }

# Create directories
foreach ($dir in @($DataDir, $ModelsDir, $CacheDir)) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}

function Download-File {
    param([string]$Url, [string]$OutPath)
    if (Test-Path $OutPath) {
        if ((Get-Item $OutPath).Length -gt 0) {
            Write-Host "Using cached: $OutPath"
            return
        }
    }
    Write-Host "Downloading: $Url"
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri $Url -OutFile $OutPath -UseBasicParsing
}

# --- GeoNames ---
Write-Host "`n==> Setting up GeoNames data"

$CitiesZip = Join-Path $CacheDir "cities1000.zip"
$CountryInfo = Join-Path $CacheDir "countryInfo.txt"

Download-File $GeonamesCitiesUrl $CitiesZip
Download-File $GeonamesCountryUrl $CountryInfo

$CitiesTxt = Join-Path $DataDir "cities1000.txt"
if (-not (Test-Path $CitiesTxt)) {
    Expand-Archive -Path $CitiesZip -DestinationPath $DataDir -Force
}

if (-not (Test-Path $CitiesTxt)) {
    Write-Error "cities1000.txt missing after extraction"
    exit 1
}

# Extract country codes (skip comments, take ISO code and name)
$CountryCodes = Join-Path $DataDir "country_codes.txt"
if (-not (Test-Path $CountryCodes) -or (Get-Item $CountryCodes).Length -eq 0) {
    $lines = Get-Content $CountryInfo | Where-Object { $_ -notmatch "^#" -and $_.Trim() -ne "" }
    $output = foreach ($line in $lines) {
        $fields = $line -split "`t"
        if ($fields.Count -ge 5) {
            "$($fields[0])`t$($fields[4])"
        }
    }
    $output | Set-Content -Path $CountryCodes -Encoding UTF8
}

# Build GeoNames DB
$GeonamesDb = Join-Path $DataDir "geonames.db"
if (-not (Test-Path $GeonamesDb) -or (Get-Item $GeonamesDb).Length -eq 0) {
    Write-Host "Building geonames SQLite DB..."
    Push-Location $RootDir
    cargo run --bin build_geonames
    Pop-Location
} else {
    Write-Host "GeoNames DB already ready: $GeonamesDb"
}

# --- ONNX Runtime ---
Write-Host "`n==> Setting up ONNX Runtime"

$LibsDir = Join-Path $RootDir "libs\onnxruntime"
if (-not (Test-Path $LibsDir)) { New-Item -ItemType Directory -Path $LibsDir -Force | Out-Null }

$OrtDll = Join-Path $LibsDir "onnxruntime.dll"
if (Test-Path $OrtDll) {
    Write-Host "Using existing ONNX Runtime in $LibsDir"
} else {
    $OrtZip = Join-Path $CacheDir "onnxruntime-win-x64.zip"
    Download-File $OrtUrl $OrtZip

    $ExtractDir = Join-Path $CacheDir "onnxruntime-win-x64"
    if (Test-Path $ExtractDir) { Remove-Item -Recurse -Force $ExtractDir }
    Expand-Archive -Path $OrtZip -DestinationPath $ExtractDir -Force

    $Found = Get-ChildItem -Path $ExtractDir -Recurse -Filter "onnxruntime.dll" | Select-Object -First 1
    if (-not $Found) {
        Write-Error "onnxruntime.dll not found in extracted archive"
        exit 1
    }
    Copy-Item $Found.FullName $OrtDll
    Write-Host "Installed ONNX Runtime: $OrtDll"
}

# --- Face Models ---
Write-Host "`n==> Setting up face models"

Download-File $ScrfdModelUrl (Join-Path $ModelsDir "scrfd_10g_bnkps.onnx")
Download-File $GlintrModelUrl (Join-Path $ModelsDir "glintr100.onnx")

Write-Host "Installed models:"
Write-Host "- $(Join-Path $ModelsDir 'scrfd_10g_bnkps.onnx')"
Write-Host "- $(Join-Path $ModelsDir 'glintr100.onnx')"

Write-Host "`nDone."
