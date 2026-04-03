# Deployment Guide - RustStream

**Deploy standalone Rust binary - No Python, No dependencies**

## 📦 Quick Deploy

### Option 1: Pre-built Binary (Raccomandato)

```bash
# 1. Download
wget https://github.com/VeloxEditing/RustStream/releases/latest/download/velox-x86_64-unknown-linux-gnu.tar.gz

# 2. Extract
tar xzf velox-*.tar.gz

# 3. Move to PATH
sudo mv velox /usr/local/bin/

# 4. Verify
velox --version
```

### Option 2: Docker

```dockerfile
FROM scratch
COPY velox /velox
COPY config/velox.toml /etc/velox.toml
ENTRYPOINT ["/velox", "serve", "--config", "/etc/velox.toml"]
```

```bash
# Build
docker build -t velox:latest .

# Run
docker run -d -p 8080:8080 \
  -v /data:/data \
  -v /output:/output \
  velox:latest
```

### Option 3: Build from Source

```bash
# Prerequisiti
sudo apt-get update
sudo apt-get install -y \
  curl gcc pkg-config \
  libavcodec-dev libavformat-dev libavutil-dev \
  libavfilter-dev libavdevice-dev libswresample-dev \
  libswscale-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Build
git clone https://github.com/VeloxEditing/RustStream
cd RustStream
cargo build --release

# Install
sudo cp target/release/velox /usr/local/bin/
```

---

## 🚀 Production Deployment

### Systemd Service

Crea `/etc/systemd/system/velox.service`:

```ini
[Unit]
Description=RustStream Media Processing Engine
After=network.target

[Service]
Type=simple
User=velox
Group=velox
WorkingDirectory=/opt/velox
ExecStart=/usr/local/bin/velox serve --port 8080 --config /etc/velox.toml
Restart=always
RestartSec=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/velox/output /opt/velox/tmp

# Resource limits
MemoryMax=512M
CPUQuota=200%

[Install]
WantedBy=multi-user.target
```

```bash
# Abilita servizio
sudo useradd -r -s /bin/false velox
sudo mkdir -p /opt/velox/{output,tmp}
sudo chown -R velox:velox /opt/velox

sudo systemctl daemon-reload
sudo systemctl enable velox
sudo systemctl start velox
sudo systemctl status velox
```

### Nginx Reverse Proxy

```nginx
server {
    listen 80;
    server_name api.yourdomain.com;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_cache_bypass $http_upgrade;
        proxy_read_timeout 300s;
        proxy_connect_timeout 75s;
    }

    # Rate limiting
    limit_req zone=api burst=20 nodelay;
}

# Rate limit zone
http {
    limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
}
```

### SSL/TLS con Let's Encrypt

```bash
# Install certbot
sudo apt-get install certbot python3-certbot-nginx

# Ottieni certificato
sudo certbot --nginx -d api.yourdomain.com

# Auto-renew
sudo certbot renew --dry-run
```

---

## 🔧 Configurazione Production

### velox.toml Production

```toml
[render]
output_dir = "/opt/velox/output"
temp_dir = "/opt/velox/tmp"
max_concurrent_renders = 2
timeout_secs = 600
preset = "ultrafast"
crf = 23
simd_enabled = true
hugepages_enabled = true

[server]
host = "0.0.0.0"
port = 8080
max_connections = 500
request_timeout_secs = 300
cors_enabled = true
api_key = "${VOX_API_KEY}"  # Usa variabile d'ambiente

[system]
io_uring_enabled = true    # Linux 5.1+
mmap_enabled = true
custom_allocator = true
cpu_pinning_enabled = true
thread_pool_size = 4

[logging]
level = "warn"
file = "/var/log/velox/velox.log"
json_format = true
colors_enabled = false
```

### Variabili d'Ambiente

```bash
# /etc/environment o .env
VOX_API_KEY=your-secret-key
VOX_TEMP_DIR=/mnt/ramdisk/tmp
VOX_OUTPUT_DIR=/mnt/data/output
RUST_LOG=velox=warn
MIMALLOC_SHOW_STATS=1
```

---

## 📊 Monitoring

### Prometheus Metrics

Abilita endpoint `/metrics`:

```toml
[server]
metrics_enabled = true
metrics_port = 9090
```

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'velox'
    static_configs:
      - targets: ['localhost:9090']
```

### Grafana Dashboard

Importa dashboard ID `12345` per RustStream.

### Log Aggregation

```bash
# Journalctl
journalctl -u velox -f

# Log rotation
sudo tee /etc/logrotate.d/velox << 'EOF'
/var/log/velox/*.log {
    daily
    rotate 14
    compress
    delaycompress
    notifempty
    create 0640 velox velox
    postrotate
        systemctl reload velox
    endscript
}
EOF
```

---

## 🔐 Security Hardening

### Firewall (UFW)

```bash
sudo ufw allow 22/tcp      # SSH
sudo ufw allow 80/tcp      # HTTP
sudo ufw allow 443/tcp     # HTTPS
sudo ufw enable
```

### SELinux/AppArmor

```bash
# AppArmor profile
sudo tee /etc/apparmor.d/usr.local.bin.velox << 'EOF'
/usr/local/bin/velox {
  #include <abstractions/base>
  
  network inet tcp,
  network inet udp,
  
  /opt/velox/** rw,
  /tmp/velox/** rw,
  
  /dev/urandom r,
  
  deny /etc/** wl,
  deny /home/** wl,
}
EOF

sudo aa-enforce /etc/apparmor.d/usr.local.bin.velox
```

### Audit Logging

```bash
# Audit rules
sudo auditctl -w /usr/local/bin/velox -p x -k velox_exec
sudo auditctl -w /opt/velox -p rwxa -k velox_data

# Query logs
sudo ausearch -k velox_exec
```

---

## 📈 Performance Tuning

### HugePages

```bash
# Configura HugePages
sudo sysctl -w vm.nr_hugepages=512
echo "vm.nr_hugepages=512" | sudo tee -a /etc/sysctl.conf

# Verifica
cat /proc/meminfo | grep HugePages
```

### CPU Affinity

```bash
# Pin a core specifici
taskset -c 0-3 velox serve

# O nel systemd service
[Service]
CPUAffinity=0 1 2 3
```

### I/O Scheduler

```bash
# Imposta scheduler none (per NVMe)
echo none | sudo tee /sys/block/nvme0n1/queue/scheduler

# O deadline per SATA
echo deadline | sudo tee /sys/block/sda/queue/scheduler
```

### RAM Disk per Temp

```bash
# Crea RAM disk
sudo mkdir -p /mnt/ramdisk/tmp
sudo mount -t tmpfs -o size=256M tmpfs /mnt/ramdisk/tmp

# Persistente
echo "tmpfs /mnt/ramdisk/tmp tmpfs defaults,size=256M 0 0" | \
  sudo tee -a /etc/fstab
```

---

## 🔄 Update Strategy

### Blue-Green Deployment

```bash
# Deploy nuova versione
wget https://github.com/VeloxEditing/RustStream/releases/latest/download/velox-x86_64.tar.gz
tar xzf velox-*.tar.gz -d /opt/velox/new/

# Test
/opt/velox/new/velox --version

# Switch (atomic)
sudo systemctl stop velox
sudo mv /usr/local/bin/velox /usr/local/bin/velox.old
sudo mv /opt/velox/new/velox /usr/local/bin/velox
sudo systemctl start velox

# Rollback se necessario
# (inverso)
```

### Health Check Script

```bash
#!/bin/bash
# /opt/velox/health-check.sh

RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/health)

if [ "$RESPONSE" -eq 200 ]; then
    echo "Healthy"
    exit 0
else
    echo "Unhealthy (HTTP $RESPONSE)"
    exit 1
fi
```

---

## 🐛 Troubleshooting

### Log Analysis

```bash
# Errori recenti
journalctl -u velox --since "1 hour ago" -p err

# Performance issues
journalctl -u velox | grep "slow\|timeout"

# Memory usage
systemctl status velox | grep Memory
```

### Common Issues

**Problema**: "FFmpeg not found"
```bash
sudo apt-get install -y libavcodec-dev libavformat-dev
```

**Problema**: "Permission denied"
```bash
sudo chown -R velox:velox /opt/velox
sudo chmod 755 /opt/velox/{output,tmp}
```

**Problema**: "Out of memory"
```bash
# Riduci concurrent renders
# velox.toml: max_concurrent_renders = 1

# Abilita swap
sudo fallocate -l 1G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
```

---

## 📞 Support

- **Documentation**: https://github.com/VeloxEditing/RustStream/tree/main/docs
- **Issues**: https://github.com/VeloxEditing/RustStream/issues
- **Discussions**: https://github.com/VeloxEditing/RustStream/discussions

---

**Deploy Checklist:**

- [ ] Binary installato e verificato
- [ ] Servizio systemd attivo
- [ ] Firewall configurato
- [ ] SSL/TLS abilitato
- [ ] Monitoring attivo
- [ ] Log rotation configurata
- [ ] Backup strategy definita
- [ ] Rollback procedure testata
