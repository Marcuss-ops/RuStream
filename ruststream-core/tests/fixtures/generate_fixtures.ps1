# generate_fixtures.ps1
# Genera le fixture minimali per i test e benchmark di ruststream-core.
# Richiede FFmpeg nel PATH.
#
# Utilizzo:
#   cd ruststream-core
#   .\tests\fixtures\generate_fixtures.ps1

param(
    [string]$OutputDir = "$PSScriptRoot"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Test-FFmpeg {
    try {
        $null = & ffmpeg -version 2>&1
        return $true
    } catch {
        return $false
    }
}

if (-not (Test-FFmpeg)) {
    Write-Error "FFmpeg non trovato nel PATH. Installalo e riprova."
    exit 1
}

Write-Host "Generazione fixture in: $OutputDir" -ForegroundColor Cyan

# ── Audio silence ──────────────────────────────────────────────────────────────
Write-Host "`n[audio] silence_1s.wav (44100 Hz stereo, 1s)" -ForegroundColor Yellow
& ffmpeg -y -f lavfi -i "anullsrc=r=44100:cl=stereo" -t 1 `
    "$OutputDir\silence_1s.wav" 2>&1 | Out-Null

Write-Host "[audio] silence_5s.wav (44100 Hz stereo, 5s)" -ForegroundColor Yellow
& ffmpeg -y -f lavfi -i "anullsrc=r=44100:cl=stereo" -t 5 `
    "$OutputDir\silence_5s.wav" 2>&1 | Out-Null

Write-Host "[audio] silence_60s.wav (44100 Hz stereo, 60s)" -ForegroundColor Yellow
& ffmpeg -y -f lavfi -i "anullsrc=r=44100:cl=stereo" -t 60 `
    "$OutputDir\silence_60s.wav" 2>&1 | Out-Null

# ── Video black H.264 ──────────────────────────────────────────────────────────
$videoArgs = @(
    "-f", "lavfi",
    "-i", "color=black:s=640x360:r=30",
    "-c:v", "libx264",
    "-crf", "28",
    "-pix_fmt", "yuv420p",
    "-movflags", "+faststart"
)

Write-Host "[video] black_1s_h264.mp4 (640x360 H.264, 1s)" -ForegroundColor Yellow
& ffmpeg -y @videoArgs -t 1 "$OutputDir\black_1s_h264.mp4" 2>&1 | Out-Null

Write-Host "[video] black_10s_h264.mp4 (640x360 H.264, 10s)" -ForegroundColor Yellow
& ffmpeg -y @videoArgs -t 10 "$OutputDir\black_10s_h264.mp4" 2>&1 | Out-Null

Write-Host "[video] black_1s_compat_a.mp4 (concat test clip A)" -ForegroundColor Yellow
& ffmpeg -y @videoArgs -t 1 "$OutputDir\black_1s_compat_a.mp4" 2>&1 | Out-Null

Write-Host "[video] black_1s_compat_b.mp4 (concat test clip B)" -ForegroundColor Yellow
& ffmpeg -y @videoArgs -t 1 "$OutputDir\black_1s_compat_b.mp4" 2>&1 | Out-Null

# ── Invalid binary ─────────────────────────────────────────────────────────────
Write-Host "[invalid] invalid.bin (file corrotto per test errori)" -ForegroundColor Yellow
[System.IO.File]::WriteAllBytes(
    "$OutputDir\invalid.bin",
    [byte[]](0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD, 0xFC)
)

# ── Report ─────────────────────────────────────────────────────────────────────
Write-Host "`n✓ Fixture generate:" -ForegroundColor Green
Get-ChildItem $OutputDir -File | Where-Object { $_.Extension -in ".wav", ".mp4", ".bin" } |
    ForEach-Object {
        $kb = [math]::Round($_.Length / 1KB, 1)
        Write-Host ("  {0,-35} {1,8} KB" -f $_.Name, $kb)
    }

Write-Host "`nOra puoi eseguire:" -ForegroundColor Cyan
Write-Host "  cargo test --features real_media"
Write-Host "  cargo bench"
