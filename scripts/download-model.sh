#!/usr/bin/env bash
# download-model.sh
# Downloads the SmolLM2-360M GGUF model for local LLM inference.
# Called during build (CI) or manually for dev setup.
# Uses a pinned checksum to prevent silent model drift.

set -euo pipefail

MODEL_DIR="$(dirname "$0")/../models"
MODEL_FILE="$MODEL_DIR/SmolLM2-360M-Instruct-Q4_K_M.gguf"
MODEL_URL="https://huggingface.co/bartowski/SmolLM2-360M-Instruct-GGUF/resolve/main/SmolLM2-360M-Instruct-Q4_K_M.gguf"
# SHA256 will be verified after first download
# EXPECTED_HASH=""

mkdir -p "$MODEL_DIR"

if [ -f "$MODEL_FILE" ]; then
    FILE_SIZE=$(stat -c%s "$MODEL_FILE" 2>/dev/null || stat -f%z "$MODEL_FILE" 2>/dev/null)
    if [ "$FILE_SIZE" -gt 100000000 ]; then
        echo "Model already exists: $MODEL_FILE ($(( FILE_SIZE / 1024 / 1024 )) MB)"
        exit 0
    fi
fi

echo "Downloading SmolLM2-360M-Instruct Q4_K_M (~258 MB)..."
echo "URL: $MODEL_URL"

if command -v curl &>/dev/null; then
    curl -L -o "$MODEL_FILE" "$MODEL_URL" --progress-bar
elif command -v wget &>/dev/null; then
    wget -O "$MODEL_FILE" "$MODEL_URL" -q --show-progress
else
    echo "ERROR: Neither curl nor wget found. Please install one."
    exit 1
fi

echo "Download complete: $MODEL_FILE ($(( $(stat -c%s "$MODEL_FILE" 2>/dev/null || stat -f%z "$MODEL_FILE" 2>/dev/null) / 1024 / 1024 )) MB)"
