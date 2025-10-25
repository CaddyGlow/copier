# install.ps1 - Install quickctx on Windows
# Usage: irm https://raw.githubusercontent.com/CaddyGlow/quickctx/main/install.ps1 | iex

$ErrorActionPreference = 'Stop'

# Configuration
$repo = "CaddyGlow/quickctx"
$appName = "quickctx"
$installDir = "$env:LOCALAPPDATA\Programs\$appName"

Write-Host "Installing $appName..." -ForegroundColor Cyan

# Detect architecture
$arch = if ([System.Environment]::Is64BitOperatingSystem) { "x86_64" } else { "i686" }
$target = "$arch-pc-windows-msvc"

try {
    # Fetch latest release
    Write-Host "Fetching latest release from GitHub..."
    $release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -ErrorAction Stop
    $version = $release.tag_name

    Write-Host "Latest version: $version" -ForegroundColor Green

    # Find the right asset
    # Pattern matches: quickctx-x86_64-pc-windows-msvc.zip or similar
    $asset = $release.assets | Where-Object {
        $_.name -match "$appName.*$target.*\.zip" -or
        $_.name -match "$appName.*windows.*$arch.*\.zip"
    } | Select-Object -First 1

    if (-not $asset) {
        Write-Error "Could not find Windows binary for $target in release assets. Available assets: $($release.assets.name -join ', ')"
        exit 1
    }

    Write-Host "Downloading $($asset.name)..." -ForegroundColor Cyan
    $zipPath = "$env:TEMP\$($asset.name)"

    # Download with progress
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath -ErrorAction Stop
    $ProgressPreference = 'Continue'

    Write-Host "Download complete: $([math]::Round((Get-Item $zipPath).Length / 1MB, 2)) MB" -ForegroundColor Green

    # Extract
    Write-Host "Installing to $installDir..." -ForegroundColor Cyan

    # Remove old installation if it exists
    if (Test-Path $installDir) {
        Write-Host "Removing previous installation..."
        Remove-Item -Path $installDir -Recurse -Force
    }

    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    Expand-Archive -Path $zipPath -DestinationPath $installDir -Force

    # Verify installation
    $exePath = "$installDir\$appName.exe"
    if (-not (Test-Path $exePath)) {
        Write-Error "Installation failed: $appName.exe not found in extracted archive"
        exit 1
    }

    # Clean up
    Remove-Item $zipPath -ErrorAction SilentlyContinue

    # Add to PATH
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$installDir*") {
        Write-Host "Adding to PATH..." -ForegroundColor Cyan
        [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
        $env:Path = "$env:Path;$installDir"  # Update current session
        Write-Host "✓ Added to PATH" -ForegroundColor Green
        Write-Host "  Note: You may need to restart your terminal for PATH changes to take effect" -ForegroundColor Yellow
    } else {
        Write-Host "✓ Already in PATH" -ForegroundColor Green
    }

    # Test installation
    Write-Host "`nVerifying installation..." -ForegroundColor Cyan
    try {
        $installedVersion = & $exePath --version 2>&1
        Write-Host "✓ $appName $version installed successfully!" -ForegroundColor Green
        Write-Host "  Installed to: $installDir" -ForegroundColor Gray
        Write-Host "`nGet started with:" -ForegroundColor Cyan
        Write-Host "  $appName --help" -ForegroundColor White
        Write-Host "  $appName update    # Check for updates" -ForegroundColor White
    } catch {
        Write-Warning "Installation completed but verification failed: $_"
        Write-Host "Try running: $appName --version" -ForegroundColor Yellow
    }

} catch {
    Write-Error "Installation failed: $_"
    Write-Host "`nTroubleshooting:" -ForegroundColor Yellow
    Write-Host "  1. Check your internet connection" -ForegroundColor Gray
    Write-Host "  2. Verify the repository exists: https://github.com/$repo" -ForegroundColor Gray
    Write-Host "  3. Check releases: https://github.com/$repo/releases" -ForegroundColor Gray
    Write-Host "  4. Try manual installation from: https://github.com/$repo/releases/latest" -ForegroundColor Gray
    exit 1
}
