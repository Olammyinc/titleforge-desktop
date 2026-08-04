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
#
# NOTE: macOS runners ship bash 3.2 (2007). NO associative arrays, NO
# ${var,,} — keep this POSIX-ish or the download step dies on macOS with
# "declare: -A: invalid option" while working fine on Linux/Windows.

set -euo pipefail

MODEL_DIR="$(dirname "$0")/../models"
mkdir -p "$MODEL_DIR"

WHICH="${1:-smollm2}"

case "$WHICH" in
  smollm2)
    MODEL_URL="https://huggingface.co/bartowski/SmolLM2-360M-Instruct-GGUF/resolve/main/SmolLM2-360M-Instruct-Q4_K_M.gguf"
    MODEL_FILE="$MODEL_DIR/SmolLM2-360M-Instruct-Q4_K_M.gguf"
    EXPECTED_HASH="2fa3f013dcdd7b99f9b237717fa0b12d75bbb89984cc1274be1471a465bac9c2"
    EXPECTED_SIZE=270590880
    LABEL="SmolLM2-360M-Instruct Q4_K_M (~258 MB)"
    ;;
  qwen)
    # Qwen2.5-1.5B-Instruct Q4_K_M from bartowski's GGUF quantisation of the
    # Apache-2.0 Qwen2.5 weights. Hash pinned from the HF LFS pointer, verified
    # against the local model file 2026-07-31.
    MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf"
    MODEL_FILE="$MODEL_DIR/qwen2.5-1.5b-instruct-q4_k_m.gguf"
    EXPECTED_HASH="1adf0b11065d8ad2e8123ea110d1ec956dab4ab038eab665614adba04b6c3370"
    EXPECTED_SIZE=986048768
    LABEL="Qwen2.5-1.5B-Instruct Q4_K_M (~940 MB)"
    ;;
  *)
    echo "ERROR: unknown model '$WHICH'. Choose: smollm2 | qwen"
    exit 1
    ;;
esac

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

echo "Downloading $LABEL..."
echo "URL: $MODEL_URL"

if command -v curl &>/dev/null; then
    # --retry 5 with backoff: HF CDN throttles large parallel downloads and
    # transient 5xx/connects are common on CI runners. --retry-all-errors
    # also covers 429/5xx that plain --retry skips by default.
    PART_FILE="$MODEL_FILE.part"
    rm -f "$PART_FILE"
    # --fail is essential: without it, an HTTP error page can be saved as a
    # successful 3 KB "model" and only fail later at the size check.
    if ! curl -L --fail -o "$PART_FILE" "$MODEL_URL" --progress-bar \
        --retry 5 --retry-delay 5 --retry-all-errors --connect-timeout 30; then
        rm -f "$PART_FILE"
        echo "ERROR: model download failed after retries."
        exit 1
    fi
    mv "$PART_FILE" "$MODEL_FILE"
elif command -v wget &>/dev/null; then
    PART_FILE="$MODEL_FILE.part"
    rm -f "$PART_FILE"
    if ! wget -O "$PART_FILE" "$MODEL_URL" -q --show-progress --tries=5; then
        rm -f "$PART_FILE"
        echo "ERROR: model download failed after retries."
        exit 1
    fi
    mv "$PART_FILE" "$MODEL_FILE"
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
