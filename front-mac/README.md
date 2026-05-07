# front-mac — frontend macOS para ram-monitord

Status bar app nativo (Swift + AppKit) que consume el HTTP+SSE de `ram-monitord`
y muestra un icono dinámico en la barra superior del Mac, con menú desplegable
de detalle de RAM, swap y top processes.

Réplica funcional del tray Linux (`crates/ram-monitor-tray`). Mismo schema
(`/v1/snapshot`, `/v1/stream`), misma paleta del donut. Sin Dock icon
ni ventanas — `LSUIElement = true`.

## Requisitos

- macOS 13 (Ventura) o superior.
- Swift 5.9+ (Xcode 15+ o `swift` en línea de comandos via toolchain).
- Un `ram-monitord` corriendo y accesible por red.

## Build

```bash
cd front-mac
swift build -c release
```

El binario sale en `.build/release/RAMMonitorTray`. Puede ejecutarse tal cual
para iterar:

```bash
.build/release/RAMMonitorTray --backend-url http://127.0.0.1:9125
```

Para uso real, empaquetar en `.app` (sin Dock icon):

```bash
./scripts/build-app.sh
open "build/RAM Monitor.app" --args --backend-url http://192.168.1.50:9125
```

## CLI

| Flag                     | Default                       | Descripción                                |
| ------------------------ | ----------------------------- | ------------------------------------------ |
| `--backend-url <URL>`    | `http://127.0.0.1:9125`       | Base del API (env: `RAM_MONITOR_TRAY_URL`) |
| `--icon-height <PT>`     | `22`                          | Altura lógica del icono en la barra        |
| `--log-level <LEVEL>`    | `info`                        | trace/debug/info/warn/error (OSLog)        |
| `--dump-icon <PATH>`     | —                             | Pinta el snapshot actual a PNG y sale      |
| `--version`              | —                             | Versión                                    |
| `-h`, `--help`           | —                             | Ayuda                                      |

## Autostart en login

Hay dos rutas:

1. **System Settings → General → Login Items → Open at Login** → `+` → seleccionar
   `RAM Monitor.app`.
2. Vía LaunchAgent (idempotente):
   ```bash
   ./scripts/install-launchagent.sh
   ```

## Network: HTTP plano

El backend está pensado para LAN privada con HTTP plano. El bundle declara
`NSAppTransportSecurity → NSAllowsArbitraryLoads = true` para permitir
`http://192.168.x.x` sin certificados. **No expongas el backend a Internet con
esta config**.

### Túnel SSH persistente (recomendado)

Mientras el daemon siga bindeado a `127.0.0.1` (default seguro), la forma limpia
de consumirlo desde el Mac es un LaunchAgent que mantiene un `ssh -N -L` vivo
al login y reconecta si se cae:

```xml
<!-- ~/Library/LaunchAgents/com.maximofn.ram-monitor-tunnel.plist -->
<plist version="1.0"><dict>
  <key>Label</key><string>com.maximofn.ram-monitor-tunnel</string>
  <key>ProgramArguments</key><array>
    <string>/usr/bin/ssh</string>
    <string>-N</string>
    <string>-o</string><string>ExitOnForwardFailure=yes</string>
    <string>-o</string><string>ServerAliveInterval=30</string>
    <string>-o</string><string>ServerAliveCountMax=3</string>
    <string>-o</string><string>StrictHostKeyChecking=accept-new</string>
    <string>-L</string><string>9125:127.0.0.1:9125</string>
    <string>&lt;ssh-host&gt;</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ThrottleInterval</key><integer>10</integer>
  <key>StandardErrorPath</key><string>/Users/&lt;you&gt;/Library/Logs/ram-monitor-tunnel.err.log</string>
</dict></plist>
```

```bash
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.maximofn.ram-monitor-tunnel.plist
# luego: la app apunta a http://127.0.0.1:9125 (default)
```

## Schema y compatibilidad

`Models.swift` replica `crates/ram-monitor-core/src/model.rs`. Si añades campos
al `Snapshot`/`Memory`/`Swap`/`Process` en Rust, **replica aquí** o el JSON
decode ignorará los nuevos campos en silencio. La API está versionada por path
(`/v1/...`) — un cambio incompatible se hace subiendo a `/v2/`.

## Diferencias con el tray Linux

- El renderer es Core Graphics + Core Text en vez de tiny-skia + freetype. La
  geometría (donut, gaps, layout) y los colores están portados 1:1.
- La fuente es **SF Pro con dígitos monoespaciados** (system) en vez de DejaVu
  Sans Mono. Hinting nativo de macOS, sin TTC manual ni búsqueda de paths.
- El label se mantiene en blanco siempre, siguiendo la convención del menu bar
  (clock, battery). En Linux el label se colorea por presión de swap; aquí esa
  señal vive en el tooltip y el menú desplegable.
- No hay archivos PNG en `~/.cache` — `NSStatusItem` acepta `NSImage` en
  memoria sin los problemas de cacheo que tiene GNOME-shell.

## Verificación rápida

```bash
# 1. Backend mock en otra máquina (o local):
cargo run -p ram-monitord --release -- --mock --bind 0.0.0.0 --port 9125

# 2. Frontend Mac apuntando al mock:
swift run --package-path front-mac RAMMonitorTray --backend-url http://<host>:9125

# 3. Volcar el icono a un PNG sin tocar la status bar (debug):
swift run --package-path front-mac RAMMonitorTray \
    --backend-url http://<host>:9125 --dump-icon /tmp/mac-icon.png
```
