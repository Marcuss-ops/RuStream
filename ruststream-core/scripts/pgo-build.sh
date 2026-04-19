#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# PGO (Profile-Guided Optimization) build script for ruststream-core (Linux)
# Usage: bash scripts/pgo-build.sh [-- <workload-args>]
#
# Requires: rustup, llvm-tools-preview component, a representative workload.
# Install component once with: rustup component add llvm-tools-preview
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

PROFDATA_DIR="${TMPDIR:-/tmp}/pgo-data-ruststream"
MERGED="${PROFDATA_DIR}/merged.profdata"
BINARY="./target/release/ruststream"

echo "╔══════════════════════════════════════════════════╗"
echo "║  ruststream-core PGO build (Linux)               ║"
echo "╚══════════════════════════════════════════════════╝"

# ── Step 1: clean slate ───────────────────────────────────────────────────────
echo ""
echo "▶ Step 1/4 — Cleaning previous profdata..."
rm -rf "${PROFDATA_DIR}"
mkdir -p "${PROFDATA_DIR}"

# ── Step 2: instrumented build ────────────────────────────────────────────────
echo ""
echo "▶ Step 2/4 — Building instrumented binary (release-pgo profile)..."
export RUSTFLAGS="-Cprofile-generate=${PROFDATA_DIR}"
cargo build --profile=release-pgo
unset RUSTFLAGS
echo "   Binary: ./target/release-pgo/ruststream"

# ── Step 3: collect profile data ─────────────────────────────────────────────
echo ""
echo "▶ Step 3/4 — Running workload to collect profiles..."
echo "   ⚠  Run the instrumented binary with a REPRESENTATIVE workload now."
echo "   The workload should exercise: audio_mix, fused_concat, probe_cached."
echo ""
echo "   Example:"
echo "     ./target/release-pgo/ruststream --probe path/to/file.mp4"
echo "     ./target/release-pgo/ruststream --concat a.mp4 b.mp4 -o out.mp4"
echo ""

# If arguments were passed, run the instrumented binary directly.
WORKLOAD_ARGS=("$@")
if [[ ${#WORKLOAD_ARGS[@]} -gt 0 ]]; then
    echo "   Running: ./target/release-pgo/ruststream ${WORKLOAD_ARGS[*]}"
    LLVM_PROFILE_FILE="${PROFDATA_DIR}/ruststream-%p-%m.profraw" \
        ./target/release-pgo/ruststream "${WORKLOAD_ARGS[@]}"
else
    echo "   No workload args provided. Run manually, then re-run this script with --skip-run."
    echo "   Waiting for profraw files in ${PROFDATA_DIR}..."
    read -r -p "   Press ENTER when done."
fi

# Count collected profiles
PROFRAW_COUNT=$(find "${PROFDATA_DIR}" -name '*.profraw' | wc -l)
if [[ "${PROFRAW_COUNT}" -eq 0 ]]; then
    echo "❌ No .profraw files found in ${PROFDATA_DIR}. Did the workload run?"
    exit 1
fi
echo "   ✓ ${PROFRAW_COUNT} profraw file(s) collected."

# ── Step 4: merge profiles + optimised build ──────────────────────────────────
echo ""
echo "▶ Step 4/4 — Merging profiles and rebuilding with PGO..."

# Find llvm-profdata (provided by rustup component llvm-tools-preview)
LLVM_PROFDATA=$(find "${HOME}/.rustup/toolchains" -name "llvm-profdata" 2>/dev/null | head -1)
if [[ -z "${LLVM_PROFDATA}" ]]; then
    echo "❌ llvm-profdata not found. Install with:"
    echo "   rustup component add llvm-tools-preview"
    exit 1
fi

"${LLVM_PROFDATA}" merge \
    --output="${MERGED}" \
    "${PROFDATA_DIR}"/*.profraw
echo "   ✓ Profiles merged → ${MERGED}"

export RUSTFLAGS="-Cprofile-use=${MERGED} -Cllvm-args=-pgo-warn-missing-function"
cargo build --release
unset RUSTFLAGS

echo ""
echo "✅ PGO build complete!"
echo "   Binary: ${BINARY}"
echo "   Expected speedup: +10–20% on hot paths (audio_mix, probe, fused_concat)"
