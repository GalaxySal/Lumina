#!/bin/bash
set -e

echo -e "\033[0;36m🚀 Starting Release Process for Lumina Browser...\033[0m"

# Define paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAURI_DIR="$SCRIPT_DIR/src-tauri"
RELEASE_DIR="$SCRIPT_DIR/Release"

# Create Release directory if it doesn't exist
mkdir -p "$RELEASE_DIR"
echo -e "\033[0;32m📁 Created Release directory: $RELEASE_DIR\033[0m"

# Build Sidecars
echo -e "\033[0;33m🔧 Building Sidecars...\033[0m"
BUILD_SIDECARS_SCRIPT="$SCRIPT_DIR/scripts/build-sidecars.sh"
if [ -f "$BUILD_SIDECARS_SCRIPT" ]; then
    bash "$BUILD_SIDECARS_SCRIPT"
else
    echo -e "\033[0;31mWarning: Sidecar build script not found at $BUILD_SIDECARS_SCRIPT\033[0m"
fi

# Navigate to src-tauri
pushd "$TAURI_DIR" > /dev/null

echo -e "\033[0;33m🔨 Building Tauri App...\033[0m"
# Build command
cargo tauri build

if [ $? -eq 0 ]; then
    echo -e "\033[0;32m✅ Build successful!\033[0m"
else
    echo -e "\033[0;31m❌ Build failed!\033[0m"
    popd > /dev/null
    exit 1
fi

# Locate artifacts
TARGET_DIR="$TAURI_DIR/target/release/bundle"
MSI_DIR="$TARGET_DIR/msi"
NSIS_DIR="$TARGET_DIR/nsis"
APPIMAGE_DIR="$TARGET_DIR/appimage"
DEB_DIR="$TARGET_DIR/deb"

# Copy MSI if exists
if [ -d "$MSI_DIR" ]; then
    find "$MSI_DIR" -name "*.msi" -exec cp {} "$RELEASE_DIR" \;
    echo -e "\033[0;32m📦 Copied MSI artifacts\033[0m"
fi

# Copy NSIS (exe) if exists
if [ -d "$NSIS_DIR" ]; then
    find "$NSIS_DIR" -name "*.exe" -exec cp {} "$RELEASE_DIR" \;
    echo -e "\033[0;32m📦 Copied NSIS artifacts\033[0m"
fi

# Copy AppImage if exists
if [ -d "$APPIMAGE_DIR" ]; then
    find "$APPIMAGE_DIR" -name "*.AppImage" -exec cp {} "$RELEASE_DIR" \;
    echo -e "\033[0;32m📦 Copied AppImage artifacts\033[0m"
fi

# Copy DEB if exists
if [ -d "$DEB_DIR" ]; then
    find "$DEB_DIR" -name "*.deb" -exec cp {} "$RELEASE_DIR" \;
    echo -e "\033[0;32m📦 Copied DEB artifacts\033[0m"
fi

popd > /dev/null

# --- Generate Release Notes ---
echo -e "\033[0;33m📝 Generating Release Notes...\033[0m"

# Get the latest tag
LATEST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")

if [ -z "$LATEST_TAG" ]; then
    echo -e "\033[0;33m⚠️ No tags found. Generating notes from the beginning.\033[0m"
    GIT_LOG=$(git log --pretty=format:"%s")
else
    echo -e "\033[0;36m🔖 Generating notes since tag: $LATEST_TAG\033[0m"
    GIT_LOG=$(git log --pretty=format:"%s" "$LATEST_TAG..HEAD")
fi

if [ ! -z "$GIT_LOG" ]; then
    FEATURES=""
    FIXES=""
    CHORES=""
    OTHERS=""

    while IFS= read -r line; do
        if [[ $line =~ ^feat(\(.*\))?: ]]; then
            FEATURES+="- ${line#*:}\n"
        elif [[ $line =~ ^fix(\(.*\))?: ]]; then
            FIXES+="- ${line#*:}\n"
        elif [[ $line =~ ^chore(\(.*\))?: ]]; then
            CHORES+="- ${line#*:}\n"
        else
            OTHERS+="- $line\n"
        fi
    done <<< "$GIT_LOG"

    RELEASE_NOTES="# Release Notes\n\n"
    
    if [ ! -z "$FEATURES" ]; then
        RELEASE_NOTES+="## 🚀 Features\n$FEATURES\n"
    fi
    if [ ! -z "$FIXES" ]; then
        RELEASE_NOTES+="## 🐛 Fixes\n$FIXES\n"
    fi
    if [ ! -z "$CHORES" ]; then
        RELEASE_NOTES+="## 🔧 Chores & Improvements\n$CHORES\n"
    fi
    if [ ! -z "$OTHERS" ]; then
        RELEASE_NOTES+="## 📋 Other Changes\n$OTHERS\n"
    fi

    NOTES_PATH="$RELEASE_DIR/RELEASE_NOTES.md"
    echo -e "$RELEASE_NOTES" > "$NOTES_PATH"
    echo -e "\033[0;32m📄 Release Notes saved to: $NOTES_PATH\033[0m"
else
    echo -e "\033[0;37mℹ️ No new commits found since last tag.\033[0m"
fi

echo -e "\033[0;36m🎉 Release process completed! Check the 'Release' folder.\033[0m"
