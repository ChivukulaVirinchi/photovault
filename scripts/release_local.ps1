$ErrorActionPreference = "Stop"

param(
    [ValidateSet("quick", "full")]
    [string]$Mode = "full",
    [switch]$IncludeLinux,
    [switch]$KeepArtifacts,
    [switch]$RequireAdminSmoke,
    [string]$TargetDir = "target-win",
    [string]$WixBinPath = "C:\tools\wix311"
)

Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $RepoRoot

$ArtifactsDir = Join-Path $RepoRoot "artifacts\local"
$WixWorkDir = Join-Path $env:TEMP "photovault-localwix"
$MsiPath = Join-Path $ArtifactsDir "PhotoVault-Setup-x64.msi"
$ZipPath = Join-Path $ArtifactsDir "photovault-x86_64-pc-windows-msvc.zip"
$AssetPackPath = Join-Path $ArtifactsDir "PhotoVault-Assets.zip"

$results = [System.Collections.Generic.List[object]]::new()
$installedMsi = $false

function Add-Result([string]$Name, [string]$Status, [string]$Info = "") {
    $results.Add([PSCustomObject]@{
        Step = $Name
        Status = $Status
        Info = $Info
    }) | Out-Null
}

function Is-Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($id)
    return $principal.IsInRole([Security.Principal.WindowsBuiltinRole]::Administrator)
}

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Name"
    }
}

function Invoke-Step([string]$Name, [scriptblock]$Body) {
    try {
        & $Body
        Add-Result $Name "PASS"
    } catch {
        Add-Result $Name "FAIL" $_.Exception.Message
        throw
    }
}

function Remove-PathSafe([string]$Path) {
    if (Test-Path $Path) {
        Remove-Item $Path -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-RobocopyMirror([string]$Source, [string]$Destination, [string[]]$ExcludeDirs) {
    $args = @($Source, $Destination, '/MIR', '/NFL', '/NDL', '/NJH', '/NJS', '/NP')
    if ($ExcludeDirs.Count -gt 0) {
        $args += '/XD'
        $args += $ExcludeDirs
    }
    & robocopy @args | Out-Null
    if ($LASTEXITCODE -ge 8) {
        throw "robocopy failed with exit code $LASTEXITCODE"
    }
}

function Build-AssetPack([string]$OutputZip) {
    if (-not (Test-Path "libs\onnxruntime\onnxruntime.dll") -or
        -not (Test-Path "models\scrfd_10g_bnkps.onnx") -or
        -not (Test-Path "models\glintr100.onnx") -or
        -not (Test-Path "data\geonames.db")) {
        & powershell -ExecutionPolicy Bypass -File "scripts\setup_assets.ps1" | Out-Null
    }

    $tempPack = Join-Path $ArtifactsDir "assets-pack-staging"
    Remove-PathSafe $tempPack
    New-Item -ItemType Directory -Path "$tempPack\libs\onnxruntime" -Force | Out-Null
    New-Item -ItemType Directory -Path "$tempPack\models" -Force | Out-Null
    New-Item -ItemType Directory -Path "$tempPack\data" -Force | Out-Null

    Copy-Item "libs\onnxruntime\onnxruntime.dll" "$tempPack\libs\onnxruntime\onnxruntime.dll" -Force
    Copy-Item "models\scrfd_10g_bnkps.onnx" "$tempPack\models\scrfd_10g_bnkps.onnx" -Force
    Copy-Item "models\glintr100.onnx" "$tempPack\models\glintr100.onnx" -Force
    Copy-Item "data\geonames.db" "$tempPack\data\geonames.db" -Force

    if (Test-Path $OutputZip) {
        Remove-Item $OutputZip -Force
    }
    Compress-Archive -Path "$tempPack\*" -DestinationPath $OutputZip -Force
    Remove-PathSafe $tempPack
}

try {
    Write-Host "PhotoVault local release verification"
    Write-Host "===================================="

    Invoke-Step "validate-toolchain" {
        Require-Command cargo
        Require-Command rustc
        $rustc = (& rustc -Vv) -join "`n"
        if ($rustc -notmatch "host:\s+x86_64-pc-windows-msvc") {
            throw "rustc host is not x86_64-pc-windows-msvc"
        }
    }

    Invoke-Step "prepare-artifacts-dir" {
        Remove-PathSafe $ArtifactsDir
        New-Item -ItemType Directory -Path $ArtifactsDir -Force | Out-Null
    }

    Invoke-Step "build-windows-release" {
        & cargo build --release --target x86_64-pc-windows-msvc --target-dir $TargetDir | Out-Null
        $exe = Join-Path $RepoRoot "$TargetDir\x86_64-pc-windows-msvc\release\photovault.exe"
        if (-not (Test-Path $exe)) {
            throw "Expected executable missing: $exe"
        }
    }

    Invoke-Step "build-portable-zip" {
        $stage = Join-Path $ArtifactsDir "zip-stage\photovault"
        Remove-PathSafe (Join-Path $ArtifactsDir "zip-stage")
        New-Item -ItemType Directory -Path $stage -Force | Out-Null
        Copy-Item "$TargetDir\x86_64-pc-windows-msvc\release\photovault.exe" "$stage\photovault.exe" -Force
        Copy-Item "README.md" "$stage\README.md" -Force
        Copy-Item "LICENSE" "$stage\LICENSE" -Force
        if (Test-Path $ZipPath) {
            Remove-Item $ZipPath -Force
        }
        Compress-Archive -Path "$($ArtifactsDir)\zip-stage\photovault" -DestinationPath $ZipPath -Force
        Remove-PathSafe (Join-Path $ArtifactsDir "zip-stage")
        if (-not (Test-Path $ZipPath)) {
            throw "ZIP output missing: $ZipPath"
        }
    }

    Invoke-Step "build-asset-pack" {
        Build-AssetPack $AssetPackPath
        if (-not (Test-Path $AssetPackPath)) {
            throw "Asset pack output missing: $AssetPackPath"
        }
    }

    Invoke-Step "verify-wix-tools" {
        if (-not (Test-Path (Join-Path $WixBinPath "candle.exe"))) {
            throw "Missing candle.exe at $WixBinPath"
        }
        if (-not (Test-Path (Join-Path $WixBinPath "light.exe"))) {
            throw "Missing light.exe at $WixBinPath"
        }
    }

    Invoke-Step "build-msi" {
        & cargo install cargo-wix --locked | Out-Null

        Remove-PathSafe $WixWorkDir
        New-Item -ItemType Directory -Path $WixWorkDir -Force | Out-Null

        Invoke-RobocopyMirror $RepoRoot $WixWorkDir @('.git', 'target', 'target-win', '.github', 'website', '.cache', 'libs', 'models', 'data', 'artifacts')

        $srcExe = Join-Path $RepoRoot "$TargetDir\x86_64-pc-windows-msvc\release\photovault.exe"
        $dstBinDir = Join-Path $WixWorkDir "target-win\x86_64-pc-windows-msvc\release"
        New-Item -ItemType Directory -Path $dstBinDir -Force | Out-Null
        Copy-Item $srcExe (Join-Path $dstBinDir "photovault.exe") -Force

        Push-Location $WixWorkDir
        try {
            & cargo wix --no-build --target x86_64-pc-windows-msvc --target-bin-dir "$dstBinDir" --bin-path "$WixBinPath" --output target\wix\PhotoVault-Setup-x64.msi | Out-Null
            New-Item -ItemType Directory -Path (Split-Path $MsiPath -Parent) -Force | Out-Null
            Copy-Item ".\target\wix\PhotoVault-Setup-x64.msi" $MsiPath -Force
        } finally {
            Pop-Location
        }

        if (-not (Test-Path $MsiPath)) {
            throw "MSI output missing: $MsiPath"
        }
    }

    if ($Mode -eq "full") {
        $isAdmin = Is-Admin
        if ($RequireAdminSmoke -and -not $isAdmin) {
            throw "Full MSI smoke test requires Administrator PowerShell session"
        }

        if (-not $isAdmin) {
            Add-Result "smoke-msi-install" "SKIP" "Run as Administrator for install/uninstall smoke test"
        } else {
            Invoke-Step "smoke-msi-install" {
                $p = Start-Process msiexec.exe -ArgumentList "/i `"$MsiPath`" /qn /norestart" -PassThru -Wait
                if ($p.ExitCode -ne 0) {
                    throw "MSI install failed with exit code $($p.ExitCode)"
                }
                $installedMsi = $true

                $installedExe = Join-Path $env:ProgramFiles "PhotoVault\photovault.exe"
                if (-not (Test-Path $installedExe)) {
                    throw "Installed exe missing: $installedExe"
                }

                $proc = Start-Process -FilePath $installedExe -PassThru
                Start-Sleep -Seconds 8
                if ($proc.HasExited -and $proc.ExitCode -ne 0) {
                    throw "Installed app exited unexpectedly with code $($proc.ExitCode)"
                }
                if (-not $proc.HasExited) {
                    Stop-Process -Id $proc.Id -Force
                }
            }
        }
    }

    if ($IncludeLinux) {
        Invoke-Step "linux-verifier" {
            & wsl bash ./scripts/verify_installers_local.sh
            if ($LASTEXITCODE -ne 0) {
                throw "Linux verifier failed"
            }
        }
    }
} catch {
    # Exception already recorded by Invoke-Step
} finally {
    if ($installedMsi) {
        $p = Start-Process msiexec.exe -ArgumentList "/x `"$MsiPath`" /qn /norestart" -PassThru -Wait
        if ($p.ExitCode -eq 0) {
            Add-Result "smoke-msi-uninstall" "PASS"
        } else {
            Add-Result "smoke-msi-uninstall" "FAIL" "Exit code $($p.ExitCode)"
        }
    }

    Remove-PathSafe $WixWorkDir

    if (-not $KeepArtifacts) {
        Remove-PathSafe (Join-Path $RepoRoot $TargetDir)
    }

    "" | Write-Host
    $results | Format-Table -AutoSize

    $reportPath = Join-Path $ArtifactsDir "local-release-report.txt"
    New-Item -ItemType Directory -Path (Split-Path $reportPath -Parent) -Force | Out-Null
    $results | Out-String | Set-Content -Path $reportPath -Encoding UTF8
    Write-Host "Report: $reportPath"

    if ($results.Status -contains "FAIL") {
        Write-Host "Overall: FAIL"
        exit 1
    }

    Write-Host "Overall: PASS"
}
