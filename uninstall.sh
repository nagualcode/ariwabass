#!/usr/bin/env bash
set -euo pipefail

echo "============================================"
echo " Uninstalling Ariwa Bass (macOS Plugin Suite)"
echo "============================================"

VST3_DIR="$HOME/Library/Audio/Plug-Ins/VST3"
CLAP_DIR="$HOME/Library/Audio/Plug-Ins/CLAP"
AU_DIR="$HOME/Library/Audio/Plug-Ins/Components"

removed=0

if [ -d "$VST3_DIR/Ariwa Bass.vst3" ]; then
    rm -rf "$VST3_DIR/Ariwa Bass.vst3"
    echo "  [REMOVED] $VST3_DIR/Ariwa Bass.vst3"
    removed=1
fi

if [ -d "$CLAP_DIR/Ariwa Bass.clap" ]; then
    rm -rf "$CLAP_DIR/Ariwa Bass.clap"
    echo "  [REMOVED] $CLAP_DIR/Ariwa Bass.clap"
    removed=1
fi

if [ -d "$AU_DIR/Ariwa Bass.component" ]; then
    rm -rf "$AU_DIR/Ariwa Bass.component"
    echo "  [REMOVED] $AU_DIR/Ariwa Bass.component"
    removed=1
fi

if [ "$removed" -eq 0 ]; then
    echo "No installed Ariwa Bass plugins were found."
else
    echo ""
    echo "Ariwa Bass was successfully uninstalled from your user library."
fi
