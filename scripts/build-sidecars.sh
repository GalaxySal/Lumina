#!/bin/bash
set -e

# Determine Root Directory (One level up from scripts/)
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="$ROOT/src-tauri/binaries"

# Create binaries directory if it doesn't exist
mkdir -p "$BIN_DIR"
echo "Created directory: $BIN_DIR"

# Get Rust Target Triple
if ! command -v rustc &> /dev/null; then
    echo "Error: Failed to detect target triple. Ensure 'rustc' is installed."
    exit 1
fi

TARGET_TRIPLE=$(rustc -vV | grep "host:" | awk '{print $2}')
echo "Detected Target Triple: $TARGET_TRIPLE"

# Determine executable extension
EXT=""
if [[ "$TARGET_TRIPLE" == *"windows"* ]]; then
    EXT=".exe"
fi

# ---------------------------------------------------------
# 1. Build Go Sidecar (lumina-net)
# ---------------------------------------------------------
echo -e "\nBuilding Go Sidecar (lumina-net)..."
GO_DIR="$ROOT/src-go"
if [ -d "$GO_DIR" ]; then
    pushd "$GO_DIR" > /dev/null
    GO_OUT="$BIN_DIR/lumina-net-$TARGET_TRIPLE$EXT"
    
    # Build command
    go build -o "$GO_OUT"
    
    if [ $? -eq 0 ]; then
        echo "Go Sidecar built successfully: $GO_OUT"
    else
        echo "Error: Go build failed"
        exit 1
    fi
    popd > /dev/null
else
    echo "Warning: src-go directory not found at $GO_DIR"
fi

# ---------------------------------------------------------
# 2. Build Kip Sidecar (kip-lang)
# ---------------------------------------------------------
echo -e "\nBuilding Kip Sidecar (kip-lang)..."
KIP_DIR="$ROOT/src-kip"
if [ -d "$KIP_DIR" ]; then
    pushd "$KIP_DIR" > /dev/null
    # Build command
    cargo build --release --bin kip-rs
    
    if [ $? -eq 0 ]; then
        KIP_SRC_NAME="kip-rs$EXT"
        KIP_SRC_PATH="target/release/$KIP_SRC_NAME"
        
        if [ -f "$KIP_SRC_PATH" ]; then
            KIP_DEST="$BIN_DIR/kip-lang-$TARGET_TRIPLE$EXT"
            cp "$KIP_SRC_PATH" "$KIP_DEST"
            echo "Kip Sidecar built and moved to: $KIP_DEST"
        else
            echo "Error: Could not find compiled binary at $KIP_SRC_PATH"
            exit 1
        fi
    else
        echo "Error: Kip build failed"
        exit 1
    fi
    popd > /dev/null
else
    echo "Warning: src-kip directory not found at $KIP_DIR"
fi

# ---------------------------------------------------------
# 3. Build Python Sidecar (lumina-sidekick)
# ---------------------------------------------------------
echo -e "\nBuilding Python Sidecar (lumina-sidekick)..."
SIDEKICK_DIR="$ROOT/src-sidekick"
if [ -d "$SIDEKICK_DIR" ]; then
    pushd "$SIDEKICK_DIR" > /dev/null
    
    # Check if Python is available
    if ! command -v python3 &> /dev/null && ! command -v python &> /dev/null; then
        echo "Error: Python is not installed or not in PATH."
        exit 1
    fi
    PYTHON_CMD=$(command -v python3 || command -v python)
    
    # Install requirements
    echo "Installing requirements..."
    "$PYTHON_CMD" -m pip install -r requirements.txt > /dev/null
    
    # Build command using PyInstaller
    echo "Running PyInstaller build..."
    
    SEP=":"
    # Even on bash, if we are on mingw/msys2/git bash, we might need ; for pyinstaller add-data
    if [[ "$TARGET_TRIPLE" == *"windows"* ]]; then SEP=";"; fi
    
    PY_ARGS=(
        "--noconfirm"
        "--onefile"
        "--windowed"
        "--name" "LuminaSidekick"
        "--collect-all" "llama_cpp"
        "--copy-metadata=imageio"
        "--copy-metadata=moviepy"
        "--hidden-import=moviepy"
        "--hidden-import=proglog"
        "--hidden-import=tqdm"
        "--add-data" "main.py${SEP}."
    )

    if [[ "$TARGET_TRIPLE" == *"windows"* ]]; then
        PY_ARGS+=("--icon=../src-tauri/icons/icon.ico")
    fi

    PY_ARGS+=("main.py")

    echo "Executing: pyinstaller ${PY_ARGS[@]}"
    pyinstaller "${PY_ARGS[@]}"
    
    DIST_PATH="dist/LuminaSidekick$EXT"
    
    if [ -f "$DIST_PATH" ]; then
        SIDEKICK_DEST="$BIN_DIR/lumina-sidekick-$TARGET_TRIPLE$EXT"
        cp "$DIST_PATH" "$SIDEKICK_DEST"
        echo "Python Sidecar built and moved to: $SIDEKICK_DEST"
    else
        echo "Error: LuminaSidekick binary was not created in dist/!"
        exit 1
    fi
    popd > /dev/null
else
    echo "Warning: src-sidekick directory not found at $SIDEKICK_DIR"
fi
