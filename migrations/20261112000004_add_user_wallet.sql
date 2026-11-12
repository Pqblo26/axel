-- Currencies and stamina, which used to be fixed on every login (src/blob.rs).
-- Default values are the amounts every account had until now.

drop table if exists user_wallet cascade;
create table user_wallet
(
  user_id            bigint      not null primary key references users (id) on delete cascade,
  -- Eris
  money              bigint      not null default 85720 check (money >= 0),
  -- Paid quartz
  real_money         integer     not null default 100000 check (real_money >= 0),
  -- Free quartz
  real_money_free    integer     not null default 1000000 check (real_money_free >= 0),
  stamina            integer     not null default 419 check (stamina >= 0),
  -- Stamina recovery is counted from this moment
  stamina_updated_at timestamptz not null default now()
);

-- Shop purchases, used for daily and total buy limits.
drop table if exists user_shop_purchases cascade;
create table user_shop_purchases
(
  id                  bigserial primary key,
  user_id             bigint      not null references users (id) on delete cascade,
  shop_item_id        integer     not null,
  num                 integer     not null check (num > 0),
  purchased_at        timestamptz not null default now()
);

create index user_shop_purchases_user_item_idx on user_shop_purchases (user_id, shop_item_id);
