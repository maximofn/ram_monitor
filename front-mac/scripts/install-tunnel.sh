#!/usr/bin/env bash
# Install / reinstall the LaunchAgent that keeps an SSH tunnel
# 9125:127.0.0.1:9125 alive to the configured host.
# Usage:
#   ./scripts/install-tunnel.sh           # install + load
#   ./scripts/install-tunnel.sh uninstall # unload + remove
set -euo pipefail

LABEL="com.maximofn.ram-monitor-tunnel"
SRC="$(cd "$(dirname "$0")" && pwd)/${LABEL}.plist"
DST="$HOME/Library/LaunchAgents/${LABEL}.plist"

uid="$(id -u)"
domain="gui/${uid}"
target="${domain}/${LABEL}"

cmd="${1:-install}"

case "$cmd" in
    install)
        if [[ ! -f "$SRC" ]]; then
            echo "error: source plist not found: $SRC" >&2
            exit 1
        fi

        if launchctl print "$target" >/dev/null 2>&1; then
            echo "==> bootout existing $LABEL"
            launchctl bootout "$target" || true
        fi

        echo "==> install $DST"
        mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
        cp "$SRC" "$DST"

        echo "==> bootstrap $target"
        launchctl bootstrap "$domain" "$DST"
        launchctl enable "$target"
        launchctl kickstart -k "$target"

        echo
        echo "Loaded. The tunnel will autostart on login."
        echo "Logs: ~/Library/Logs/ram-monitor-tunnel.{out,err}.log"
        echo
        echo "If the SSH host is not 'wallabot', edit:"
        echo "  ~/Library/LaunchAgents/${LABEL}.plist"
        echo "and re-run this script."
        ;;
    uninstall)
        if launchctl print "$target" >/dev/null 2>&1; then
            echo "==> bootout $target"
            launchctl bootout "$target" || true
        fi
        if [[ -f "$DST" ]]; then
            echo "==> remove $DST"
            rm -f "$DST"
        fi
        echo "Uninstalled."
        ;;
    *)
        echo "usage: $0 [install|uninstall]" >&2
        exit 2
        ;;
esac
