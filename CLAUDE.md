# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Comandos

Toolchain anclado a `stable` por `rust-toolchain.toml` (rustup lo instala solo). Todo se opera desde la raíz del workspace.

```bash
cargo build --workspace                      # debug build de los 3 crates
cargo build --release --workspace            # release (lo que se distribuye)
cargo test --workspace                       # todos los tests
cargo clippy --workspace -- -D warnings      # CI lo exige limpio
cargo fmt --all                              # formateo

# Ejecución manual (para iterar):
./target/release/ram-monitord --bind 127.0.0.1 --port 9125 --sample-interval-ms 1000
./target/release/ram-monitor-tray --backend-url http://127.0.0.1:9125

# Sin /proc real (CI, tests):
./target/release/ram-monitord --mock

# Volcar el icono renderizado a un PNG y salir, sin tocar el panel.
# Imprescindible para depurar fallos visuales sin pelearte con GNOME:
./target/release/ram-monitor-tray --backend-url http://127.0.0.1:9125 --dump-icon /tmp/icon.png
```

`ram_monitor.py` (legacy) sigue funcional y puede correr en paralelo a la versión Rust mientras dure la migración. No los pongas a usar el mismo `--port`.

Hermanos de la familia: `gpu_monitor` (puerto 9123), `cpu_monitor` (9124), `ram_monitor` (este, 9125), `disk_monitor` (9126). Cada uno es un workspace independiente para que el usuario pueda instalar solo los que aplica a su máquina.

## Arquitectura

Workspace Cargo con tres crates:

```
crates/ram-monitor-core    →  tipos compartidos (serde Snapshot/Memory/Swap/Process)
crates/ram-monitord        →  daemon HTTP+SSE que lee /proc/meminfo y /proc/<pid>/status
crates/ram-monitor-tray    →  frontend Linux (system tray)
```

El protocolo es REST + Server-Sent Events sobre HTTP. El backend está pensado para correr 24/7 mientras los frontends locales (o remotos en el futuro) van y vienen.

### Flujo del backend (`ram-monitord`)

`main.rs` arranca y mantiene un único `RamSource` (trait): `ProcfsSource` en producción, `MockSource` cuando se pasa `--mock`. **Nunca abras `/proc/meminfo` o `/proc/<pid>/status` por petición HTTP** — todas las lecturas las hace el sampler.

`sampler::spawn` lanza una task de tokio que muestrea cada N ms y publica en un `tokio::sync::watch::Sender<Snapshot>`. Todos los handlers HTTP leen del `Receiver` (`borrow().clone()`), latencia O(µs). El handler SSE reenvía el watch como stream con `WatchStream`.

Para procesos: iteramos `/proc/[0-9]+/status` y leemos `Name`, `VmSize`, `VmRSS`. Saltamos los PIDs que desaparecen entre `readdir` y `read_to_string` (procesos efímeros — race-by-design). El `top_n` se controla con `--max-processes` (0 desactiva el escaneo).

`with_graceful_shutdown` de axum **no se usa** porque espera a que se vacíen las conexiones, y los streams SSE son por naturaleza infinitos: `systemctl stop` quedaría colgado. La salida se hace con `tokio::select!` entre `axum::serve` y la señal.

### MemAvailable vs MemFree

Reportamos ambos. **`used_bytes = total - available`** (no `total - free`). `MemFree` ignora el page cache reclaimable; usar `total - free` haría parecer que la máquina está más llena de lo que realmente está. Si el kernel es muy viejo (<3.14) y no expone `MemAvailable`, aproximamos como `free + buffers + cached` (ver `parse_meminfo`).

### Flujo del frontend (`ram-monitor-tray`)

`client::spawn` mantiene un loop de SSE con backoff (1s → 2s → 4s → 5s tope) que se resetea al recibir `Event::Open`.

`tray::RamTray::set_state` llama a `refresh_icon_file`: rerenderiza el PNG con `IconRenderer::render_png`, lo escribe en `~/.cache/ram-monitor/icons/ram-monitor-tray-N.png` (donde N es un contador que solo crece), y borra el frame anterior. La impl `Tray` publica `IconName = ram-monitor-tray-N` y `IconThemePath = ~/.cache/ram-monitor/icons`.

#### Punto crítico: por qué un PNG en disco y no `IconPixmap`

SNI permite mandar el icono como bytes ARGB inline. **No lo hagas** — la extensión `ubuntu-appindicators` de GNOME comprime los pixmaps anchos a una proporción cuadrada y mangle iconos multi-elemento. La estrategia de archivo + `IconName` con contador incremental es exactamente lo que `AppIndicator3.set_icon_full(path, ...)` hace internamente y es la única que GNOME respeta a anchura nativa.

Si alguien "limpia" esto sustituyéndolo por `icon_pixmap()` se rompe visualmente en GNOME aunque pase los tests.

### Render del icono (`icon::render`)

`tiny-skia` produce RGBA **premultiplicado**. Hay dos rutas de salida:

- `unpremultiply_to_rgba` → para el PNG del disco (los visores PNG asumen straight RGBA).
- `rgba_premul_to_argb_straight` → para `IconPixmap` por si en algún momento se vuelve a usar.

Texto: `freetype-rs 0.32` (versión pinned por compatibilidad con la libfreetype de Ubuntu 20.04, ABI 23.1.17). Se usa el bytecode interpreter real, no un rasterizador pure-Rust — `fontdue` y `ab_glyph` quedan borrosos a 10–11 px. La fuente se busca en `DEFAULT_FONT_PATHS`; si falta, requiere `fonts-dejavu-core`.

**FreeType no implementa `Send`** pero `ksni::TrayService::spawn` lo exige; envolvemos `Library` y `Face` en `FtState` con `unsafe impl Send + Sync`. Es seguro porque el acceso es secuencial (solo desde la callback de `update` del thread de ksni).

### Layout y código de colores del icono

```
[ram icon ~22x22] 2px [label "{used:>2}/{total:<2}G"] 2px [donut 18x18 con % al centro]
```

| const            | hex       | uso                                         |
|---               |---        |---                                          |
| `COLOR_TEXT`     | `#ffffff` | label normal, número dentro del donut       |
| `COLOR_FREE`     | `#66b3ff` | anillo: porción libre                       |
| `COLOR_OK`       | `#99ff99` | anillo lleno <70%                           |
| `COLOR_WARN1`    | `#ffdb4d` | anillo 70–80%                               |
| `COLOR_WARN2`    | `#ffcc99` | anillo 80–90%                               |
| `COLOR_HIGH`     | `#ff6666` | anillo ≥90%                                 |

**Estado disconnected**: todo el bloque pasa a gris (`#aaaaaa` texto, `#808080` libre, `#606060` usado) y el label muestra `"  --G"`. Sigue indicando que hay tray vivo pero los datos están viejos.

Para RAM, el único elemento "coloreable" es el donut según el porcentaje usado. No hay temperatura ni utilización separada, así que el label va siempre en color neutro.

## Home Assistant (`home-assistant/`)

Integración declarativa con HA usando el componente `rest` de `default_config`. 15 sensores: host/kernel/total + 6 métricas de memoria (used/available/free/buffers/cached/used_percent) + 3 de swap + 3 de procesos.

**Topología**: túnel SSH forward desde raspihome (always-on) al host con RAM, puerto 9125. Toda la persistencia en la pi; en wallabot solo una pubkey en `authorized_keys` con `restrict,port-forwarding,permitopen="127.0.0.1:9125"`. Sin `port-forwarding` explícito, sshd corta el canal con `administratively prohibited` aunque la auth pase.

**Bytes → GiB en plantilla**: el daemon expone bytes (precisos), HA convierte a GiB con 2 decimales (`/ 1073741824`) para que el UI no salga saturado. `state_class: measurement` mantiene historial graficable. `device_class: data_size` es lo que HA acepta para unidades de almacenamiento (B, KiB, MiB, GiB, TiB...).

**`available` vs `free`**: en Linux, `MemAvailable` es lo que el kernel considera reclaimable (page cache reciclable cuenta), `MemFree` es solo lo no asignado. El paquete expone los dos; los gauges deben usar `used_percent` (que ya viene `total - available` desde el daemon) y `available` para "memoria libre visible al usuario", NO `free`. Si dejas que el usuario mire `free`, va a pensar que tiene una máquina al 95% cuando solo tiene cache caliente.

**Schema replication**: igual que con `front-mac/Models.swift`, si añades un campo a `Memory` / `Swap` / `Process` en `ram-monitor-core`, replícalo en `home-assistant/packages/ram_monitor.yaml` como nuevo `value_template`.

## Convenciones del repo

- **API versioning** por prefijo de path (`/v1/...`). `ram_monitor_core::API_VERSION` es la fuente de verdad.
- **Tipos serializados** viven en `ram-monitor-core`. Si añades un campo a `Snapshot` / `Memory` / `Process`, tanto backend como tray lo ven sin drift, pero **es un cambio de schema**.
- **Defaults seguros**: el daemon bindea `127.0.0.1` sin auth.
- **Dependencias compartidas** declaradas en `[workspace.dependencies]` del `Cargo.toml` raíz.
- **Logging** vía `tracing` + `tracing-subscriber`; controlable con `RUST_LOG` o `--log-level`.
- **Tests del frontend** evitan dependencias de runtime gráfico: el render se prueba comparando bytes, no abriendo ventanas. `/proc` se prueba contra `MockSource`, nunca contra el sistema real.

## Modelo de arranque: daemon ≠ tray

- **Daemon (`ram-monitord`)**: systemd `--user` service. Se reinicia con `systemctl --user restart ram-monitord`.
- **Tray (`ram-monitor-tray`)**: `.desktop` autostart en `~/.config/autostart/`, **NO es un servicio systemd**. Se lanza al login y vive hasta logout.

El tray necesita la sesión gráfica completa (DBus user bus, panel SNI). systemd-user arranca antes que la sesión gráfica en algunos compositores; con un `.desktop` te aseguras que el tray solo arranca cuando hay panel donde plantar el icono.

Tras `cargo build` + `install` del binario nuevo del tray, `systemctl --user restart ram-monitor-tray` falla con "Unit not found". Flujo correcto:

```bash
install -m 0755 target/release/ram-monitor-tray ~/.local/bin/
pkill -f "$HOME/.local/bin/ram-monitor-tray$"   # mata el viejo
nohup ~/.local/bin/ram-monitor-tray >/dev/null 2>&1 & disown
```

O logout/login si no hay prisa.

## Localización del icono base

Para que el binario instalado funcione independiente de dónde se compiló, busca en este orden:

1. `$RAM_MONITOR_TRAY_ICON` (override env var)
2. `$XDG_DATA_HOME/ram-monitor/ram.png` (típicamente `~/.local/share/ram-monitor/`)
3. `/usr/share/ram-monitor/ram.png`
4. `assets/ram.png` relativo al cwd (dev)
5. `<workspace>/assets/ram.png` baked-in via `env!("CARGO_MANIFEST_DIR")` (último recurso)

## Errores que ya costaron tiempo (no repetir)

- `pkill -x ram-monitor-tray` no funciona: el kernel trunca `comm` a 15 chars, y `ram-monitor-tray` tiene 16. Mata por path completo (`pkill -f "/full/path$"`) o por PID concreto.
- `axum::serve(...).with_graceful_shutdown(...)` cuelga `systemctl stop` si hay un cliente SSE conectado. Usa `tokio::select!` con la señal en su lugar.
- No spawnear `ps`/`top` para procesos: parsing regex frágil + 50–200 ms por llamada. Itera `/proc/<pid>/status` directo (~1 ms total).
- `--no-verify` y otros bypasses de hooks no se usan aunque algo bloquee.
