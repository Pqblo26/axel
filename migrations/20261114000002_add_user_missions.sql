-- Missions screen (master `mission`): missions received and counters that nothing else stores.
drop table if exists user_missions cascade;
create table user_missions
(
  user_id     bigint      not null references users (id) on delete cascade,
  mission_id  integer     not null,
  -- Daily missions are received again each day (Japan time)
  received_at timestamptz not null default now(),
  primary key (user_id, mission_id)
);

drop table if exists user_mission_counters cascade;
create table user_mission_counters
(
  user_id bigint  not null references users (id) on delete cascade,
  -- "login" (one row per day), "gacha", "expedition", "score"
  counter text    not null,
  -- Japan time
  day     date    not null,
  count   integer not null default 0,
  primary key (user_id, counter, day)
);
