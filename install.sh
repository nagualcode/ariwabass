#!/usr/bin/env bash
set -euo pipefail

echo "=========================================="
echo " Installing Ariwa Bass (macOS Plugin Suite)"
echo "=========================================="

VST3_DIR="$HOME/Library/Audio/Plug-Ins/VST3"
CLAP_DIR="$HOME/Library/Audio/Plug-Ins/CLAP"
AU_DIR="$HOME/Library/Audio/Plug-Ins/Components"

mkdir -p "$VST3_DIR" "$CLAP_DIR" "$AU_DIR"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Check local bundle directory or script directory
BUNDLE_DIR=""
if [ -d "$SCRIPT_DIR/target/bundles" ]; then
    BUNDLE_DIR="$SCRIPT_DIR/target/bundles"
elif [ -d "$SCRIPT_DIR" ]; then
    BUNDLE_DIR="$SCRIPT_DIR"
fi

echo "Copying plugins to user audio library..."

if [ -d "$BUNDLE_DIR/Ariwa Bass.vst3" ]; then
    rm -rf "$VST3_DIR/Ariwa Bass.vst3"
    cp -R "$BUNDLE_DIR/Ariwa Bass.vst3" "$VST3_DIR/"
    echo "  [OK] VST3: $VST3_DIR/Ariwa Bass.vst3"
fi

if [ -d "$BUNDLE_DIR/Ariwa Bass.clap" ]; then
    rm -rf "$CLAP_DIR/Ariwa Bass.clap"
    cp -R "$BUNDLE_DIR/Ariwa Bass.clap" "$CLAP_DIR/"
    echo "  [OK] CLAP: $CLAP_DIR/Ariwa Bass.clap"
fi

if [ -d "$BUNDLE_DIR/Ariwa Bass.component" ]; then
    rm -rf "$AU_DIR/Ariwa Bass.component"
    cp -R "$BUNDLE_DIR/Ariwa Bass.component" "$AU_DIR/"
    echo "  [OK] AU:   $AU_DIR/Ariwa Bass.component"
fi

echo ""
echo "Installation complete! Restart your DAW (Ableton Live, Logic Pro, Reaper, Bitwig) to scan."
