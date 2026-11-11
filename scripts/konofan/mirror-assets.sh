#!/usr/bin/env bash
# Descarga (o actualiza) una copia local de los recursos del juego desde el espejo
# de Assasans, para no depender de él. Son unos 16 GiB; se puede cortar y reanudar.
set -euo pipefail
RESOURCES_ROOT="${KONOFAN_RESOURCES:-/srv/konofd}"
RSYNC_BASE="rsync://aqua.assasans.dev/konofd"
for dir in banners webview bundles; do
  echo "==> $dir"
  mkdir -p "$RESOURCES_ROOT/$dir"
  rsync -a --partial --info=progress2 "$RSYNC_BASE/$dir/" "$RESOURCES_ROOT/$dir/"
done
