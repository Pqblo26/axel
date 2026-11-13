#!/usr/bin/env bash
# Deja una cuenta lista para hacer pruebas: rango 45 (o el que se indique) y todas las fases de
# Main Quest (Normal, Hard y Expert) y de Free Quest superadas con las 3 coronas, así que todo
# queda desbloqueado. No da las recompensas de las coronas ni toca personajes, objetos ni monedas.
# Las mazmorras se quedan sin superar para poder jugarlas.
#
# Uso:
#   ./scripts/konofan/desbloquear-cuenta.sh <jugador> [rango] [--si]
#
# Hazlo con el juego en la portada.
set -euo pipefail

cd "$(dirname "$0")/../.."
DB="${KONOFAN_DB:-host=/var/run/postgresql dbname=axel}"

USER_ID="${1:-}"
[[ "$USER_ID" =~ ^[0-9]+$ ]] || { echo "Uso: $0 <jugador> [rango] [--si]"; exit 1; }
RANK=45
CONFIRM=""
for arg in "${@:2}"; do
  if [ "$arg" = "--si" ]; then CONFIRM=1; elif [[ "$arg" =~ ^[0-9]+$ ]]; then RANK="$arg"; fi
done
if [ -z "$CONFIRM" ]; then
  read -r -p "¿Poner la cuenta $USER_ID en rango $RANK con toda la historia y Free Quest superadas? Escribe 'si': " ANSWER
  [ "$ANSWER" = "si" ] || { echo "Cancelado"; exit 1; }
fi

# Experiencia total del rango y lista de fases, sacadas de los maestros
EXP=$(python3 -c "
import json, sys
rows = json.load(open('master/exp.json'))
match = [r for r in rows if r['rank'] == '$RANK']
if not match: sys.exit('no existe el rango $RANK en master/exp.json')
print(match[0]['allexp'])")
STAGES=$(python3 -c "
import json
ids = [row['id'] for name in ('mainquest_stage', 'huntingquest_stage') for row in json.load(open(f'master/{name}.json'))]
print(','.join(sorted(set(ids))))")

psql "$DB" -v ON_ERROR_STOP=1 <<SQL
begin;
do \$\$ begin
  if not exists (select 1 from users where id = $USER_ID) then
    raise exception 'no existe el jugador $USER_ID';
  end if;
end \$\$;
insert into user_progress (user_id, rank, exp) values ($USER_ID, $RANK, $EXP)
on conflict (user_id) do update set rank = excluded.rank, exp = excluded.exp;
insert into user_quest_progress (user_id, quest_id, clear_count, task1, task2, task3, daily_count)
select $USER_ID, id, 1, true, true, true, 0 from unnest('{$STAGES}'::integer[]) as id
on conflict (user_id, quest_id) do update set
  clear_count = greatest(user_quest_progress.clear_count, 1), task1 = true, task2 = true, task3 = true;
commit;
SQL
echo "Cuenta $USER_ID: rango $RANK y $(echo "$STAGES" | tr ',' '\n' | wc -l) fases superadas."
