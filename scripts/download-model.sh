#!/usr/bin/env bash
# download-model.sh
# Downloads the local LLM GGUF for offline title generation.
# Called during build (CI) or manually for dev setup.
# Uses a pinned checksum to prevent silent model drift.
#
# Usage:
#   bash scripts/download-model.sh            # SmolLM2-360M (bundled fallback, 258 MB)
#   bash scripts/download-model.sh qwen       # Qwen2.5-1.5B (primary engine, 940 MB)
#
# Qwen2.5-1.5B is the production engine but is NOT bundled in the installer
# yet (gated on bundling decisions). CI uses `qwen` to VERIFY cross-platform
# build + runtime without shipping it.

set -euo pipefail

MODEL_DIR="$(dirname "$0")/../models"
mkdir -p "$MODEL_DIR"

# ── Model definitions ──
# Qwen2.5-1.5B-Instruct Q4_K_M from bartowski's GGUF quantisation of the
# Apache-2.0 Qwen2.5 weights. Hash pinned from the HF LFS pointer, verified
# against the local model file 2026-07-31.
declare -A MODEL_URL=(
  [smollm2]="https://huggingface.co/bartowski/SmolLM2-360M-Instruct-GGUF/resolve/main/SmolLM2-360M-Instruct-Q4_K_M.gguf"
  [qwen]="https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf"
)
declare -A MODEL_FILE=(
  [smollm2]="SmolLM2-360M-Instruct-Q4_K_M.gguf"
  [qwen]="qwen2.5-1.5b-instruct-q4_k_m.gguf"
)
declare -A MODEL_HASH=(
  [smollm2]="2fa3f013dcdd7b99f9b237717fa0b12d75bbb89984cc1274be1471a465bac9c2"
  [qwen]="1adf0b11065d8ad2e8123ea110d1ec956dab4ab038eab665614adba04b6c3370"
)
declare -A MODEL_SIZE=(
  [smollm2]=270590880
  [qwen]=986048768
)
declare -A MODEL_LABEL=(
  [smollm2]="SmolLM2-360M-Instruct Q4_K_M (~258 MB)"
  [qwen]="Qwen2.5-1.5B-Instruct Q4_K_M (~940 MB)"
)

WHICH="${1:-smollm2}"
if [ -z "${MODEL_URL[$WHICH]+_}" ]; then
  echo "ERROR: unknown model '$WHICH'. Choose: smollm2 | qwen"
  exit 1
fi

MODEL_FILE="$MODEL_DIR/${MODEL_FILE[$WHICH]}"
MODEL_URL="${MODEL_URL[$WHICH]}"
EXPECTED_HASH="${MODEL_HASH[$WHICH]}"
EXPECTED_SIZE="${MODEL_SIZE[$WHICH]}"

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

echo "Downloading ${MODEL_LABEL[$WHICH]}..."
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
