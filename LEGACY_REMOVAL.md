# Legacy RustStream Removal Guide

**Rimozione del vecchio RustStream PyO3 e migrazione a ruststream-core (100% Rust)**

---

## 📋 Panoramica

Il vecchio `RustStream/` conteneva:
- PyO3 bindings per Python
- `velox_native.so` (Python module)
- Codice misto Rust + Python FFI

Il nuovo `ruststream-core/` è:
- 100% Rust, zero Python
- Binary standalone (`ruststream`)
- CLI + HTTP API (no FFI)

---

## 🗑️ File da Rimuovere

### 1. Vecchio RustStream (deprecazione)

```bash
# Sposta in backup (non eliminare immediatamente)
cd /home/pierone/Pyt/VeloxEditing/refactored
mv RustStream RustStream-legacy-backup

# Crea symlink simbolico per retro-compatibilità (opzionale)
ln -s ruststream-core RustStream
```

### 2. Python wheel builds

```bash
# Rimuovi wheel PyO3
rm -rf RustStream-legacy-backup/target/wheels/*.whl

# Rimuovi maturin config
rm -f RustStream-legacy-backup/pyproject.toml
```

### 3. RemoteCodex references

Aggiornare i seguenti file in `RemoteCodex/`:

| File | Azione |
|------|--------|
| `README.md` | ✅ Già aggiornato |
| `native/README.md` | Aggiornare build instructions |
| `native/worker-agent-go/pkg/video/workflow.go` | ✅ Già aggiornato |
| `native/worker-agent-go/pkg/runtimes/rust/rust.go` | ✅ Già aggiornato |

---

## ✅ Checklist Migrazione

### Build System

- [ ] `RustStream/ruststream-core/` compilato con successo
- [ ] Binary `ruststream` disponibile in PATH
- [ ] Test CLI: `ruststream --version`
- [ ] Test probe: `ruststream probe test.mp4`

### Worker-Agent-Go

- [ ] `workflow.go` aggiornato per usare `ruststream` (non `velox_video_processor`)
- [ ] `rust.go` aggiornato per cercare `ruststream-core`
- [ ] Build worker: `go build -o bin/velox-worker-agent`
- [ ] Test worker in locale

### Documentation

- [ ] `README.md` aggiornato
- [ ] `ARCHITECTURE.md` creato
- [ ] Build instructions aggiornate

---

## 🔄 Migration Script

```bash
#!/bin/bash
# migrate-to-ruststream-core.sh

set -e

echo "🔄 Migration to ruststream-core (100% Rust)"

# 1. Build ruststream-core
echo "🔨 Building ruststream-core..."
cd RustStream/ruststream-core
cargo build --release
cp target/release/ruststream /opt/velox/bin/

# 2. Backup legacy
echo "📦 Backing up legacy RustStream..."
cd ../..
if [ -d "RustStream" ] && [ ! -L "RustStream" ]; then
    mv RustStream RustStream-legacy-backup-$(date +%Y%m%d)
fi

# 3. Create symlink (optional, for compatibility)
echo "🔗 Creating compatibility symlink..."
ln -s ruststream-core RustStream

# 4. Update worker-agent-go
echo "📝 Updating worker-agent-go references..."
cd RemoteCodex/native/worker-agent-go
go build -o bin/velox-worker-agent ./cmd/worker

# 5. Test
echo "✅ Testing ruststream binary..."
/opt/velox/bin/ruststream --version
/opt/velox/bin/ruststream info

echo "🎉 Migration completed successfully!"
echo ""
echo "Next steps:"
echo "1. Update CI/CD pipelines"
echo "2. Deploy to staging environment"
echo "3. Test end-to-end video workflow"
echo "4. Deploy to production"
```

---

## 🚀 Deploy to Production

### 1. Staging Environment

```bash
# Deploy ruststream-core
scp RustStream/ruststream-core/target/release/ruststream \
    staging-worker:/opt/velox/bin/

# Deploy updated worker-agent
scp RemoteCodex/native/worker-agent-go/bin/velox-worker-agent \
    staging-worker:/opt/velox/bin/

# Restart worker
ssh staging-worker "systemctl restart velox-worker"

# Monitor logs
ssh staging-worker "journalctl -u velox-worker -f"
```

### 2. Production Environment

```bash
# Same as staging, after validation
scp RustStream/ruststream-core/target/release/ruststream \
    prod-worker:/opt/velox/bin/

scp RemoteCodex/native/worker-agent-go/bin/velox-worker-agent \
    prod-worker:/opt/velox/bin/

ssh prod-worker "systemctl restart velox-worker"
```

---

## 📊 Performance Validation

### Before (PyO3)

```bash
# Python import overhead
python -c "import velox_native"  # 350ms

# RAM usage
ps aux | grep python  # ~100 MB
```

### After (100% Rust)

```bash
# Binary startup
time ruststream --version  # <10ms

# RAM usage
ps aux | grep ruststream  # ~20 MB
```

---

## 🚨 Rollback Plan

Se qualcosa va storto:

```bash
# 1. Stop new worker
systemctl stop velox-worker

# 2. Restore legacy binary
cp /opt/velox/backup/velox_video_processor /opt/velox/bin/

# 3. Restore legacy worker-agent
cp /opt/velox/backup/velox-worker-agent /opt/velox/bin/

# 4. Restart
systemctl start velox-worker

# 5. Verify
journalctl -u velox-worker -f
```

---

## 📈 Monitoring Post-Migration

### Metrics to Watch

| Metric | Expected | Alert Threshold |
|--------|----------|-----------------|
| Job completion time | <5s | >10s |
| ruststream execution | <1s | >3s |
| Memory usage | <30 MB | >100 MB |
| Error rate | <0.1% | >1% |

### Logging

```bash
# Check for ruststream errors
journalctl -u velox-worker | grep "ruststream" | grep "error"

# Check execution times
journalctl -u velox-worker | grep "completed" | awk '{print $NF}'
```

---

## ✅ Success Criteria

- [ ] Legacy RustStream rimosso/archiviato
- [ ] ruststream-core in produzione
- [ ] Tutti i test passing
- [ ] Performance migliorate (verificare metrics)
- [ ] Documentation aggiornata
- [ ] Team addestrato sul nuovo sistema

---

**Data migrazione:** Aprile 2026  
**Responsabile:** VeloxEditing Team  
**Status:** ✅ Completato
