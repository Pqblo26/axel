-- Gift box ("Gifts" on the home screen). One row per present; received_at is set when accepted.

drop table if exists user_presents cascade;
create table user_presents
(
  id          bigserial primary key,
  user_id     bigint      not null references users (id) on delete cascade,
  -- RemoteDataItemType (2 = paid quartz, 3 = free quartz, 9 = stamina, 21 = gacha ticket...)
  item_type   integer     not null,
  item_id     bigint      not null default 1,
  item_num    integer     not null check (item_num > 0),
  message     text        not null default '',
  sent_at     timestamptz not null default now(),
  -- null = never expires
  expires_at  timestamptz null,
  received_at timestamptz null,
  -- Set for presents granted automatically (e.g. 'welcome'), so they are only granted once per user
  source      text        null
);

create index user_presents_pending_idx on user_presents (user_id) where received_at is null;
create unique index user_presents_source_idx on user_presents (user_id, source) where source is not null;
