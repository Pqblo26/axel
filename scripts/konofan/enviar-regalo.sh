#!/usr/bin/env bash
# Mete un regalo en el buzón (Gifts) de un jugador o de todos.
#
# Uso:
#   ./scripts/konofan/enviar-regalo.sh <tipo> <id> <cantidad> "<mensaje>" [jugador|todos] [días]
#
# Ejemplos:
#   ./scripts/konofan/enviar-regalo.sh 3 1 5000 "Regalo de prueba"          # 5000 cuarzos gratis a todos
#   ./scripts/konofan/enviar-regalo.sh 29 1 10 "Comida" 1 7                  # 10 objetos de estamina al jugador 1, caduca en 7 días
#
# Tipos más útiles (RemoteDataItemType en src/api/mod.rs):
#   1 eris · 2 cuarzo de pago · 3 cuarzo gratis · 9 estamina · 18 pociones de poder (id 1-3)
#   21 tickets de gacha (id según master/item.json) · 29 objetos de estamina (id 1-2) · 20 medallas
# Miembros (tipo 4) se reciben de verdad; armas y accesorios todavía no (se quedan en el buzón).
#
# El juego actualiza el contador al volver a la portada.
set -euo pipefail

if [ $# -lt 4 ]; then
  sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
  exit 1
fi

ITEM_TYPE="$1"
ITEM_ID="$2"
ITEM_NUM="$3"
MESSAGE="$4"
TARGET="${5:-todos}"
DAYS="${6:-}"
DB="${KONOFAN_DB:-host=/var/run/postgresql dbname=axel}"

EXPIRES="null"
[ -n "$DAYS" ] && EXPIRES="now() + interval '$DAYS days'"

WHERE="true"
[ "$TARGET" != "todos" ] && WHERE="id = $TARGET"

psql "$DB" -v ON_ERROR_STOP=1 \
  -v item_type="$ITEM_TYPE" -v item_id="$ITEM_ID" -v item_num="$ITEM_NUM" -v message="$MESSAGE" <<SQL
insert into user_presents (user_id, item_type, item_id, item_num, message, expires_at)
select id, :item_type, :item_id, :item_num, :'message', $EXPIRES
from users
where $WHERE;
SQL
