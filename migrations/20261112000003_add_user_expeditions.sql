-- Jobs ("expeditions"): which character works in which job and since when.

drop table if exists user_expeditions cascade;
create table user_expeditions
(
  user_id       bigint      not null references users (id) on delete cascade,
  expedition_id integer     not null,
  character_id  bigint      not null,
  -- Rewards are counted from this moment; reset when they are claimed
  started_at    timestamptz not null default now(),
  primary key (user_id, expedition_id)
);

-- Exchange shop: one row per exchange, used for "N left" limits.
drop table if exists user_exchanges cascade;
create table user_exchanges
(
  id                bigserial primary key,
  user_id           bigint      not null references users (id) on delete cascade,
  exchange_item_id  integer     not null,
  num               integer     not null check (num > 0),
  exchanged_at      timestamptz not null default now()
);

create index user_exchanges_user_item_idx on user_exchanges (user_id, exchange_item_id);
