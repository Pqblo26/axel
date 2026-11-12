-- Dungeon runs and cleared dungeon areas, which used to be faked on every request.

-- The dungeon area the user is going through (one at a time, like the original game).
drop table if exists user_dungeon_runs cascade;
create table user_dungeon_runs
(
  user_id         bigint      not null primary key references users (id) on delete cascade,
  area_id         integer     not null,
  -- Current floor
  stage_id        integer     not null,
  is_practice     boolean     not null default false,
  -- Team members (member ids), in order
  members         bigint[]    not null,
  -- Current HP by member id; members not listed have full HP
  member_hp       jsonb       not null default '{}',
  -- Current HP by enemy id on the current floor; enemies not listed have full HP
  enemy_hp        jsonb       not null default '{}',
  -- Chosen benefits (dungeon_benefit_level ids)
  benefits        integer[]   not null default '{}',
  -- Benefits offered and not chosen yet (3, or empty)
  offered         integer[]   not null default '{}',
  relottery_count integer     not null default 0,
  -- Members knocked out during the run (decides the clear rank)
  knockouts       integer     not null default 0,
  -- Set when the last floor is won: the clear is shown once, then the run is removed
  clear_rank      integer     null,
  clear_rewards   jsonb       not null default '[]',
  started_at      timestamptz not null default now()
);

drop table if exists user_dungeon_clears cascade;
create table user_dungeon_clears
(
  user_id    bigint      not null references users (id) on delete cascade,
  area_id    integer     not null,
  -- Best clear rank reached (3 is the best)
  clear_rank integer     not null,
  cleared_at timestamptz not null default now(),
  primary key (user_id, area_id)
);
