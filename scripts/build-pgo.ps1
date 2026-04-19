# build-pgo.ps1
# PGO (Profile-Guided Optimization) build script for RuStream on Windows.
#
# Equivalent to scripts/build-pgo.sh for Linux.
# Requires: Rust nightly or stable >= 1.75, LLVM toolchain (llvm-profdata in PATH).
#
# Usage (from ruststream-core/ directory):
#   ..\scripts\build-pgo.ps1
#
# Output:
#   target\release\ruststream.exe  — PGO-optimised binary

param(
    [string]$WorkloadDir  = "tests\fixtures",
    [string]$PgoDataDir   = "pgo-data",
    [switch]$SkipGenerate = $false  # set to skip step 1+2 and re-use existing .profraw
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Crate = "ruststream"
$Bin   = "target\release\$Crate.exe"

function Require-Tool([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Write-Error "Required tool '$Name' not found in PATH."
        exit 1
    }
}

Require-Tool "cargo"
Require-Tool "llvm-profdata"

# ── Step 1: Instrumented build ────────────────────────────────────────────────
if (-not $SkipGenerate) {
    Write-Host "`n[PGO 1/4] Building instrumented binary..." -ForegroundColor Cyan

    $profrawDir = "$PgoDataDir\raw"
    New-Item -ItemType Directory -Force -Path $profrawDir | Out-Null

    $env:RUSTFLAGS = "-Cprofile-generate=$((Resolve-Path $profrawDir).Path)"
    cargo build --release --quiet
    $env:RUSTFLAGS = ""

    if (-not (Test-Path $Bin)) {
        Write-Error "Instrumented build failed — binary not found at $Bin."
        exit 1
    }
    Write-Host "  ✓ Instrumented binary: $Bin" -ForegroundColor Green

    # ── Step 2: Run workload to generate .profraw files ───────────────────────
    Write-Host "`n[PGO 2/4] Running workload to collect profiling data..." -ForegroundColor Cyan

    $fixtures = @(
        "black_1s_h264.mp4",
        "black_10s_h264.mp4",
        "silence_1s.wav",
        "silence_5s.wav"
    )

    $ran = 0
    foreach ($f in $fixtures) {
        $fp = Join-Path $WorkloadDir $f
        if (Test-Path $fp) {
            Write-Host "  probe $fp"
            & ".\$Bin" probe $fp 2>&1 | Out-Null
            $ran++
        }
    }

    if ($ran -eq 0) {
        Write-Warning "No fixture files found in '$WorkloadDir'. Run generate_fixtures.ps1 first."
        Write-Warning "PGO will proceed but the profile will be weak."
    } else {
        Write-Host "  ✓ Workload ran on $ran fixture(s)" -ForegroundColor Green
    }

    # Extra runs for warm-path coverage
    foreach ($f in $fixtures) {
        $fp = Join-Path $WorkloadDir $f
        if (Test-Path $fp) {
            & ".\$Bin" probe $fp 2>&1 | Out-Null
        }
    }
}

# ── Step 3: Merge .profraw → merged.profdata ──────────────────────────────────
Write-Host "`n[PGO 3/4] Merging profile data..." -ForegroundColor Cyan

$profrawDir  = "$PgoDataDir\raw"
$mergedFile  = "$PgoDataDir\merged.profdata"

$rawFiles = Get-ChildItem -Path $profrawDir -Filter "*.profraw" -ErrorAction SilentlyContinue
if (-not $rawFiles) {
    Write-Error "No .profraw files found in '$profrawDir'. Run without -SkipGenerate."
    exit 1
}

llvm-profdata merge `
    --output="$((Resolve-Path $profrawDir).Path)\merged.profdata" `
    (Get-ChildItem "$profrawDir\*.profraw" | ForEach-Object { $_.FullName })

# llvm-profdata writes to the dir; move to expected location
$mergedSource = "$profrawDir\merged.profdata"
if (Test-Path $mergedSource) {
    Move-Item -Force $mergedSource $mergedFile
}

Write-Host "  ✓ Merged profile: $mergedFile" -ForegroundColor Green

# ── Step 4: PGO-optimised build ───────────────────────────────────────────────
Write-Host "`n[PGO 4/4] Building PGO-optimised binary..." -ForegroundColor Cyan

$env:RUSTFLAGS = "-Cprofile-use=$((Resolve-Path $mergedFile).Path) -Cllvm-args=-pgo-warn-missing-function"
cargo build --release --quiet
$env:RUSTFLAGS = ""

Write-Host ""
Write-Host "✓ PGO build complete!" -ForegroundColor Green
Write-Host "  Binary: $(Resolve-Path $Bin)" -ForegroundColor Green

$sizeMB = [math]::Round((Get-Item $Bin).Length / 1MB, 2)
Write-Host "  Size:   $sizeMB MB"

Write-Host ""
Write-Host "Tip: compare performance with a non-PGO release build using:" -ForegroundColor Cyan
Write-Host "  cargo bench"
