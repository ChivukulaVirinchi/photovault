# Smriti asset setup for Windows
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
$AdafaceModelUrl = if ($env:ADAFACE_MODEL_URL) { $env:ADAFACE_MODEL_URL } else { "https://drive.usercontent.google.com/download?id=1dgMFOASKnaujQcCL4sSYkKOkBrmXUUU1&export=download&confirm=t" }
$OrtUrl = if ($env:ORT_URL) { $env:ORT_URL } else { "https://github.com/microsoft/onnxruntime/releases/download/v1.23.0/onnxruntime-win-x64-1.23.0.zip" }

# Create directories
foreach ($dir in @($DataDir, $ModelsDir, $CacheDir)) {
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
}

function Download-File {
    param([string]$Url, [string]$OutPath, [long]$MinimumBytes = 1024)
    if (Test-Path $OutPath) {
        if ((Get-Item $OutPath).Length -ge $MinimumBytes) {
            Write-Host "Using cached: $OutPath"
            return
        }
        Remove-Item -LiteralPath $OutPath -Force
    }
    Write-Host "Downloading: $Url"
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri $Url -OutFile $OutPath -UseBasicParsing
    if (-not (Test-Path $OutPath) -or (Get-Item $OutPath).Length -lt $MinimumBytes) {
        Remove-Item -LiteralPath $OutPath -Force -ErrorAction SilentlyContinue
        throw "Downloaded file is unexpectedly small: $Url"
    }
}

function Test-GeonamesDbReady {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return $false }
    # A fully-populated cities1000 database is ~18 MB. Also inspect the
    # first SQLite pages for the v2 column so an old schema is rebuilt.
    if ((Get-Item $Path).Length -le 1MB) { return $false }
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $buffer = New-Object byte[] 65536
        $read = $stream.Read($buffer, 0, $buffer.Length)
        $header = [System.Text.Encoding]::ASCII.GetString($buffer, 0, $read)
        return $header.Contains("feature_code")
    } finally {
        $stream.Dispose()
    }
}

function Expand-ZipWithRetry {
    param(
        [string]$Url,
        [string]$ZipPath,
        [string]$Destination,
        [long]$MinimumBytes = 1024
    )
    try {
        Expand-Archive -LiteralPath $ZipPath -DestinationPath $Destination -Force
    } catch {
        Write-Warning "Cached archive is invalid; downloading it again."
        Remove-Item -LiteralPath $ZipPath -Force -ErrorAction SilentlyContinue
        Download-File $Url $ZipPath $MinimumBytes
        Expand-Archive -LiteralPath $ZipPath -DestinationPath $Destination -Force
    }
}

# --- GeoNames ---
Write-Host "`n==> Setting up GeoNames data"

$CitiesZip = Join-Path $CacheDir "cities1000.zip"
$CountryInfo = Join-Path $CacheDir "countryInfo.txt"

Download-File $GeonamesCitiesUrl $CitiesZip 1MB
Download-File $GeonamesCountryUrl $CountryInfo 10KB

$CitiesTxt = Join-Path $DataDir "cities1000.txt"
Expand-ZipWithRetry $GeonamesCitiesUrl $CitiesZip $DataDir 1MB

if (-not (Test-Path $CitiesTxt)) {
    Write-Error "cities1000.txt missing after extraction"
    exit 1
}

# Extract country codes (skip comments, take ISO code and name)
$CountryCodes = Join-Path $DataDir "country_codes.txt"
$lines = Get-Content $CountryInfo | Where-Object { $_ -notmatch "^#" -and $_.Trim() -ne "" }
$output = foreach ($line in $lines) {
    $fields = $line -split "`t"
    if ($fields.Count -ge 5) {
        "$($fields[0])`t$($fields[4])"
    }
}
$output | Set-Content -Path $CountryCodes -Encoding UTF8
if (-not (Test-Path $CountryCodes) -or (Get-Item $CountryCodes).Length -eq 0) {
    throw "country_codes.txt is empty after processing countryInfo.txt"
}

# Build GeoNames DB
$GeonamesDb = Join-Path $DataDir "geonames.db"
if (-not (Test-GeonamesDbReady $GeonamesDb)) {
    Write-Host "Building geonames SQLite DB..."
    Push-Location $RootDir
    try {
        cargo run --bin build_geonames
        if ($LASTEXITCODE -ne 0) { throw "GeoNames database build failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
    if (-not (Test-GeonamesDbReady $GeonamesDb)) {
        throw "GeoNames database is missing or invalid after the build"
    }
} else {
    Write-Host "GeoNames DB already ready: $GeonamesDb"
}

# --- ONNX Runtime ---
Write-Host "`n==> Setting up ONNX Runtime"

$LibsDir = Join-Path $RootDir "libs\onnxruntime"
if (-not (Test-Path $LibsDir)) { New-Item -ItemType Directory -Path $LibsDir -Force | Out-Null }

$OrtDll = Join-Path $LibsDir "onnxruntime.dll"
if ((Test-Path $OrtDll) -and (Get-Item $OrtDll).Length -ge 1MB) {
    Write-Host "Using existing ONNX Runtime in $LibsDir"
} else {
    Remove-Item -LiteralPath $OrtDll -Force -ErrorAction SilentlyContinue
    $OrtZip = Join-Path $CacheDir "onnxruntime-win-x64.zip"
    Download-File $OrtUrl $OrtZip 1MB

    $ExtractDir = Join-Path $CacheDir "onnxruntime-win-x64"
    if (Test-Path $ExtractDir) { Remove-Item -Recurse -Force $ExtractDir }
    Expand-ZipWithRetry $OrtUrl $OrtZip $ExtractDir 1MB

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

Download-File $ScrfdModelUrl (Join-Path $ModelsDir "scrfd_10g_bnkps.onnx") 1MB
Download-File $AdafaceModelUrl (Join-Path $ModelsDir "adaface_ir101_webface12m.onnx") 1MB

Write-Host "Installed models:"
Write-Host "- $(Join-Path $ModelsDir 'scrfd_10g_bnkps.onnx')"
Write-Host "- $(Join-Path $ModelsDir 'adaface_ir101_webface12m.onnx')"

Write-Host "`nDone."
