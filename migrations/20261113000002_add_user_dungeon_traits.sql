-- The party trait chosen in the dungeon party screen, kept for every dungeon until changed.
drop table if exists user_dungeon_traits cascade;
create table user_dungeon_traits
(
  user_id    bigint      not null primary key references users (id) on delete cascade,
  skill_id   bigint      not null default 0,
  updated_at timestamptz not null default now()
);
