$ErrorActionPreference = "Stop"

param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [switch]$RunLocalChecks,
    [switch]$IncludeLinuxChecks,
    [switch]$Wait,
    [string]$Branch = "master",
    [switch]$DeleteTagOnFailure,
    [switch]$AllowDirty,
    [switch]$KeepLocalArtifacts,
    [switch]$SkipChecksumsVerification
)

Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $RepoRoot

if ($Version -notmatch '^v\d+\.\d+\.\d+(-rc\d+)?$') {
    throw "Version must match vX.Y.Z or vX.Y.Z-rcN"
}

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Name"
    }
}

Require-Command git
Require-Command gh
Require-Command cargo

if (-not $AllowDirty) {
    $dirty = (& git status --porcelain)
    if ($dirty) {
        throw "Working tree is dirty. Commit/stash before release publish. Use -AllowDirty to override."
    }
}

$currentBranch = (& git rev-parse --abbrev-ref HEAD).Trim()
if ($currentBranch -ne $Branch) {
    throw "Current branch is '$currentBranch'. Expected '$Branch'."
}

& gh auth status | Out-Null

& git fetch origin $Branch --tags

$localHead = (& git rev-parse HEAD).Trim()
$remoteHead = (& git rev-parse "origin/$Branch").Trim()
if ($localHead -ne $remoteHead) {
    throw "Local $Branch is not in sync with origin/$Branch"
}

$tagContainsOutput = (& git tag --contains HEAD)
if ($tagContainsOutput -match '^v') {
    throw "HEAD already has release tags. Move to a new commit before creating another release tag."
}

if ($RunLocalChecks) {
    $args = @("-Mode", "full")
    if ($IncludeLinuxChecks) {
        $args += "-IncludeLinux"
    }
    if ($KeepLocalArtifacts) {
        $args += "-KeepArtifacts"
    }
    & powershell -ExecutionPolicy Bypass -File "scripts\release_local.ps1" @args
    if ($LASTEXITCODE -ne 0) {
        throw "Local release checks failed"
    }
}

Write-Host "Enforcing push gate: cargo fmt --all --check, cargo clippy --all-targets, cargo test --no-run"
& cargo fmt --all --check
if ($LASTEXITCODE -ne 0) {
    throw "cargo fmt --all --check failed"
}
& cargo clippy --all-targets
if ($LASTEXITCODE -ne 0) {
    throw "cargo clippy --all-targets failed"
}
& cargo test --no-run
if ($LASTEXITCODE -ne 0) {
    throw "cargo test --no-run failed"
}

if ((& git rev-parse -q --verify "refs/tags/$Version" 2>$null)) {
    throw "Tag already exists locally: $Version"
}

& git ls-remote --exit-code --tags origin "refs/tags/$Version" *> $null
if ($LASTEXITCODE -eq 0) {
    throw "Tag already exists on origin: $Version"
}

& git tag -a $Version -m "PhotoVault $Version"

try {
    & git push origin $Version

    if ($Wait) {
        Start-Sleep -Seconds 8

        $runId = (& gh run list --workflow release.yml --limit 30 --json databaseId,headSha,displayTitle,event --jq ".[] | select(.event==\"push\" and (.displayTitle==\"$Version\" or .headSha==\"$localHead\")) | .databaseId" | Select-Object -First 1)
        if (-not $runId) {
            throw "Could not find release workflow run for tag push"
        }

        & gh run watch $runId --exit-status

        $required = @(
            "PhotoVault-Setup-x64.msi",
            "photovault-x86_64-pc-windows-msvc.zip",
            "PhotoVault-ubuntu-amd64.deb",
            "PhotoVault-x86_64.AppImage",
            "PhotoVault-Assets.zip"
        )
        if (-not $SkipChecksumsVerification) {
            $required += "SHA256SUMS"
        }

        $release = (& gh release view $Version --json isDraft,url,assets)
        if (-not $release) {
            throw "Failed to fetch GitHub release data for $Version"
        }
        $obj = $release | ConvertFrom-Json
        if (-not $obj.isDraft) {
            throw "Release is already published; expected draft"
        }

        $assetNames = @($obj.assets | ForEach-Object { $_.name })
        $missing = @($required | Where-Object { $_ -notin $assetNames })
        if ($missing.Count -gt 0) {
            throw "Draft release is missing expected assets: $($missing -join ', ')"
        }

        Write-Host "Draft release ready: $($obj.url)"
        Write-Host "Publish manually from GitHub UI when ready."
    } else {
        Write-Host "Tag pushed: $Version"
        Write-Host "Release workflow is running."
    }
} catch {
    Write-Host "Release publish failed: $($_.Exception.Message)"
    if ($DeleteTagOnFailure) {
        Write-Host "Deleting pushed/local tag due to failure..."
        & git push origin ":refs/tags/$Version" 2>$null
        & git tag -d $Version 2>$null
    }
    throw
}
