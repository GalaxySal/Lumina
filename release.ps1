# Release Script for Lumina Browser
$ErrorActionPreference = "Stop"

Write-Host "🚀 Starting Release Process for Lumina Browser v0.2.2..." -ForegroundColor Cyan

# Define paths
$ScriptDir = $PSScriptRoot
$TauriDir = Join-Path $ScriptDir "src-tauri"
$ReleaseDir = Join-Path $ScriptDir "Release"

# Create Release directory if it doesn't exist
if (-not (Test-Path $ReleaseDir)) {
    New-Item -ItemType Directory -Path $ReleaseDir | Out-Null
    Write-Host "📁 Created Release directory: $ReleaseDir" -ForegroundColor Green
}

# Build Sidecars
Write-Host "🔧 Building Sidecars..." -ForegroundColor Yellow
$BuildSidecarsScript = Join-Path $ScriptDir "scripts\build-sidecars.ps1"
if (Test-Path $BuildSidecarsScript) {
    & $BuildSidecarsScript
} else {
    Write-Warning "Sidecar build script not found at $BuildSidecarsScript"
}

# Navigate to src-tauri
Push-Location $TauriDir

try {
    Write-Host "🔨 Building Tauri App..." -ForegroundColor Yellow
    # Run the build command
    # Note: Ensure 'cargo tauri' is available or use 'cargo run -- tauri build'
    # We use 'cargo tauri build' assuming tauri-cli is installed via cargo or we can use 'npm run tauri build' if it's an npm project.
    # Looking at the file structure, it seems to be a .NET Blazor app with Tauri.
    # The tauri.conf.json has beforeBuildCommand: "dotnet publish -c release src/tauri-browser.csproj -o dist"
    
    cargo tauri build
    
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✅ Build successful!" -ForegroundColor Green
    } else {
        throw "Build failed with exit code $LASTEXITCODE"
    }
}
catch {
    Write-Host "❌ Error during build: $_" -ForegroundColor Red
    Pop-Location
    exit 1
}

# Locate artifacts
# Usually in target/release/bundle/msi or nsis
$TargetDir = Join-Path $TauriDir "target\release\bundle"
$MsiDir = Join-Path $TargetDir "msi"
$NsisDir = Join-Path $TargetDir "nsis"

# Copy MSI if exists
if (Test-Path $MsiDir) {
    $MsiFiles = Get-ChildItem -Path $MsiDir -Filter "*.msi"
    foreach ($File in $MsiFiles) {
        Copy-Item -Path $File.FullName -Destination $ReleaseDir -Force
        Write-Host "📦 Copied MSI: $($File.Name)" -ForegroundColor Green
    }
}

# Copy NSIS (exe) if exists
if (Test-Path $NsisDir) {
    $ExeFiles = Get-ChildItem -Path $NsisDir -Filter "*.exe"
    foreach ($File in $ExeFiles) {
        Copy-Item -Path $File.FullName -Destination $ReleaseDir -Force
        Write-Host "📦 Copied Setup EXE: $($File.Name)" -ForegroundColor Green
    }
}

Pop-Location

# --- Generate Release Notes ---
Write-Host "📝 Generating Release Notes..." -ForegroundColor Yellow

try {
    # Get the latest tag
    $LatestTag = git describe --tags --abbrev=0 2>$null
    
    if (-not $LatestTag) {
        Write-Host "⚠️ No tags found. Generating notes from the beginning." -ForegroundColor Yellow
        $GitLog = git log --pretty=format:"%s"
    } else {
        Write-Host "🔖 Generating notes since tag: $LatestTag" -ForegroundColor Cyan
        $GitLog = git log --pretty=format:"%s" "$LatestTag..HEAD"
    }

    if ($GitLog) {
        $Features = @()
        $Fixes = @()
        $Chores = @()
        $Others = @()

        foreach ($Line in $GitLog) {
            if ($Line -match "^feat(\(.*\))?:") { $Features += "- $($Line -replace '^feat(\(.*\))?:\s*', '')" }
            elseif ($Line -match "^fix(\(.*\))?:") { $Fixes += "- $($Line -replace '^fix(\(.*\))?:\s*', '')" }
            elseif ($Line -match "^chore(\(.*\))?:") { $Chores += "- $($Line -replace '^chore(\(.*\))?:\s*', '')" }
            else { $Others += "- $Line" }
        }

        $ReleaseNotes = "# Release Notes v0.2.2`n`n"
        
        if ($Features.Count -gt 0) {
            $ReleaseNotes += "## 🚀 Features`n" + ($Features -join "`n") + "`n`n"
        }
        if ($Fixes.Count -gt 0) {
            $ReleaseNotes += "## 🐛 Fixes`n" + ($Fixes -join "`n") + "`n`n"
        }
        if ($Chores.Count -gt 0) {
            $ReleaseNotes += "## 🔧 Chores & Improvements`n" + ($Chores -join "`n") + "`n`n"
        }
        if ($Others.Count -gt 0) {
            $ReleaseNotes += "## 📋 Other Changes`n" + ($Others -join "`n") + "`n`n"
        }

        $NotesPath = Join-Path $ReleaseDir "RELEASE_NOTES.md"
        $ReleaseNotes | Out-File -FilePath $NotesPath -Encoding utf8
        Write-Host "📄 Release Notes saved to: $NotesPath" -ForegroundColor Green
    } else {
        Write-Host "ℹ️ No new commits found since last tag." -ForegroundColor Gray
    }
}
catch {
    Write-Host "⚠️ Failed to generate release notes: $_" -ForegroundColor Yellow
}

Write-Host "🎉 Release process completed! Check the 'Release' folder." -ForegroundColor Cyan
