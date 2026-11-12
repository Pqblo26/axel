-- Battle Arena (score challenge) scores, which used to be faked. Attempts per day are counted in
-- user_quest_progress like other quests.

drop table if exists user_score_challenge_best cascade;
create table user_score_challenge_best
(
  user_id    bigint      not null references users (id) on delete cascade,
  -- Arena stage (scorechallenge_stage id: one per arena and difficulty)
  quest_id   integer     not null,
  best_score bigint      not null default 0,
  updated_at timestamptz not null default now(),
  primary key (user_id, quest_id)
);

-- The arena battle in progress, to know at the result whether it was a practice.
drop table if exists user_score_challenge_runs cascade;
create table user_score_challenge_runs
(
  user_id     bigint      not null primary key references users (id) on delete cascade,
  quest_id    integer     not null,
  is_practice boolean     not null,
  started_at  timestamptz not null default now()
);
