# RAM monitor

🖥️ RAM Monitor for Ubuntu: real-time RAM tracking in your menu bar. Live free/used/cached/swap stats and the top processes by RSS, all from a tiny system-tray icon.

![ram monitor](ram_monitor.gif)

## Architecture

Two flavours, same project:

- **Rust (current)** — workspace with three crates:
  - `ram-monitord` — HTTP+SSE daemon that reads `/proc/meminfo` and `/proc/<pid>/status` (~few MB RSS, < 1% CPU).
  - `ram-monitor-tray` — Linux system-tray frontend. Renders the icon with `tiny-skia` + FreeType.
  - `ram-monitor-core` — shared serde types between back and front.
- **Python (legacy)** — `legacy/ram_monitor.py`, a single-file GTK indicator. Still functional, kept for reference for one release cycle.

The two can coexist on different ports.

Sister projects (each one is its own repo so you can install only what your machine needs): [`gpu_monitor`](https://github.com/maximofn/gpu_monitor), `cpu_monitor`, `disk_monitor`. Default ports: gpu=9123, cpu=9124, **ram=9125**, disk=9126.

## Install (Rust)

### Build

```bash
sudo apt install fonts-dejavu-core libfreetype6 libfreetype-dev pkg-config build-essential
git clone https://github.com/maximofn/ram_monitor.git
cd ram_monitor
cargo build --release --workspace
```

### Install binaries and assets

```bash
install -m 0755 target/release/ram-monitord     ~/.local/bin/
install -m 0755 target/release/ram-monitor-tray ~/.local/bin/
install -Dm 0644 assets/ram.png                 ~/.local/share/ram-monitor/ram.png
```

### Daemon as a user systemd service

```bash
install -Dm 0644 packaging/systemd/ram-monitord.service ~/.config/systemd/user/ram-monitord.service
systemctl --user daemon-reload
systemctl --user enable --now ram-monitord
```

Sanity check:

```bash
curl -s http://127.0.0.1:9125/v1/info
curl -s http://127.0.0.1:9125/v1/memory
```

### Tray as autostart

```bash
install -Dm 0644 packaging/autostart/ram-monitor-tray.desktop ~/.config/autostart/ram-monitor-tray.desktop
nohup ~/.local/bin/ram-monitor-tray >/dev/null 2>&1 & disown
```

Or just log out / log in.

### CLI flags

```bash
ram-monitord --help
# --bind, --port, --sample-interval-ms, --max-processes, --mock, --log-level

ram-monitor-tray --help
# --backend-url, --icon-height, --dump-icon (debug: render one PNG and exit), --log-level
```

## Install (Python, legacy)

> Kept for one release cycle. Prefer the Rust path above. The `legacy/` directory will be removed once the Rust path proves stable in the wild.

```bash
sudo apt install lm-sensors psensor
sudo sensors-detect
pip install psutil
cd legacy && ./add_to_startup.sh
```

Install python3-pip

```bash
sudo apt install python3-pip
```

Install matplotlib

```bash
pip3 install matplotlib
```

## Execution at start-up

The daemon serves snapshots over plain HTTP and SSE.

| Method | Path             | Description                                |
|--------|------------------|--------------------------------------------|
| GET    | `/healthz`       | Liveness + uptime                          |
| GET    | `/v1/info`       | Backend version, host, kernel, total RAM   |
| GET    | `/v1/snapshot`   | Full snapshot (memory + swap + processes)  |
| GET    | `/v1/memory`     | Memory totals only                         |
| GET    | `/v1/swap`       | Swap totals only                           |
| GET    | `/v1/processes`  | Top-N processes by RSS                     |
| GET    | `/v1/stream`     | SSE stream of `Snapshot` events            |

Defaults: bind `127.0.0.1`, port `9125`, sample interval 1 s, top-20 processes.

## Support

Consider giving a **☆ Star** to this repository, or invite me to a coffee:

[![BuyMeACoffee](https://img.shields.io/badge/Buy_Me_A_Coffee-support_my_work-FFDD00?style=for-the-badge&logo=buy-me-a-coffee&logoColor=white&labelColor=101010)](https://www.buymeacoffee.com/maximofn)
