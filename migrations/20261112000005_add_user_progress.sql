-- Player rank and quest progress, which used to be fixed (rank 45, every quest cleared).

-- Player rank and experience. The row is created on first login with rank 1.
drop table if exists user_progress cascade;
create table user_progress
(
  user_id bigint  not null primary key references users (id) on delete cascade,
  -- Player rank ("Level" on the client), from the "exp" master
  rank    integer not null default 1 check (rank >= 1),
  -- Total player experience, the "allexp" column of the "exp" master
  exp     integer not null default 0 check (exp >= 0)
);

-- One row per quest stage the user has played (main, free, event and fame stages share ids).
drop table if exists user_quest_progress cascade;
create table user_quest_progress
(
  user_id       bigint      not null references users (id) on delete cascade,
  quest_id      integer     not null,
  -- Times won (0 = played but never won)
  clear_count   integer     not null default 0,
  -- Crowns (quest missions) achieved at least once
  task1         boolean     not null default false,
  task2         boolean     not null default false,
  task3         boolean     not null default false,
  -- Wins and skips today, for stages and areas with a daily limit (day in Japan time)
  daily_count   integer     not null default 0,
  daily_date    date        not null default ((now() at time zone 'Asia/Tokyo')::date),
  primary key (user_id, quest_id)
);

-- New accounts start like the original game: few eris and the quartz for the first gacha.
alter table user_wallet alter column money set default 10000;
alter table user_wallet alter column real_money set default 0;
alter table user_wallet alter column real_money_free set default 3000;
alter table user_wallet alter column stamina set default 10;
