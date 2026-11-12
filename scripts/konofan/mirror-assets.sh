#!/usr/bin/env bash
# Descarga (o actualiza) una copia local de los recursos del juego desde el espejo
# de Assasans, para no depender de él. Son unos 16 GiB; se puede cortar y reanudar.
set -euo pipefail
RESOURCES_ROOT="${KONOFAN_RESOURCES:-/srv/konofd}"
RSYNC_BASE="rsync://aqua.assasans.dev/konofd"
for dir in banners webview bundles; do
  echo "==> $dir"
  mkdir -p "$RESOURCES_ROOT/$dir"
  # .directory: desktop folder metadata on the mirror, unreadable and not needed.
  # Exit codes 23/24 (some files unreadable or vanished) should not stop the whole mirror.
  rsync -a --partial --info=progress2 --exclude='.directory' "$RSYNC_BASE/$dir/" "$RESOURCES_ROOT/$dir/" \
    || { rc=$?; [ "$rc" -eq 23 ] || [ "$rc" -eq 24 ] || exit "$rc"; echo "AVISO: algunos ficheros de $dir no se pudieron copiar (rsync $rc)"; }
done
