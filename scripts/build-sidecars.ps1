# scripts/build-sidecars.ps1
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

# ---------------------------------------------------------
# 1. Build Go Sidecar (lumina-net)
# ---------------------------------------------------------
Write-Host "`nBuilding Go Sidecar (lumina-net)..." -ForegroundColor Yellow
$GoDir = Join-Path $Root "src-go"
if (Test-Path $GoDir) {
    Push-Location $GoDir
    try {
        $GoOut = Join-Path $BinDir "lumina-net-$TargetTriple$Ext"
        
        # Build command
        go build -o $GoOut
        
        if ($LASTEXITCODE -eq 0) {
            Write-Host "Go Sidecar built successfully: $GoOut" -ForegroundColor Green
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
Write-Host "`nBuilding Kip Sidecar (kip-lang)..." -ForegroundColor Yellow
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
                Write-Host "Kip Sidecar built and moved to: $KipDest" -ForegroundColor Green
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

# ---------------------------------------------------------
# 3. Build Python Sidecar (lumina-sidekick)
# ---------------------------------------------------------
Write-Host "`nBuilding Python Sidecar (lumina-sidekick)..." -ForegroundColor Yellow
$SidekickDir = Join-Path $Root "src-sidekick"
if (Test-Path $SidekickDir) {
    Push-Location $SidekickDir
    try {
        # Check if Python is available
        if (!(Get-Command python -ErrorAction SilentlyContinue)) {
            Write-Error "Python is not installed or not in PATH."
            exit 1
        }
        
        # Install requirements
        Write-Host "Installing requirements..." -ForegroundColor Gray
        python -m pip install -r requirements.txt | Out-Null
        
        # Build command using PyInstaller
        Write-Host "Running PyInstaller build..." -ForegroundColor Gray
        
        $Sep = ":"
        if ($TargetTriple -match "windows") { $Sep = ";" }
        
        $PyArgs = @(
            "--noconfirm",
            "--onefile",
            "--windowed",
            "--name", "LuminaSidekick",
            "--collect-all", "llama_cpp",
            "--copy-metadata=imageio",
            "--copy-metadata=moviepy",
            "--hidden-import=moviepy",
            "--hidden-import=proglog",
            "--hidden-import=tqdm",
            "--add-data", "main.py$($Sep)."
        )

        if ($TargetTriple -match "windows") {
            $PyArgs += "--icon=../src-tauri/icons/icon.ico"
        }

        $PyArgs += "main.py"

        Write-Host "Executing: pyinstaller $PyArgs" -ForegroundColor DarkGray
        & pyinstaller $PyArgs
        
        $DistPath = Join-Path "dist" "LuminaSidekick$Ext"
        
        if (Test-Path $DistPath) {
            $SidekickDest = Join-Path $BinDir "lumina-sidekick-$TargetTriple$Ext"
            Copy-Item -Path $DistPath -Destination $SidekickDest -Force
            Write-Host "Python Sidecar built and moved to: $SidekickDest" -ForegroundColor Green
        } else {
            Write-Error "LuminaSidekick binary was not created in dist/!"
        }

    } catch {
        Write-Error "Python build failed: $_"
    } finally {
        Pop-Location
    }
} else {
    Write-Warning "src-sidekick directory not found at $SidekickDir"
}
