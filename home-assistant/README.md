# Home Assistant integration

Expone el estado de la RAM (`ram-monitord`, puerto 9125) en Home Assistant
como sensores nativos. Sin custom component: solo configuración YAML usando
la integración `rest` que viene con `default_config`.

## Arquitectura

```
[ Ubuntu (wallabot) ]                 [ Raspberry (raspihome) ]
  ram-monitord                            Home Assistant (Docker, host net)
  127.0.0.1:9125  ◄──── ssh -L ─────  127.0.0.1:9125
                                            │
                                            └─► sensor.rest (scan_interval=15s)
```

Túnel SSH **forward** desde raspihome: abre `127.0.0.1:9125` en la pi y reenvía
cada conexión al loopback de wallabot. Persistencia en raspihome (systemd user
unit con linger); en wallabot solo una clave pública restringida.

## Instalación

### 1) Túnel SSH desde raspihome

```bash
# En raspihome (linger ya habilitado si desplegaste gpu/cpu antes):
cd /ruta/al/repo/ram_monitor/home-assistant/tunnel
./install.sh
```

Genera `~/.ssh/id_ed25519_ram_tunnel`, imprime la línea para `authorized_keys`
de wallabot (`restrict,port-forwarding,permitopen="127.0.0.1:9125" ssh-ed25519
...`), instala `ram-monitor-ha-tunnel.service` como user systemd unit.

Verifica:

```bash
systemctl --user status ram-monitor-ha-tunnel.service
curl -fsS http://127.0.0.1:9125/v1/info | jq
```

### 2) Paquete de Home Assistant

Si ya hay otro monitor de la familia desplegado, `homeassistant: { packages: !include_dir_named packages }` ya está en `configuration.yaml` y solo hay que:

```bash
scp packages/ram_monitor.yaml raspihome:/home/raspihome/docker/homeassistant/packages/
ssh raspihome 'docker restart homeassistant'
```

Si es la primera vez, primero añadir esto al final del `configuration.yaml`:

```yaml
homeassistant:
  packages: !include_dir_named packages
```

Tras recargar, en HA aparecen las entidades:

```
sensor.ram_monitor_host        sensor.ram_used               sensor.swap_total
sensor.ram_monitor_kernel      sensor.ram_available          sensor.swap_used
sensor.ram_monitor_total       sensor.ram_free               sensor.swap_used_percent
                               sensor.ram_buffers
                               sensor.ram_cached             sensor.ram_top_process
                               sensor.ram_used_percent       sensor.ram_top_process_rss
                                                             sensor.ram_process_count
```

`sensor.ram_top_process` lleva `attributes.processes` con la lista completa.

### 3) Dashboard (opcional)

`lovelace/ram_dashboard.yaml` — pegar como vista nueva en el Raw editor.

## Notas

- **Bytes → GiB en plantilla**: el daemon expone bytes crudos (precisos), HA convierte a GiB con 2 decimales para el UI. `state_class: measurement` mantiene historial graficable.
- **`available` vs `free`**: en Linux, `MemAvailable` es lo que el kernel considera reclaimable (incluye page cache), `MemFree` es solo lo no asignado. La barra del UI debe mostrar `used = total - available`, no `total - free`.
- **Coexistencia**: convive con `gpu_monitor.yaml`, `cpu_monitor.yaml`, etc. en `/config/packages/`. Puertos distintos, túneles distintos, claves SSH distintas.

## Troubleshooting

- **Sensores `unavailable`**: `systemctl --user status ram-monitor-ha-tunnel.service` y `curl http://127.0.0.1:9125/healthz` desde raspihome.
- **`administratively prohibited`**: la línea en `authorized_keys` de wallabot omite `port-forwarding`. Debe ser `restrict,port-forwarding,permitopen="127.0.0.1:9125" ...`.
