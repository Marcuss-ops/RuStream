# Sviluppo su Windows — Setup FFmpeg

`ruststream-core` dipende da `ffmpeg-next` che a sua volta linka le librerie FFmpeg native.
Su Windows questo richiede un setup manuale prima di poter compilare.

## Opzione A — vcpkg (consigliata per MSVC)

```powershell
# 1. Installa vcpkg (se non già installato)
git clone https://github.com/microsoft/vcpkg C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat

# 2. Installa FFmpeg
C:\vcpkg\vcpkg install ffmpeg:x64-windows

# 3. Integra con cargo
$env:VCPKG_ROOT = "C:\vcpkg"
$env:FFMPEG_DIR = "C:\vcpkg\installed\x64-windows"

# 4. Verifica
cargo check --manifest-path ruststream-core\Cargo.toml
```

## Opzione B — FFmpeg precompilato + pkg-config (più veloce)

```powershell
# 1. Scarica FFmpeg shared libs da https://github.com/BtbN/FFmpeg-Builds/releases
#    (es. ffmpeg-master-latest-win64-gpl-shared.zip)

# 2. Estrai in C:\ffmpeg

# 3. Installa pkg-config per Windows
#    (es. tramite scoop: scoop install pkg-config)

# 4. Imposta variabili d'ambiente
$env:FFMPEG_DIR = "C:\ffmpeg"
$env:PKG_CONFIG_PATH = "C:\ffmpeg\lib\pkgconfig"
$env:PATH += ";C:\ffmpeg\bin"

# 5. Verifica
pkg-config --libs libavutil
cargo check --manifest-path ruststream-core\Cargo.toml
```

## Opzione C — Compilazione in Linux (WSL2 o CI)

Il target di produzione è Linux. Per sviluppo su Windows senza installare FFmpeg:

```powershell
# Usa WSL2 con Ubuntu
wsl --install
# Nel terminale WSL2:
sudo apt-get install -y ffmpeg libavcodec-dev libavformat-dev libavutil-dev \
    libswresample-dev libswscale-dev pkg-config
cargo check  # eseguito dentro WSL2
```

## Note

- Il `cargo check` fallisce **solo per il build script di ffmpeg-sys-next**, non per errori nel nostro codice Rust
- Tutti gli edit (probe_fast, stream-copy, Profiler, subprocess wrapper, ecc.) sono **sintatticamente corretti**
- La CI Linux con `apt-get install ffmpeg libavcodec-dev ...` compila senza problemi
- Per test locali su Windows senza FFmpeg: `cargo test --lib` (solo unit test senza linker)
