# scripts/build-sidecars.ps1
param (
    [string]$Mode = "Build" # Options: "Build", "Mock"
)

$ErrorActionPreference = "Stop"

# Determine Root Directory (One level up from scripts/)
$Root = $PSScriptRoot | Split-Path -Parent
$BinDir = Join-Path $Root "src-tauri/binaries"

# Create binaries directory if it doesn't exist
if (-not (Test-Path $BinDir)) { 
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    Write-Host "Created directory: $BinDir" -ForegroundColor Gray
}

# Get Rust Target Triple
try {
    $TargetTriple = & rustc -vV | Select-String "host:" | ForEach-Object { $_.ToString().Split(" ")[1].Trim() }
    Write-Host "Detected Target Triple: $TargetTriple" -ForegroundColor Cyan
} catch {
    Write-Error "Failed to detect target triple. Ensure 'rustc' is installed."
    exit 1
}

# Determine executable extension
$Ext = ""
if ($TargetTriple -match "windows") { $Ext = ".exe" }

# Function to create mock binary
function New-MockBinary {
    param ($Name)
    $Path = Join-Path $BinDir "$Name-$TargetTriple$Ext"
    Set-Content -Path $Path -Value "Mock Binary for CI/Clippy"
    Write-Host "✅ Created Mock Binary: $Path" -ForegroundColor Magenta
}

# ---------------------------------------------------------
# MOCK MODE
# ---------------------------------------------------------
if ($Mode -eq "Mock") {
    Write-Host "`n🧪 Running in MOCK mode - creating dummy binaries for CI..." -ForegroundColor Magenta
    New-MockBinary "lumina-net"
    New-MockBinary "kip-lang"
    New-MockBinary "lumina-sidekick"
    exit 0
}

# ---------------------------------------------------------
# BUILD MODE
# ---------------------------------------------------------

# 1. Build Go Sidecar (lumina-net)
# ---------------------------------------------------------
Write-Host "`n🔨 Building Go Sidecar (lumina-net)..." -ForegroundColor Yellow
$GoDir = Join-Path $Root "src-go"
if (Test-Path $GoDir) {
    Push-Location $GoDir
    try {
        $GoOut = Join-Path $BinDir "lumina-net-$TargetTriple$Ext"
        
        # Build command
        go build -o $GoOut
        
        if ($LASTEXITCODE -eq 0) {
            Write-Host "✅ Go Sidecar built successfully: $GoOut" -ForegroundColor Green
        } else {
            Write-Error "Go build failed with exit code $LASTEXITCODE"
        }
    } catch {
        Write-Error "Go build failed: $_"
    } finally {
        Pop-Location
    }
} else {
    Write-Warning "src-go directory not found at $GoDir"
}

# ---------------------------------------------------------
# 2. Build Kip Sidecar (kip-lang)
# ---------------------------------------------------------
Write-Host "`n🔨 Building Kip Sidecar (kip-lang)..." -ForegroundColor Yellow
$KipDir = Join-Path $Root "src-kip"
if (Test-Path $KipDir) {
    Push-Location $KipDir
    try {
        # Build command
        cargo build --release --bin kip-rs
        
        if ($LASTEXITCODE -eq 0) {
            $KipSrcName = "kip-rs$Ext"
            $KipSrcPath = Join-Path "target/release" $KipSrcName
            
            if (Test-Path $KipSrcPath) {
                $KipDest = Join-Path $BinDir "kip-lang-$TargetTriple$Ext"
                Copy-Item -Path $KipSrcPath -Destination $KipDest -Force
                Write-Host "✅ Kip Sidecar built and moved to: $KipDest" -ForegroundColor Green
            } else {
                Write-Error "Could not find compiled binary at $KipSrcPath"
            }
        } else {
            Write-Error "Kip build failed with exit code $LASTEXITCODE"
        }
    } catch {
        Write-Error "Kip build failed: $_"
    } finally {
        Pop-Location
    }
} else {
    Write-Warning "src-kip directory not found at $KipDir"
}

Write-Host "`n🎉 Sidecar build process completed." -ForegroundColor Cyan
