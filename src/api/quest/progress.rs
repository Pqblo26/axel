//! What each user has cleared, stored in `user_quest_progress` (see migration 20261112000005).
//! Main and free quest lists, unlocks, crowns and daily attempt limits are worked out from it.

use crate::api::quest::stage::{stage_info, StageInfo, StageKind};
use crate::database::QueryExecutor;
use crate::user::id::UserId;
use anyhow::Context;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDateTime, TimeZone, Utc, Weekday};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuestProgress {
  pub clear_count: i32,
  pub tasks: [bool; 3],
  /// Wins and skips today (Japan time)
  pub daily_count: i32,
}

/// 0 - unlocked (new), 1 - unlocked (played), 2 - cleared, 3 - cleared with all crowns
pub fn quest_status(progress: Option<&QuestProgress>) -> i32 {
  match progress {
    None => 0,
    Some(progress) if progress.clear_count == 0 => 1,
    Some(progress) if progress.tasks.iter().all(|task| *task) => 3,
    Some(_) => 2,
  }
}

/// Status of a group of stages (an area or a part): cleared when every stage is.
pub fn group_status<'a>(statuses: impl IntoIterator<Item = i32>) -> i32 {
  let statuses = statuses.into_iter().collect::<Vec<_>>();
  if statuses.is_empty() {
    return 0;
  }
  if statuses.iter().all(|status| *status == 3) {
    3
  } else if statuses.iter().all(|status| *status >= 2) {
    2
  } else if statuses.iter().any(|status| *status >= 1) {
    1
  } else {
    0
  }
}

pub type ProgressMap = HashMap<i32, QuestProgress>;

pub fn is_cleared(progress: &ProgressMap, quest_id: i32) -> bool {
  progress.get(&quest_id).is_some_and(|progress| progress.clear_count > 0)
}

/// Japan time, which the original server used for dates in the masters and daily resets.
pub fn japan() -> FixedOffset {
  FixedOffset::east_opt(9 * 3600).unwrap()
}

/// Parses master dates like "2021/8/3 0:00" (Japan time). "0" or empty = no date.
pub fn parse_master_date(value: &str) -> Option<DateTime<Utc>> {
  if value.is_empty() || value == "0" {
    return None;
  }
  let naive = NaiveDateTime::parse_from_str(value, "%Y/%m/%d %H:%M").ok()?;
  Some(japan().from_local_datetime(&naive).single()?.with_timezone(&Utc))
}

/// Whether now is inside a [start, end) window from the masters. Missing dates do not limit.
pub fn in_date_window(start: &str, end: &str, now: DateTime<Utc>) -> bool {
  parse_master_date(start).is_none_or(|start| start <= now) && parse_master_date(end).is_none_or(|end| now < end)
}

/// Whether an area that only opens some days ("sun".."sat" columns set to 1) is open today.
pub fn open_today(row: &serde_json::Value, now: DateTime<Utc>) -> bool {
  let key = match now.with_timezone(&japan()).weekday() {
    Weekday::Sun => "sun",
    Weekday::Mon => "mon",
    Weekday::Tue => "tue",
    Weekday::Wed => "wed",
    Weekday::Thu => "thu",
    Weekday::Fri => "fri",
    Weekday::Sat => "sat",
  };
  row.get(key).and_then(|v| v.as_str()) != Some("0")
}

/// A stage can be played once the stages it depends on are cleared and its start date has passed.
pub fn stage_unlocked(stage: &StageInfo, progress: &ProgressMap, now: DateTime<Utc>) -> bool {
  stage
    .unlock_stages
    .iter()
    .all(|&required| required == 0 || is_cleared(progress, required))
    && in_date_window(&stage.unlock_date_start, "0", now)
}

/// Daily attempts of a main quest stage: Normal unlimited, Hard 3, Expert 1.
pub fn main_stage_daily_limit(stage: &StageInfo) -> i32 {
  match (stage.kind, stage.mode) {
    (StageKind::Main, 2) => 3,
    (StageKind::Main, 3) => 1,
    _ => 0,
  }
}

pub async fn load_progress<'a>(executor: impl Into<QueryExecutor<'a>>, user_id: UserId) -> anyhow::Result<ProgressMap> {
  let executor = executor.into();
  #[rustfmt::skip]
  let rows = executor.client().query(/* language=postgresql */ r#"
    select quest_id, clear_count, task1, task2, task3,
           case when daily_date = (now() at time zone 'Asia/Tokyo')::date then daily_count else 0 end
    from user_quest_progress
    where user_id = $1
  "#, &[&user_id]).await.context("failed to read quest progress")?;
  Ok(
    rows
      .iter()
      .map(|row| {
        (
          row.get::<_, i32>(0),
          QuestProgress {
            clear_count: row.get(1),
            tasks: [row.get(2), row.get(3), row.get(4)],
            daily_count: row.get(5),
          },
        )
      })
      .collect(),
  )
}

/// What changed when a battle or skip was recorded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordOutcome {
  /// The stage had never been won before
  pub first_clear: bool,
  /// Crowns achieved for the first time
  pub new_tasks: [bool; 3],
  pub progress: QuestProgress,
}

/// Records [wins] wins (0 for a lost battle) with the crowns achieved, and adds [wins] to today's
/// count for daily limits.
pub async fn record_quest<'a>(
  executor: impl Into<QueryExecutor<'a>>,
  user_id: UserId,
  quest_id: i32,
  wins: i32,
  tasks: [bool; 3],
) -> anyhow::Result<RecordOutcome> {
  let executor = executor.into();
  let client = executor.client();
  #[rustfmt::skip]
  let before = client.query_opt(/* language=postgresql */ r#"
    select clear_count, task1, task2, task3
    from user_quest_progress
    where user_id = $1 and quest_id = $2
    for update
  "#, &[&user_id, &quest_id]).await.context("failed to read quest progress")?;
  let (before_clears, before_tasks) = before
    .map(|row| (row.get::<_, i32>(0), [row.get(1), row.get(2), row.get(3)]))
    .unwrap_or((0, [false; 3]));

  #[rustfmt::skip]
  let row = client.query_one(/* language=postgresql */ r#"
    insert into user_quest_progress (user_id, quest_id, clear_count, task1, task2, task3, daily_count)
    values ($1, $2, $3, $4, $5, $6, $3)
    on conflict (user_id, quest_id) do update set
      clear_count = user_quest_progress.clear_count + excluded.clear_count,
      task1 = user_quest_progress.task1 or excluded.task1,
      task2 = user_quest_progress.task2 or excluded.task2,
      task3 = user_quest_progress.task3 or excluded.task3,
      daily_count = case
        when user_quest_progress.daily_date = (now() at time zone 'Asia/Tokyo')::date
          then user_quest_progress.daily_count + excluded.daily_count
        else excluded.daily_count
      end,
      daily_date = (now() at time zone 'Asia/Tokyo')::date
    returning clear_count, task1, task2, task3, daily_count
  "#, &[&user_id, &quest_id, &wins.max(0), &tasks[0], &tasks[1], &tasks[2]])
    .await
    .context("failed to save quest progress")?;
  let progress = QuestProgress {
    clear_count: row.get(0),
    tasks: [row.get(1), row.get(2), row.get(3)],
    daily_count: row.get(4),
  };

  let mut new_tasks = [false; 3];
  for index in 0..3 {
    new_tasks[index] = progress.tasks[index] && !before_tasks[index];
  }
  Ok(RecordOutcome {
    first_clear: before_clears == 0 && progress.clear_count > 0,
    new_tasks,
    progress,
  })
}

/// Crowns achieved in a battle, from the client's "clearquestmission" (mission id per crown, 0 = not
/// achieved).
pub fn tasks_from_missions(clearquestmission: &[i32]) -> [bool; 3] {
  let mut tasks = [false; 3];
  for (index, mission) in clearquestmission.iter().take(3).enumerate() {
    tasks[index] = *mission != 0;
  }
  tasks
}

/// Attempts used today in a free quest area with a daily limit (sum over its stages).
pub fn area_daily_count(progress: &ProgressMap, kind: StageKind, area_id: i32) -> i32 {
  progress
    .iter()
    .filter(|(quest_id, _)| stage_info(**quest_id).is_some_and(|stage| stage.kind == kind && stage.area_id == area_id))
    .map(|(_, progress)| progress.daily_count)
    .sum()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn statuses() {
    assert_eq!(quest_status(None), 0);
    let played = QuestProgress::default();
    assert_eq!(quest_status(Some(&played)), 1);
    let cleared = QuestProgress {
      clear_count: 1,
      tasks: [true, false, true],
      daily_count: 0,
    };
    assert_eq!(quest_status(Some(&cleared)), 2);
    let crowned = QuestProgress {
      tasks: [true; 3],
      ..cleared
    };
    assert_eq!(quest_status(Some(&crowned)), 3);
    assert_eq!(group_status([3, 3]), 3);
    assert_eq!(group_status([3, 2]), 2);
    assert_eq!(group_status([2, 0]), 1);
    assert_eq!(group_status([0, 0]), 0);
  }

  #[test]
  fn master_dates() {
    let date = parse_master_date("2021/8/3 0:00").unwrap();
    assert_eq!(date.to_rfc3339(), "2021-08-02T15:00:00+00:00");
    assert!(parse_master_date("0").is_none());
    let now = Utc::now();
    assert!(in_date_window("0", "0", now));
    assert!(in_date_window("2021/8/3 0:00", "0", now));
    assert!(!in_date_window("2123/8/11 15:00", "2123/8/12 0:00", now));
    assert!(!in_date_window("2021/9/14 0:00", "2021/9/19 0:00", now));
  }

  #[test]
  fn tasks() {
    assert_eq!(tasks_from_missions(&[12, 0, 15]), [true, false, true]);
    assert_eq!(tasks_from_missions(&[]), [false; 3]);
  }
}
