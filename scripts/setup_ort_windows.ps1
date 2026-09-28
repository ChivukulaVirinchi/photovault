param(
    [string]$OutputDir = (Join-Path $PSScriptRoot "../libs/onnxruntime"),
    [string]$CacheDir = (Join-Path $PSScriptRoot "../.cache/downloads/directml")
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
Add-Type -AssemblyName System.Net.Http
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

# Pinned native ORT package and its declared DirectML dependency, not the CPU archive.
$packages = @(
    @{
        Id = "microsoft.ml.onnxruntime.directml"; Version = "1.23.0"
        Hash = "a33ec2382b3c440bab74042a135733bb6e5085f293b908d3997688a58fe307e7"
        Entries = @{
            "runtimes/win-x64/native/onnxruntime.dll" = "onnxruntime.dll"
            "runtimes/win-x64/native/onnxruntime_providers_shared.dll" = "onnxruntime_providers_shared.dll"
            "LICENSE" = "LICENSE-onnxruntime.txt"
        }
    },
    @{
        Id = "microsoft.ai.directml"; Version = "1.15.4"
        Hash = "4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9"
        Entries = @{
            "bin/x64-win/DirectML.dll" = "DirectML.dll"
            "LICENSE.txt" = "LICENSE-DirectML.txt"
        }
    }
)
$outputRoot = [IO.Path]::GetFullPath($OutputDir)
$cacheRoot = [IO.Path]::GetFullPath($CacheDir)
New-Item -ItemType Directory -Path $outputRoot, $cacheRoot -Force | Out-Null
$stage = Join-Path $cacheRoot ("stage-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $stage | Out-Null
try {
    foreach ($package in $packages) {
        $name = "$($package.Id).$($package.Version).nupkg"
        $archive = Join-Path $cacheRoot $name
        $valid = (Test-Path -LiteralPath $archive) -and ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -eq $package.Hash)
        if (-not $valid) {
            $client = [Net.Http.HttpClient]::new()
            $client.Timeout = [TimeSpan]::FromMinutes(10)
            try {
                $bytes = $client.GetByteArrayAsync("https://api.nuget.org/v3-flatcontainer/$($package.Id)/$($package.Version)/$name").GetAwaiter().GetResult()
                $sha = [Security.Cryptography.SHA256]::Create()
                try { $hash = ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant() }
                finally { $sha.Dispose() }
                if ($hash -ne $package.Hash) { throw "Checksum mismatch downloading $name; nothing was installed." }
                [IO.File]::WriteAllBytes($archive, $bytes)
            } finally { $client.Dispose() }
        }
        $zip = [IO.Compression.ZipFile]::OpenRead($archive)
        try {
            foreach ($entryName in $package.Entries.Keys) {
                $entry = $zip.GetEntry($entryName)
                if ($null -eq $entry) { throw "Missing package entry $entryName" }
                [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $stage $package.Entries[$entryName]))
            }
        } finally { $zip.Dispose() }
    }
    # All package bytes and required files are verified before installation.
    Get-ChildItem -LiteralPath $stage -File | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $outputRoot $_.Name) -Force
    }
    Write-Host "Installed ORT 1.23.0 + DirectML 1.15.4 (Windows x64) in $outputRoot"
} finally {
    $resolvedStage = [IO.Path]::GetFullPath($stage)
    if (-not $resolvedStage.StartsWith($cacheRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing cleanup outside the download cache"
    }
    Remove-Item -LiteralPath $resolvedStage -Recurse -Force
}
