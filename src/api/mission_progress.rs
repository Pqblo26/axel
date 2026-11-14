//! Progress of the missions in the Missions screen (master `mission`): Normal, Daily and Beginner.
//! Progress is worked out from what is already stored (quests, rank, members) plus a few
//! counters (`user_mission_counters`); received missions are kept in `user_missions`.
//! List of rules: jq '.[].rule' master/mission.json -r | sort | uniq -c

use crate::api::master_all::get_master_manager;
use crate::api::quest::progress::{japan, load_progress, ProgressMap};
use crate::api::quest::stage::{stage_info, StageKind};
use crate::database::QueryExecutor;
use crate::member::{FetchUserMembers, Member};
use crate::user::id::UserId;
use anyhow::Context;
use chrono::{NaiveDate, Utc};
use serde_json::Value;
use std::collections::HashMap;

/// Counted events that nothing else stores, by API method (see [count_call]).
const COUNTED_CALLS: &[(&str, &str)] = &[
  ("home", "login"),
  ("gachanormal", "gacha"),
  ("gachachain", "gacha"),
  ("gachadaily", "gacha"),
  ("expeditionset", "expedition"),
  ("scorechallengestart", "score"),
  ("missionhome", "home_chara"),
];

pub fn is_counted(method: &str) -> bool {
  COUNTED_CALLS.iter().any(|(name, _)| *name == method)
}

/// Today in Japan time, when daily missions and quest limits reset.
pub fn today() -> NaiveDate {
  Utc::now().with_timezone(&japan()).date_naive()
}

/// Counts a successful API call for the missions that need it. "login" counts days, not calls.
pub async fn count_call(client: &deadpool_postgres::Client, user_id: UserId, method: &str) -> anyhow::Result<()> {
  let Some((_, counter)) = COUNTED_CALLS.iter().find(|(name, _)| *name == method) else {
    return Ok(());
  };
  let increment = if *counter == "login" { 0 } else { 1 };
  #[rustfmt::skip]
  client.execute(/* language=postgresql */ r#"
    insert into user_mission_counters (user_id, counter, day, count) values ($1, $2, $3, 1)
    on conflict (user_id, counter, day) do update set count = user_mission_counters.count + $4
  "#, &[&user_id, counter, &today(), &increment]).await.context("failed to count mission event")?;
  Ok(())
}

#[derive(Debug, Default, Clone, Copy)]
struct Counter {
  total: i32,
  today: i32,
  days: i32,
}

pub struct MissionState {
  progress: ProgressMap,
  rank: i32,
  members: Vec<Member>,
  counters: HashMap<String, Counter>,
  /// Mission id -> day received (Japan time)
  received: HashMap<i32, NaiveDate>,
  today: NaiveDate,
}

pub fn int(row: &Value, key: &str) -> i32 {
  row[key].as_str().and_then(|value| value.parse().ok()).unwrap_or(0)
}

pub fn is_daily(mission: &Value) -> bool {
  mission["mission_type"].as_str() == Some("DAILY")
}

impl MissionState {
  pub async fn load(client: &deadpool_postgres::Client, user_id: UserId) -> anyhow::Result<Self> {
    let progress = load_progress(QueryExecutor::from(client), user_id).await?;
    let rank = crate::user::progress::PlayerProgress::load(client, user_id).await?.rank;
    let members = FetchUserMembers::new(client).await?.run(user_id).await?;
    let today = today();
    #[rustfmt::skip]
    let rows = client.query(/* language=postgresql */ r#"
      select counter, sum(count)::int, coalesce(sum(count) filter (where day = $2), 0)::int, count(*)::int
      from user_mission_counters where user_id = $1 group by counter
    "#, &[&user_id, &today]).await.context("failed to read mission counters")?;
    let counters = rows
      .iter()
      .map(|row| (row.get::<_, String>(0), Counter { total: row.get(1), today: row.get(2), days: row.get(3) }))
      .collect();
    #[rustfmt::skip]
    let rows = client.query(/* language=postgresql */ r#"
      select mission_id, (received_at at time zone 'Asia/Tokyo')::date from user_missions where user_id = $1
    "#, &[&user_id]).await.context("failed to read missions")?;
    let received = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
    Ok(MissionState { progress, rank, members, counters, received, today })
  }

  /// Received (today, for daily missions).
  pub fn is_received(&self, mission: &Value) -> bool {
    self
      .received
      .get(&int(mission, "mission_id"))
      .is_some_and(|day| !is_daily(mission) || *day == self.today)
  }

  fn counter(&self, name: &str) -> Counter {
    self.counters.get(name).copied().unwrap_or_default()
  }

  fn main_stages(&self, mode: i32) -> impl Iterator<Item = (i32, &crate::api::quest::progress::QuestProgress)> + '_ {
    self.progress.iter().filter_map(move |(quest_id, progress)| {
      let stage = stage_info(*quest_id)?;
      (stage.kind == StageKind::Main && (mode == 0 || stage.mode == mode)).then_some((stage.area_id, progress))
    })
  }

  fn daily_of_kind(&self, kind: StageKind, modes: &[i32]) -> i32 {
    self
      .progress
      .iter()
      .filter(|(quest_id, _)| {
        stage_info(**quest_id).is_some_and(|stage| stage.kind == kind && (modes.is_empty() || modes.contains(&stage.mode)))
      })
      .map(|(_, progress)| progress.daily_count)
      .sum()
  }

  fn received_of_type(&self, kind: &str, except_rule: &str) -> i32 {
    get_master_manager()
      .get_master("mission")
      .iter()
      .filter(|mission| mission["mission_type"].as_str() == Some(kind) && mission["rule"].as_str() != Some(except_rule))
      .filter(|mission| self.is_received(mission))
      .count() as i32
  }

  /// Current value of the mission's rule; the client compares it with the target it works out.
  pub fn progress(&self, mission: &Value) -> i32 {
    let rule = mission["rule"].as_str().unwrap_or("");
    let (x, y) = (int(mission, "rule_x"), int(mission, "rule_y"));
    let mode = |name: &str| match name {
      _ if name.contains("hard") => 2,
      _ if name.contains("expert") => 3,
      _ => 1,
    };
    match rule {
      "battle_count_normal_unique" | "battle_count_hard_unique" | "battle_count_expert_unique" => {
        self.progress.get(&y).is_some_and(|progress| progress.clear_count > 0) as i32
      }
      "battle_chapter_normal" | "battle_chapter_hard" | "battle_chapter_expert" => self
        .main_stages(mode(rule))
        .filter(|(area, progress)| *area == x && progress.clear_count > 0)
        .count() as i32,
      "battle_chapter_normal_perfect" | "battle_chapter_hard_perfect" | "battle_chapter_expert_perfect" => self
        .main_stages(mode(rule))
        .filter(|(area, progress)| *area == x && progress.tasks.iter().all(|task| *task))
        .count() as i32,
      "battle_star" => self
        .main_stages(0)
        .map(|(_, progress)| progress.tasks.iter().filter(|task| **task).count() as i32)
        .sum(),
      "battle_count_main" if is_daily(mission) => self.daily_of_kind(StageKind::Main, &[]),
      "battle_count_main" => self.main_stages(0).map(|(_, progress)| progress.clear_count).sum(),
      "battle_count_main_hard" => self.daily_of_kind(StageKind::Main, &[2, 3]),
      "battle_count_hunt" => self.daily_of_kind(StageKind::Hunting, &[]),
      "battle_count_fame_quest" => self.daily_of_kind(StageKind::Fame, &[]),
      "battle_count_expedition" => self.counter("expedition").today,
      "battle_count_score_or_score_ex" => self.counter("score").today,
      "player_lv" => self.rank,
      "member_lv" => self.members.iter().filter(|member| member.level() >= y).count() as i32,
      "member_limitlv" => self.members.iter().filter(|member| member.promotion_level >= y).count() as i32,
      "gacha_count" => self.counter("gacha").total,
      "player_login" => self.counter("login").days,
      "time" => 1,
      "all_daily" => self.received_of_type("DAILY", "all_daily"),
      // The client shows it as done (1/1) with any value: 1 only once every other one is received
      "all_beginner" => {
        let others = get_master_manager()
          .get_master("mission")
          .iter()
          .filter(|row| row["mission_type"].as_str() == Some("BEGINNER") && row["rule"].as_str() != Some("all_beginner"))
          .count() as i32;
        (self.received_of_type("BEGINNER", "all_beginner") >= others) as i32
      }
      "player_home_chara" => (self.counter("home_chara").total > 0) as i32,
      _ => 0,
    }
  }
}

/// Missions (Normal, Daily, Beginner) by id.
pub fn mission_row(mission_id: i32) -> Option<&'static Value> {
  get_master_manager().get_master("mission").iter().find(|mission| int(mission, "mission_id") == mission_id)
}

/// Target the client shows for a rule, when it is the rule's x; None when the client works it out
/// (chapters, single quests, "all" missions).
pub fn target(mission: &Value) -> Option<i32> {
  let rule = mission["rule"].as_str().unwrap_or("");
  match rule {
    "battle_count_normal_unique" | "battle_count_hard_unique" | "battle_count_expert_unique" => Some(1),
    "battle_chapter_normal" | "battle_chapter_hard" | "battle_chapter_expert" | "battle_chapter_normal_perfect"
    | "battle_chapter_hard_perfect" | "battle_chapter_expert_perfect" => {
      let x = int(mission, "rule_x");
      let mode = if rule.contains("hard") {
        2
      } else if rule.contains("expert") {
        3
      } else {
        1
      };
      Some(
        crate::api::quest::stage::stages_of_kind(StageKind::Main)
          .iter()
          .filter(|stage| stage.area_id == x && stage.mode == mode)
          .count() as i32,
      )
    }
    "all_beginner" => Some(1),
    "time" => Some(1),
    _ => Some(int(mission, "rule_x")).filter(|x| *x > 0),
  }
}
