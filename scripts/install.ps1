#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Installer for smista on Windows.

.DESCRIPTION
    Downloads the latest (or a specified) smista release for Windows from
    GitHub, verifies its checksum, extracts the binary into an install
    directory and adds it to the current user's PATH.

.PARAMETER Version
    The smista version to install (defaults to the latest released version).

.PARAMETER InstallDir
    The directory the smista.exe binary is installed into.
    Defaults to "$env:LOCALAPPDATA\Programs\smista".

.PARAMETER Force
    Skip the confirmation prompt during installation. Alias: -Yes.

.EXAMPLE
    irm https://smista.ai/install/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Version 1.0.0 -Force
#>
[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\smista",
    [Alias("Yes")]
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$GithubRepo = "smista-ai/smista.ai"
$IssuesUrl = "https://github.com/$GithubRepo/issues/new"

# -- output helpers ----------------------------------------------------------

function Write-Info {
    param([string]$Message)
    Write-Host "> " -ForegroundColor DarkGray -NoNewline
    Write-Host $Message
}

function Write-Warn {
    param([string]$Message)
    Write-Host "! $Message" -ForegroundColor Yellow
}

function Write-Err {
    param([string]$Message)
    Write-Host "x $Message" -ForegroundColor Red
}

function Write-Completed {
    param([string]$Message)
    Write-Host "✓ " -ForegroundColor Green -NoNewline
    Write-Host $Message
}

function Confirm-Action {
    param([string]$Message)
    if ($Force) {
        return
    }
    $answer = Read-Host "? $Message [y/N]"
    if ($answer -ne "y" -and $answer -ne "yes") {
        Write-Err 'Aborting (please answer "yes" to continue)'
        exit 1
    }
}

# -- platform detection ------------------------------------------------------

# Currently supporting:
#   - x86_64 (AMD64)
#   - aarch64 (ARM64)
function Get-SmistaTarget {
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) {
        $arch = $env:PROCESSOR_ARCHITEW6432
    }

    switch ($arch.ToUpper()) {
        "AMD64" { return "x86_64-pc-windows-msvc" }
        "ARM64" { return "aarch64-pc-windows-msvc" }
        default {
            Write-Err "Unsupported architecture: $arch"
            Write-Info "Only x86_64 (AMD64) and aarch64 (ARM64) are supported by this installer."
            Write-Info "Alternatively you can install smista with Cargo <https://www.rust-lang.org/tools/install>: cargo install smista --locked"
            exit 1
        }
    }
}

# -- version resolution ------------------------------------------------------

function Get-LatestSmistaVersion {
    try {
        $release = Invoke-RestMethod `
            -Uri "https://api.github.com/repos/$GithubRepo/releases/latest" `
            -Headers @{ "User-Agent" = "smista-installer" } `
            -UseBasicParsing
    } catch {
        Write-Err "Could not query the latest smista release: $($_.Exception.Message)"
        Write-Warn "If no release has been published yet, pass a version explicitly with '-Version X.Y.Z'."
        Write-Warn "If you believe this is a bug, please report an issue at <$IssuesUrl>"
        exit 1
    }
    return $release.tag_name.TrimStart("v")
}

# -- installation ------------------------------------------------------------

function Install-Smista {
    param(
        [string]$InstallVersion,
        [string]$Target
    )

    $asset = "smista-v$InstallVersion-$Target.zip"
    $url = "https://github.com/$GithubRepo/releases/download/v$InstallVersion/$asset"

    Write-Host ""
    Write-Host "  smista configuration"
    Write-Info "Version:       $InstallVersion"
    Write-Info "Target:        $Target"
    Write-Info "Install dir:   $InstallDir"
    Write-Host ""

    Confirm-Action "Install smista $InstallVersion?"

    $tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) "smista-$([System.IO.Path]::GetRandomFileName())"
    New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null

    try {
        $archive = Join-Path $tmpDir $asset
        Write-Info "Downloading smista from $url …"
        try {
            Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing
        } catch {
            Write-Err "Failed to download smista: $($_.Exception.Message)"
            Write-Warn "Check that release v$InstallVersion exists and provides artifacts for $Target."
            Write-Warn "If you believe this is a bug, please report an issue at <$IssuesUrl>"
            exit 1
        }

        $checksumFile = "$archive.sha256"
        try {
            Invoke-WebRequest -Uri "$url.sha256" -OutFile $checksumFile -UseBasicParsing
            $expected = (Get-Content $checksumFile -Raw).Trim().ToLower()
            $actual = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
            if ($expected -ne $actual) {
                Write-Err "Checksum mismatch for the downloaded archive (expected $expected, got $actual)."
                Write-Err "Please retry, and report an issue at <$IssuesUrl> if the problem persists."
                exit 1
            }
            Write-Info "Checksum verified"
        } catch {
            Write-Warn "Could not verify the archive checksum: $($_.Exception.Message)"
        }

        Write-Info "Extracting archive …"
        Expand-Archive -Path $archive -DestinationPath $tmpDir -Force

        $binary = Join-Path $tmpDir "smista.exe"
        if (-not (Test-Path $binary)) {
            Write-Err "smista.exe was not found in the downloaded archive."
            Write-Warn "Please report an issue at <$IssuesUrl>"
            exit 1
        }

        if (-not (Test-Path $InstallDir)) {
            New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        }

        Write-Info "Installing smista to $InstallDir …"
        Copy-Item -Path $binary -Destination (Join-Path $InstallDir "smista.exe") -Force

        Add-ToUserPath -Directory $InstallDir
    } finally {
        Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Add-ToUserPath {
    param([string]$Directory)

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $entries = @()
    if ($userPath) {
        $entries = $userPath.Split(";") | Where-Object { $_ -ne "" }
    }

    if ($entries -contains $Directory) {
        return
    }

    Write-Info "Adding $Directory to your user PATH …"
    $newPath = (@($entries) + $Directory) -join ";"
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    # make smista available in the current session too
    $env:Path = "$env:Path;$Directory"
    Write-Warn "Restart your terminal for the PATH change to take effect in new sessions."
}

# -- main --------------------------------------------------------------------

$target = Get-SmistaTarget
if (-not $Version) {
    Write-Info "Resolving the latest smista version…"
    $Version = Get-LatestSmistaVersion
}

Install-Smista -InstallVersion $Version -Target $target

Write-Completed "smista has successfully been installed on your system!"
Write-Info "Get started with the user guide <https://docs.smista.ai>"
Write-Info "If you encounter any issue, please report it at <$IssuesUrl>"

exit 0
