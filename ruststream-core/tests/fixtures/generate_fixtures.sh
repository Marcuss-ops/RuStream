#!/usr/bin/env bash
# generate_fixtures.sh
# Genera le fixture minimali per i test e benchmark di ruststream-core.
# Richiede FFmpeg nel PATH.
#
# Utilizzo:
#   cd ruststream-core
#   bash tests/fixtures/generate_fixtures.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${1:-$SCRIPT_DIR}"

if ! command -v ffmpeg &>/dev/null; then
    echo "ERROR: ffmpeg non trovato nel PATH." >&2
    exit 1
fi

echo "Generazione fixture in: $OUTPUT_DIR"

# ── Audio silence ──────────────────────────────────────────────────────────────
echo "[audio] silence_1s.wav"
ffmpeg -y -f lavfi -i "anullsrc=r=44100:cl=stereo" -t 1 \
    "$OUTPUT_DIR/silence_1s.wav" &>/dev/null

echo "[audio] silence_5s.wav"
ffmpeg -y -f lavfi -i "anullsrc=r=44100:cl=stereo" -t 5 \
    "$OUTPUT_DIR/silence_5s.wav" &>/dev/null

echo "[audio] silence_60s.wav"
ffmpeg -y -f lavfi -i "anullsrc=r=44100:cl=stereo" -t 60 \
    "$OUTPUT_DIR/silence_60s.wav" &>/dev/null

# ── Video black H.264 ──────────────────────────────────────────────────────────
VIDEO_ARGS=(-f lavfi -i "color=black:s=640x360:r=30"
    -c:v libx264 -crf 28 -pix_fmt yuv420p -movflags +faststart)

echo "[video] black_1s_h264.mp4"
ffmpeg -y "${VIDEO_ARGS[@]}" -t 1  "$OUTPUT_DIR/black_1s_h264.mp4"  &>/dev/null

echo "[video] black_10s_h264.mp4"
ffmpeg -y "${VIDEO_ARGS[@]}" -t 10 "$OUTPUT_DIR/black_10s_h264.mp4" &>/dev/null

echo "[video] black_1s_compat_a.mp4"
ffmpeg -y "${VIDEO_ARGS[@]}" -t 1  "$OUTPUT_DIR/black_1s_compat_a.mp4" &>/dev/null

echo "[video] black_1s_compat_b.mp4"
ffmpeg -y "${VIDEO_ARGS[@]}" -t 1  "$OUTPUT_DIR/black_1s_compat_b.mp4" &>/dev/null

# ── Invalid binary ─────────────────────────────────────────────────────────────
echo "[invalid] invalid.bin"
printf '\x00\x01\x02\x03\xff\xfe\xfd\xfc' > "$OUTPUT_DIR/invalid.bin"

# ── Report ─────────────────────────────────────────────────────────────────────
echo ""
echo "✓ Fixture generate:"
for f in "$OUTPUT_DIR"/*.{wav,mp4,bin}; do
    [ -f "$f" ] || continue
    kb=$(du -k "$f" | cut -f1)
    printf "  %-35s %8s KB\n" "$(basename "$f")" "$kb"
done

echo ""
echo "Ora puoi eseguire:"
echo "  cargo test --features real_media"
echo "  cargo bench"
