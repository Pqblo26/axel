-- Stores every member obtained from the gacha, for the in-game draw history ("gachalog").

create table if not exists gacha.user_draws
(
  id        bigserial   primary key,
  user_id   bigint      not null references users (id) on delete cascade,
  gacha_id  bigint      not null,
  item_type integer     not null,
  item_id   bigint      not null,
  item_num  integer     not null default 1,
  drawn_at  timestamptz not null default now()
);

create index if not exists idx_gacha_user_draws_user_time on gacha.user_draws (user_id, drawn_at desc);
