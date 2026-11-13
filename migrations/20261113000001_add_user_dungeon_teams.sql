-- The team chosen in the dungeon team screen (Suggest), used when entering an area.
drop table if exists user_dungeon_teams cascade;
create table user_dungeon_teams
(
  user_id    bigint      not null primary key references users (id) on delete cascade,
  -- Member ids, in order; empty = the strongest members
  members    bigint[]    not null default '{}',
  updated_at timestamptz not null default now()
);
