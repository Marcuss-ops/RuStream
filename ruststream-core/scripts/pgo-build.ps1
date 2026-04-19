# ─────────────────────────────────────────────────────────────────────────────
# PGO (Profile-Guided Optimization) build script for ruststream-core (Windows)
# Run from the ruststream-core directory in PowerShell:
#   .\scripts\pgo-build.ps1
# Or with a workload:
#   .\scripts\pgo-build.ps1 -WorkloadArgs "--probe C:\media\video.mp4"
#
# Requires: rustup, llvm-tools-preview component
# Install once: rustup component add llvm-tools-preview
# ─────────────────────────────────────────────────────────────────────────────
param(
    [string]$WorkloadArgs = "",
    [switch]$SkipRun
)

$ErrorActionPreference = "Stop"

$ProfDataDir = "C:\tmp\pgo-data-ruststream"
$Merged      = "$ProfDataDir\merged.profdata"
$Binary      = ".\target\release\ruststream.exe"

Write-Host "╔══════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  ruststream-core PGO build (Windows PowerShell)  ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════╝" -ForegroundColor Cyan

# ── Step 1: clean ─────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "▶ Step 1/4 — Cleaning previous profdata..." -ForegroundColor Yellow
if (Test-Path $ProfDataDir) { Remove-Item -Recurse -Force $ProfDataDir }
New-Item -ItemType Directory -Path $ProfDataDir | Out-Null

# ── Step 2: instrumented build ────────────────────────────────────────────────
Write-Host ""
Write-Host "▶ Step 2/4 — Building instrumented binary (release-pgo profile)..." -ForegroundColor Yellow
$env:RUSTFLAGS = "-Cprofile-generate=$ProfDataDir"
cargo build --profile=release-pgo
Remove-Item Env:\RUSTFLAGS
Write-Host "   Binary: .\target\release-pgo\ruststream.exe"

# ── Step 3: collect profile data ─────────────────────────────────────────────
Write-Host ""
Write-Host "▶ Step 3/4 — Running workload to collect profiles..." -ForegroundColor Yellow

if (-not $SkipRun) {
    if ($WorkloadArgs -ne "") {
        # Run with user-provided args
        Write-Host "   Running: .\target\release-pgo\ruststream.exe $WorkloadArgs"
        $env:LLVM_PROFILE_FILE = "$ProfDataDir\ruststream-%p-%m.profraw"
        $argList = $WorkloadArgs -split ' '
        & ".\target\release-pgo\ruststream.exe" @argList
        Remove-Item Env:\LLVM_PROFILE_FILE
    } else {
        Write-Host "   ⚠  No -WorkloadArgs provided. Run the instrumented binary manually:" -ForegroundColor DarkYellow
        Write-Host "      `$env:LLVM_PROFILE_FILE = '$ProfDataDir\ruststream-%p-%m.profraw'"
        Write-Host "      .\target\release-pgo\ruststream.exe --probe C:\path\to\video.mp4"
        Write-Host "      .\target\release-pgo\ruststream.exe --concat a.mp4 b.mp4 -o out.mp4"
        Write-Host ""
        Read-Host "   Press ENTER when done (profraw files collected)"
    }
} else {
    Write-Host "   -SkipRun: assuming profraw files already collected."
}

$profrawFiles = Get-ChildItem "$ProfDataDir\*.profraw" -ErrorAction SilentlyContinue
if (-not $profrawFiles) {
    Write-Host "❌ No .profraw files found in $ProfDataDir" -ForegroundColor Red
    exit 1
}
Write-Host "   ✓ $($profrawFiles.Count) profraw file(s) collected."

# ── Step 4: merge profiles + PGO build ───────────────────────────────────────
Write-Host ""
Write-Host "▶ Step 4/4 — Merging profiles and rebuilding with PGO..." -ForegroundColor Yellow

# Find llvm-profdata from rustup toolchain
$llvmProfdata = Get-ChildItem "$env:USERPROFILE\.rustup\toolchains" -Recurse -Filter "llvm-profdata.exe" -ErrorAction SilentlyContinue |
    Select-Object -First 1 -ExpandProperty FullName

if (-not $llvmProfdata) {
    Write-Host "❌ llvm-profdata.exe not found. Install with:" -ForegroundColor Red
    Write-Host "   rustup component add llvm-tools-preview"
    exit 1
}

Write-Host "   Using: $llvmProfdata"
& $llvmProfdata merge --output=$Merged "$ProfDataDir\*.profraw"
Write-Host "   ✓ Profiles merged → $Merged"

$env:RUSTFLAGS = "-Cprofile-use=$Merged -Cllvm-args=-pgo-warn-missing-function"
cargo build --release
Remove-Item Env:\RUSTFLAGS

Write-Host ""
Write-Host "✅ PGO build complete!" -ForegroundColor Green
Write-Host "   Binary: $Binary"
Write-Host "   Expected speedup: +10-20% on hot paths (audio_mix, probe, fused_concat)" -ForegroundColor Green
