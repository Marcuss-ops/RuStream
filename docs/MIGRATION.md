# Migration Guide - Python a RustStream 100% Rust

**Come migrare da `velox_native` PyO3 a RustStream standalone**

---

## 📋 Panoramica

| Prima (PyO3) | Dopo (100% Rust) |
|--------------|------------------|
| `import velox_native` | `./velox` CLI o HTTP API |
| Python 3.10+ | Nessun runtime necessario |
| `pip install velox_native` | Download binary |
| Virtual environment | Standalone executable |
| 100 MB RAM | 25 MB RAM |

---

## 🔄 Migration Patterns

### 1. Probe Metadata

#### Prima (Python)
```python
import velox_native

cache = velox_native.MediaCache.open_default()
metadata = velox_native.probe_full_with_cache("video.mp4", cache)

print(f"Duration: {metadata.video.duration_secs}")
print(f"Resolution: {metadata.video.width}x{metadata.video.height}")
```

#### Dopo (CLI)
```bash
# Output JSON
velox probe video.mp4 --json > metadata.json

# O in script bash
DURATION=$(velox probe video.mp4 --json | jq '.video.duration_secs')
WIDTH=$(velox probe video.mp4 --json | jq '.video.width')
```

#### Dopo (HTTP API)
```python
import requests

response = requests.post(
    "http://localhost:8080/probe",
    json={"path": "video.mp4"}
)
metadata = response.json()

print(f"Duration: {metadata['metadata']['video']['duration_secs']}")
```

#### Dopo (Rust library)
```rust
use velox_native::{probe, MediaCache};

let cache = MediaCache::open_default()?;
let metadata = probe::probe_full_with_cache("video.mp4", &cache)?;

println!("Duration: {}", metadata.video.duration_secs);
```

---

### 2. Render Timeline

#### Prima (Python)
```python
import velox_native

timeline_json = '''
{
    "video_tracks": [
        {"clips": [{"path": "clip1.mp4", "start": 0, "duration": 5}]}
    ]
}
'''

graph = velox_native.RenderGraph.new(
    graph_id="job-123",
    timeline_json=timeline_json,
    audio_json="{}",
    config=velox_native.RenderConfig(mode="normal")
)

result = velox_native.process_render_graph(graph)
print(f"Output: {result.artifact_path}")
```

#### Dopo (CLI)
```bash
# Crea file timeline.json
cat > timeline.json << 'EOF'
{
    "video_tracks": [
        {"clips": [{"path": "clip1.mp4", "start": 0, "duration": 5}]}
    ]
}
EOF

# Esegui render
velox render --input timeline.json --output output.mp4 --config config.toml

# Check risultato
if [ $? -eq 0 ]; then
    echo "Render completato!"
else
    echo "Render fallito!"
fi
```

#### Dopo (HTTP API)
```python
import requests

timeline = {
    "video_tracks": [
        {"clips": [{"path": "clip1.mp4", "start": 0, "duration": 5}]}
    ]
}

response = requests.post(
    "http://localhost:8080/render",
    json={
        "timeline": timeline,
        "config": {"preset": "ultrafast", "crf": 23}
    }
)

result = response.json()
print(f"Job ID: {result['job_id']}")
print(f"Status: {result['status']}")

# Poll per completamento
while result['status'] == 'processing':
    time.sleep(1)
    response = requests.get(f"http://localhost:8080/jobs/{result['job_id']}")
    result = response.json()

print(f"Output: {result['output_path']}")
```

---

### 3. Concat Video

#### Prima (Python)
```python
import velox_native

config = velox_native.ConcatConfig(
    inputs=["clip1.mp4", "clip2.mp4", "clip3.mp4"],
    output="merged.mp4"
)

success = velox_native.concat_videos(config)
if success:
    print("Concat completato!")
```

#### Dopo (CLI)
```bash
# Concatena video
velox concat clip1.mp4 clip2.mp4 clip3.mp4 --output merged.mp4

# Check risultato
if [ $? -eq 0 ]; then
    echo "Concat completato!"
else
    echo "Concat fallito!"
fi
```

#### Dopo (HTTP API)
```python
import requests

response = requests.post(
    "http://localhost:8080/concat",
    json={
        "inputs": ["clip1.mp4", "clip2.mp4", "clip3.mp4"],
        "output": "merged.mp4"
    }
)

result = response.json()
if result['status'] == 'completed':
    print(f"Concat completato: {result['output_path']}")
```

---

### 4. Audio Processing

#### Prima (Python)
```python
import velox_native

config = velox_native.AudioBakeConfig(
    input_path="audio.wav",
    output_path="output.aac",
    sample_rate=48000,
    channels=2,
    gate_ranges=[{"start": 0, "end": 100, "mute": True}]
)

result = velox_native.bake_audio(config)
```

#### Dopo (CLI)
```bash
# Crea config audio
cat > audio.json << 'EOF'
{
    "input_path": "audio.wav",
    "output_path": "output.aac",
    "sample_rate": 48000,
    "channels": 2,
    "gate_ranges": [{"start": 0, "end": 100, "mute": true}]
}
EOF

# Esegui audio processing
velox render --input audio.json --output output.aac
```

---

### 5. Subtitle Generation

#### Prima (Python)
```python
import velox_native

styles = [
    velox_native.AssStyle(
        name="Default",
        fontname="Arial",
        fontsize=24,
        primary_color=&H00FFFFFF,
    )
]

events = [
    velox_native.AssEvent(
        start=0.5,
        end=3.0,
        text="Hello, World!"
    )
]

ass_content = velox_native.generate_ass(styles, events)
```

#### Dopo (CLI)
```bash
# Crea file ASS manualmente o via API
cat > subtitles.ass << 'EOF'
[Script Info]
Title: Example Subtitles

[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour
Style: Default,Arial,24,&H00FFFFFF

[Events]
Format: Start, End, Text
Dialogue: 0:00:00.50,0:00:03.00,Default,,Hello, World!
EOF

# Usa nel render
velox render --input timeline.json --output output.mp4
```

---

## 🛠️ Script di Migration Automatica

### Python Wrapper per Transizione

```python
#!/usr/bin/env python3
"""
Wrapper per transizione graduale da velox_native PyO3 a RustStream CLI.
"""

import subprocess
import json
from pathlib import Path
from typing import Optional, Dict, Any

class RustStreamWrapper:
    """Wrapper compatibile con velox_native API."""
    
    def __init__(self, velox_path: str = "velox"):
        self.velox_path = velox_path
    
    def probe(self, path: str) -> Dict[str, Any]:
        """Probe metadata (compatibile con velox_native.probe_full)."""
        result = subprocess.run(
            [self.velox_path, "probe", path, "--json"],
            capture_output=True,
            text=True
        )
        
        if result.returncode != 0:
            raise RuntimeError(f"Probe failed: {result.stderr}")
        
        return json.loads(result.stdout)
    
    def render(self, timeline_json: str, output_path: str, **kwargs) -> Dict[str, Any]:
        """Render timeline (compatibile con velox_native.process_render_graph)."""
        # Scrivi timeline su file temporaneo
        timeline_file = Path("/tmp/timeline.json")
        timeline_file.write_text(timeline_json)
        
        cmd = [
            self.velox_path, "render",
            "--input", str(timeline_file),
            "--output", output_path
        ]
        
        if "config" in kwargs:
            config_file = Path("/tmp/config.toml")
            config_file.write_text(kwargs["config"])
            cmd.extend(["--config", str(config_file)])
        
        result = subprocess.run(cmd, capture_output=True, text=True)
        
        if result.returncode != 0:
            return {
                "success": False,
                "error": result.stderr
            }
        
        return {
            "success": True,
            "output_path": output_path
        }
    
    def concat(self, inputs: list, output: str) -> bool:
        """Concat videos (compatibile con velox_native.concat_videos)."""
        cmd = [self.velox_path, "concat"] + inputs + ["--output", output]
        result = subprocess.run(cmd, capture_output=True, text=True)
        return result.returncode == 0


# Utilizzo
if __name__ == "__main__":
    velox = RustStreamWrapper()
    
    # Probe
    metadata = velox.probe("video.mp4")
    print(f"Duration: {metadata['video']['duration_secs']}")
    
    # Concat
    success = velox.concat(["a.mp4", "b.mp4"], "merged.mp4")
    print(f"Concat: {'OK' if success else 'FAIL'}")
```

---

## 📊 Timeline di Migration

### Settimana 1-2: Dual Mode

```bash
# Mantieni entrambi i sistemi
├── velox_native (PyO3) - per clients esistenti
└── velox (CLI) - per nuovi clients

# Feature flag
VELOX_USE_RUST=1  # Clients opt-in per nuovo sistema
```

### Settimana 3-4: Migration Graduale

```bash
# Migra clients uno per uno
for client in clients/*.py; do
    if migrate_to_rust "$client"; then
        echo "✓ $client migrato"
    else
        echo "✗ $client fallito"
    fi
done

# Monitora metriche
# - Error rate
# - Latency P99
# - RAM usage
```

### Settimana 5: Deprecazione

```bash
# Annuncia deprecazione
# - Email a tutti i clients
# - Documentation update
# - 30 giorni grace period

# Dopo 30 giorni
rm -rf velox_native/  # Rimuovi PyO3
```

---

## ✅ Migration Checklist

### Pre-Migration

- [ ] Backup sistema esistente
- [ ] Test suite passing
- [ ] Documentazione API corrente
- [ ] Lista di tutti i clients

### Durante Migration

- [ ] Installa RustStream binary
- [ ] Configura HTTP API server
- [ ] Migra 1 client di test
- [ ] Verifica funzionalità
- [ ] Benchmark performance

### Post-Migration

- [ ] Tutti i clients migrati
- [ ] Performance migliorate
- [ ] RAM usage ridotto
- [ ] PyO3 rimosso
- [ ] Documentazione aggiornata

---

## 🐛 Troubleshooting

### "Command not found: velox"

```bash
# Aggiungi a PATH
export PATH=$PATH:/usr/local/bin

# O usa path assoluto
/opt/velox/velox probe video.mp4
```

### "JSON parsing error"

```bash
# Verifica formato JSON
velox probe video.mp4 --json | jq .

# Se fallisce, check encoding
file video.mp4
```

### "HTTP 500 error"

```bash
# Check server logs
journalctl -u velox -f

# Check health
curl http://localhost:8080/health
```

---

## 📞 Support

- **Migration Issues**: https://github.com/VeloxEditing/RustStream/issues
- **Discussioni**: https://github.com/VeloxEditing/RustStream/discussions
- **Documentation**: https://github.com/VeloxEditing/RustStream/tree/main/docs

---

**Good luck con la migration! 🚀**

*La migration completa richiede 2-4 settimane per la maggior parte dei progetti.*
