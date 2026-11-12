#!/usr/bin/env bash
# Devuelve una cuenta al estado de después del tutorial: los 4 personajes de inicio y los 10 del
# gacha del tutorial, rango 1, sin progreso en las misiones, monedas iniciales y los regalos de
# bienvenida otra vez en el buzón. NO SE PUEDE DESHACER.
#
# Uso:
#   ./scripts/konofan/reiniciar-cuenta.sh            # lista las cuentas
#   ./scripts/konofan/reiniciar-cuenta.sh <jugador>  # reinicia esa cuenta (pide confirmación)
#
# Hazlo con el juego en la portada; al entrar, el servidor vuelve a crear personajes, equipos, etc.
set -euo pipefail

DB="${KONOFAN_DB:-host=/var/run/postgresql dbname=axel}"

if [ $# -lt 1 ]; then
  psql "$DB" -c "
    select u.id as jugador, coalesce(u.username, '(sin nombre)') as nombre, u.created_at::date as creada,
           max(d.last_used) as ultimo_uso, (select count(*) from user_members m where m.user_id = u.id) as personajes
    from users u left join user_devices d on d.user_id = u.id
    group by u.id order by ultimo_uso desc nulls last"
  exit 0
fi

USER_ID="$1"
[[ "$USER_ID" =~ ^[0-9]+$ ]] || { echo "El jugador tiene que ser un número"; exit 1; }
if [ "${2:-}" != "--si" ]; then
  read -r -p "¿Reiniciar la cuenta $USER_ID? No se puede deshacer. Escribe 'si': " ANSWER
  [ "$ANSWER" = "si" ] || { echo "Cancelado"; exit 1; }
fi

psql "$DB" -v ON_ERROR_STOP=1 -v user_id="$USER_ID" <<'SQL'
begin;
select set_config('reset.user_id', :'user_id', true);
do $$
declare
  uid bigint := current_setting('reset.user_id')::bigint;
  t text;
begin
  if not exists (select 1 from users where id = uid) then
    raise exception 'no existe el jugador %', uid;
  end if;
  foreach t in array array[
    'user_party_forms', 'user_parties', 'user_member_skills', 'user_members_reserve', 'user_members',
    'user_character_special_skills', 'user_character_skills', 'user_character_pieces', 'user_characters',
    'user_items', 'user_items_equipment', 'user_home_illustrations', 'user_presents', 'user_exchanges',
    'user_expeditions', 'user_shop_purchases', 'user_wallet', 'user_progress', 'user_quest_progress',
    'user_dungeon_runs', 'user_dungeon_clears', 'user_score_challenge_best', 'user_score_challenge_runs',
    'gacha.user_draws'
  ] loop
    if to_regclass(t) is not null then
      execute format('delete from %s where user_id = $1', t) using uid;
    end if;
  end loop;
  update users set home_current_illustration_id = null, favorite_member = 1001100 where id = uid;
  -- Personajes de inicio y los del gacha del tutorial (src/user/grant.rs)
  insert into user_members (user_id, member_id, xp, promotion_level)
  select uid, m, 0, 0
  from unnest(array[1001100, 1011100, 1021100, 1031100,
                    1063113, 1034100, 1152102, 1083110, 1122100, 1093100, 1132100, 1002102, 1282100, 1064217]::bigint[]) as m;
  raise notice 'cuenta % reiniciada', uid;
end $$;
commit;
SQL
