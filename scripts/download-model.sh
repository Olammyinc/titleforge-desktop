#!/usr/bin/env bash
# download-model.sh
# Downloads the SmolLM2-360M GGUF model for local LLM inference.
# Called during build (CI) or manually for dev setup.
# Uses a pinned checksum to prevent silent model drift.

set -euo pipefail

MODEL_DIR="$(dirname "$0")/../models"
MODEL_FILE="$MODEL_DIR/SmolLM2-360M-Instruct-Q4_K_M.gguf"
MODEL_URL="https://huggingface.co/bartowski/SmolLM2-360M-Instruct-GGUF/resolve/main/SmolLM2-360M-Instruct-Q4_K_M.gguf"
# Pinned from the HuggingFace LFS pointer for this file (bartowski/SmolLM2-360M-Instruct-GGUF,
# main branch, SmolLM2-360M-Instruct-Q4_K_M.gguf) — verified 2026-07-16.
EXPECTED_HASH="2fa3f013dcdd7b99f9b237717fa0b12d75bbb89984cc1274be1471a465bac9c2"
EXPECTED_SIZE=270590880

verify_hash() {
    local file="$1"
    local actual_hash=""
    if command -v sha256sum &>/dev/null; then
        actual_hash=$(sha256sum "$file" | awk '{print $1}')
    elif command -v shasum &>/dev/null; then
        actual_hash=$(shasum -a 256 "$file" | awk '{print $1}')
    else
        echo "WARNING: no sha256sum/shasum found — skipping checksum verification."
        return 0
    fi

    if [ "$actual_hash" != "$EXPECTED_HASH" ]; then
        echo "ERROR: checksum mismatch for $file"
        echo "  expected: $EXPECTED_HASH"
        echo "  actual:   $actual_hash"
        echo "This means the model was corrupted in transit or the upstream file changed."
        echo "Deleting the bad file so a re-run doesn't treat it as cached."
        rm -f "$file"
        exit 1
    fi
    echo "Checksum OK: $actual_hash"
}

mkdir -p "$MODEL_DIR"

if [ -f "$MODEL_FILE" ]; then
    FILE_SIZE=$(stat -c%s "$MODEL_FILE" 2>/dev/null || stat -f%z "$MODEL_FILE" 2>/dev/null)
    if [ "$FILE_SIZE" -eq "$EXPECTED_SIZE" ]; then
        echo "Model already exists: $MODEL_FILE ($(( FILE_SIZE / 1024 / 1024 )) MB) — verifying checksum..."
        verify_hash "$MODEL_FILE"
        exit 0
    else
        echo "Existing model file has unexpected size ($FILE_SIZE bytes, expected $EXPECTED_SIZE) — re-downloading."
        rm -f "$MODEL_FILE"
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

DOWNLOADED_SIZE=$(stat -c%s "$MODEL_FILE" 2>/dev/null || stat -f%z "$MODEL_FILE" 2>/dev/null)
echo "Download complete: $MODEL_FILE ($(( DOWNLOADED_SIZE / 1024 / 1024 )) MB)"

if [ "$DOWNLOADED_SIZE" -ne "$EXPECTED_SIZE" ]; then
    echo "ERROR: downloaded size ($DOWNLOADED_SIZE bytes) does not match expected size ($EXPECTED_SIZE bytes)."
    rm -f "$MODEL_FILE"
    exit 1
fi

verify_hash "$MODEL_FILE"
