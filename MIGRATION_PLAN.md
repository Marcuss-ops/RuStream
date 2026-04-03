# Legacy RustStream Migration Plan

**Analisi: Cosa serve realmente a RemoteCodex**

---

## 📊 Analisi Utilizzo Attuale

### RemoteCodex usa SOLO:
1. ✅ `ruststream probe` - Metadata extraction
2. ✅ `ruststream render` - Full video pipeline
3. ✅ `ruststream concat` - Video concatenation
4. ✅ `ruststream benchmark` - Performance testing
5. ✅ `ruststream info` - System info

### RemoteCodex NON usa:
- ❌ Headless rendering (browser-free rendering)
- ❌ Whisper transcription
- ❌ Advanced validation
- ❌ Image texture atlas
- ❌ Complex instrumentation
- ❌ CPU affinity features
- ❌ Quality gates advanced

---

## 📁 Stato Attuale File

| Directory | File .rs | Size | Status |
|-----------|----------|------|--------|
| `src-legacy-pyo3/` | 129 | 4.6 GB | ❌ Legacy PyO3 |
| `ruststream-core/src/` | 13 | 917 MB | ✅ 100% Rust |
| **Da migrare:** | ~40 | - | ⚠️ Needed |
| **Da eliminare:** | ~76 | - | ❌ Unused |

---

## 🎯 Moduli da Migrare (Necessari)

### 1. **core/render_graph/** (ESSENZIALE)
```
src-legacy-pyo3/core/render_graph/
├── graph.rs              → DA MIGRARE (input contract)
├── result.rs             → DA MIGRARE (output contract)
├── process.rs            → DA MIGRARE (pipeline execution)
├── config.rs             → DA MIGRARE (RenderConfig)
├── metrics.rs            → DA MIGRARE (RenderMetrics)
└── mod.rs
```

**Perché serve:** RemoteCodex chiama `ruststream render` che usa `process_render_graph()`

### 2. **core/audio_orchestrator.rs** (ESSENZIALE)
```
src-legacy-pyo3/core/audio_orchestrator.rs  → DA MIGRARE
```

**Perché serve:** Audio mixing con SIMD per voiceover + music

### 3. **core/media_pipeline.rs** (ESSENZIALE)
```
src-legacy-pyo3/core/media_pipeline.rs  → DA MIGRARE
```

**Perché serve:** Pipeline orchestration (validate → probe → decode → effects → overlay → audio → encode)

### 4. **core/timeline.rs** (ESSENZIALE)
```
src-legacy-pyo3/core/timeline.rs  → DA MIGRARE
```

**Perché serve:** MediaTimelinePlan per input rendering

### 5. **audio/hot_kernels/** (ESSENZIALE - GIÀ MIGRATO)
```
ruststream-core/src/audio/hot_kernels.rs  ✅ GIÀ PRESENTE
```

**Stato:** Già migrato con SIMD AVX2

### 6. **video/concat.rs** (ESSENZIALE - GIÀ MIGRATO)
```
ruststream-core/src/video/mod.rs  ✅ GIÀ PRESENTE
```

**Stato:** Già migrato

### 7. **probe/** (ESSENZIALE - GIÀ MIGRATO)
```
ruststream-core/src/probe/mod.rs  ✅ GIÀ PRESENTE
ruststream-core/src/probe/cache.rs  ✅ GIÀ PRESENTE
```

**Stato:** Già migrato

---

## ❌ Moduli da Eliminare (NON usati)

### 1. **headless/** (NON USATO)
```
src-legacy-pyo3/headless/
├── renderer.rs
├── schema.rs
└── mod.rs
```
**Motivo:** Browser-free rendering non serve a RemoteCodex

### 2. **whisper/** (NON USATO)
```
src-legacy-pyo3/whisper/
├── model.rs
└── mod.rs
```
**Motivo:** Transcription non è nel pipeline video

### 3. **image/advanced** (NON USATO)
```
src-legacy-pyo3/image/
├── texture_atlas.rs    → ELIMINARE
└── color_convert.rs    → MIGRARE (utile)
```

### 4. **validation/** (NON USATO)
```
src-legacy-pyo3/validation/
├── *.rs
```
**Motivo:** Validation base è già in render_graph

### 5. **core/instrumentation/** (NON USATO)
```
src-legacy-pyo3/core/instrumentation/
```
**Motivo:** Metrics base sono già in render_graph/metrics.rs

### 6. **core/quality_gates/** (NON USATO)
```
src-legacy-pyo3/core/quality_gates/
```
**Motivo:** Validation base è sufficiente

### 7. **core/cpu_affinity.rs** (NON USATO)
```
src-legacy-pyo3/core/cpu_affinity.rs
```
**Motivo:** CPU affinity automatica, non serve controllo manuale

### 8. **bindings/** (NON USATO - PyO3)
```
src-legacy-pyo3/bindings/
├── probe.rs
├── audio.rs
├── video.rs
└── ... (11 file PyO3)
```
**Motivo:** 100% Rust, zero Python bindings

---

## 🔄 Piano di Migrazione

### Fase 1: Migrare Render Graph (ESSENZIALE)
```bash
# Copia render_graph
cp -r src-legacy-pyo3/core/render_graph/ ruststream-core/src/core/render_graph/

# Rimuovi PyO3 da graph.rs, result.rs, process.rs
# Aggiungi error handling Rust-native

# Test
cargo build --release
./target/release/ruststream render --help
```

### Fase 2: Migrare Audio Orchestrator
```bash
# Copia audio_orchestrator.rs
cp src-legacy-pyo3/core/audio_orchestrator.rs ruststream-core/src/core/

# Rimuovi PyO3, usa redb per cache
# Integra con hot_kernels esistente

# Test
cargo test audio
```

### Fase 3: Migrare Media Pipeline
```bash
# Copia media_pipeline.rs e timeline.rs
cp src-legacy-pyo3/core/{media_pipeline.rs,timeline.rs} ruststream-core/src/core/

# Adatta per usare render_graph migrato

# Test
cargo test pipeline
```

### Fase 4: Pulizia Legacy
```bash
# Rimuovi moduli non usati
rm -rf src-legacy-pyo3/{headless,whisper,validation,bindings}
rm -rf src-legacy-pyo3/core/{instrumentation,quality_gates,cpu_affinity.rs}

# Verifica build
cargo build --release
```

---

## 📊 Timeline Stimata

| Fase | Moduli | Tempo Stimato | Priorità |
|------|--------|---------------|----------|
| 1 | render_graph/ | 2-3 ore | 🔴 CRITICA |
| 2 | audio_orchestrator.rs | 1-2 ore | 🔴 CRITICA |
| 3 | media_pipeline.rs, timeline.rs | 2-3 ore | 🟡 ALTA |
| 4 | Pulizia legacy | 1 ora | 🟢 MEDIA |

**Totale:** 6-9 ore di lavoro

---

## ✅ Criteri di Completamento

- [ ] `cargo build --release` compila senza errori
- [ ] `./target/release/ruststream render` funziona
- [ ] `./target/release/ruststream probe` funziona
- [ ] `./target/release/ruststream concat` funziona
- [ ] Zero dipendenze PyO3
- [ ] Zero riferimenti a headless/whisper
- [ ] Test passano: `cargo test`
- [ ] RemoteCodex integration test OK

---

## 🎯 Risultato Atteso

**Prima:**
- 129 file .rs legacy
- 4.6 GB di codice PyO3
- Confusione su cosa migrare

**Dopo:**
- ~50 file .rs essenziali
- ~500 MB di codice 100% Rust
- Struttura chiara e documentata
- RemoteCodex fully compatible

---

**Inizio migrazione:** Aprile 2026  
**Stima completamento:** 1 giorno  
**Responsabile:** VeloxEditing Team
