//! Hierarchy is Part (1) -> Area (Chapter 1) -> Stage (Chapter 1-1)

use crate::api::dungeon::BattleSkipReward;
use crate::api::master_all::get_master_manager;
use crate::api::quest::quest_hunting::BattleHuntingSkipRequest;
use crate::api::quest::skip::skip_quests;
use crate::api::NotificationData;
use crate::call::{CallCustom, CallResponse};
use crate::extractor::Params;
use crate::handler::{IntoHandlerResponse, Unsigned};
use crate::user::session::Session;
use crate::AppState;
use serde::{Deserialize, Serialize};
use crate::api::quest::progress::{
  group_status, in_date_window, is_cleared, load_progress, quest_status, stage_unlocked, ProgressMap,
};
use crate::api::quest::stage::{stages_of_kind, StageKind};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::sync::Arc;

// See [Wonder_Api_QuestMainPartListResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct QuestMainPartListResponse {
  pub quests: Vec<QuestMainPartListItem>,
}

impl CallCustom for QuestMainPartListResponse {}

// See [Wonder_Api_QuestMainPartListQuestsResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct QuestMainPartListItem {
  pub quest_part_id: i32,
  /// 0 - unlocked (new), 1 - unlocked, 2 - completed, 3 - 100% completed
  pub status: i32,
}

/// Unlock condition of a part or area row: a cleared stage and a start date.
fn row_unlocked(row: &Value, progress: &ProgressMap, now: DateTime<Utc>) -> bool {
  let required = row["unlock_clearstage"].as_str().and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
  (required == 0 || is_cleared(progress, required))
    && in_date_window(
      row["unlock_date_start"].as_str().unwrap_or("0"),
      row["unlock_date_end"].as_str().unwrap_or("0"),
      now,
    )
}

fn row_int(row: &Value, key: &str) -> i32 {
  row[key].as_str().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Statuses of the unlocked main quest stages, optionally only one mode (1 normal, 2 hard, 3 expert).
fn stage_statuses(progress: &ProgressMap, now: DateTime<Utc>, area_ids: &[i32], mode: Option<i32>) -> Vec<i32> {
  stages_of_kind(StageKind::Main)
    .into_iter()
    .filter(|stage| area_ids.contains(&stage.area_id) && mode.is_none_or(|mode| stage.mode == mode))
    .map(|stage| {
      if stage_unlocked(stage, progress, now) {
        quest_status(progress.get(&stage.id))
      } else {
        // Locked stages keep the group from counting as cleared
        -1
      }
    })
    .collect()
}

pub async fn quest_main_part_list(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let progress = load_progress(&client, session.user_id).await?;
  let now = Utc::now();
  let areas = get_master_manager().get_master("mainquest_area");
  let parts = get_master_manager().get_master("main_quest_part");
  let parts = parts
    .iter()
    .filter(|part| row_unlocked(part, &progress, now))
    .map(|part| {
      let part_id = row_int(part, "part");
      let area_ids = areas
        .iter()
        .filter(|area| row_int(area, "part_id") == part_id)
        .map(|area| row_int(area, "id"))
        .collect::<Vec<_>>();
      QuestMainPartListItem {
        quest_part_id: part_id,
        status: group_status(stage_statuses(&progress, now, &area_ids, Some(1)).into_iter().map(|s| s.max(0))),
      }
    })
    .collect::<Vec<_>>();

  Ok(Unsigned(QuestMainPartListResponse { quests: parts }))
}

// See [Wonder_Api_QuestMainAreaListResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct QuestMainAreaListResponse {
  pub normal_area_list: Vec<QuestMainAreaListItem>,
  pub hard_area_list: Vec<QuestMainAreaListItem>,
  pub expert_area_list: Vec<QuestMainAreaListItem>,
}

impl CallCustom for QuestMainAreaListResponse {}

// See [Wonder_Api_QuestMainAreaResponseDto_Fields]
#[derive(Clone, Debug, Serialize)]
pub struct QuestMainAreaListItem {
  pub quest_area_master_id: i32,
  /// 0 - unlocked (new), 1 - unlocked, 2 - completed, 3 - 100% completed
  pub status: i32,
}

pub async fn quest_main_area_list(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let progress = load_progress(&client, session.user_id).await?;
  let now = Utc::now();
  let areas = get_master_manager().get_master("mainquest_area");

  // An area is listed for a difficulty once one of its stages of that difficulty is unlocked
  let list_for_mode = |mode: i32| {
    areas
      .iter()
      .filter(|area| row_unlocked(area, &progress, now))
      .filter_map(|area| {
        let area_id = row_int(area, "id");
        let statuses = stage_statuses(&progress, now, &[area_id], Some(mode));
        if !statuses.iter().any(|status| *status >= 0) {
          return None;
        }
        Some(QuestMainAreaListItem {
          quest_area_master_id: area_id,
          status: group_status(statuses.into_iter().map(|s| s.max(0))),
        })
      })
      .collect::<Vec<_>>()
  };

  let mut response: CallResponse<dyn CallCustom> = CallResponse::new_success(Box::new(QuestMainAreaListResponse {
    normal_area_list: list_for_mode(1),
    hard_area_list: list_for_mode(2),
    expert_area_list: list_for_mode(3),
  }));
  response.add_notifications(vec![NotificationData::new(1, 7, 20, 0, "".to_owned(), "".to_owned())]);

  Ok(Unsigned(response))
}

// See [Wonder_Api_QuestMainStageListRequest_Fields]
#[derive(Debug, Deserialize)]
pub struct QuestMainStageListRequest {
  pub area_id: i32,
}

// See [Wonder_Api_QuestMainStageListResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct QuestMainStageListResponse {
  pub quests: Vec<QuestMainStageListItem>,
}

impl CallCustom for QuestMainStageListResponse {}

// See [Wonder_Api_QuestMainStageResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct QuestMainStageListItem {
  pub quest_stage_id: i32,
  pub status: i32,
  pub task1: i32,
  pub task2: i32,
  pub task3: i32,
  /// Number of attempts done. Unlimited for Normal stages, 3 for Hard stages, 1 for Expert stages.
  pub challenge_count: i32,
  pub difficulty: i32,
}

pub async fn quest_main_stage_list(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<QuestMainStageListRequest>,
) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let progress = load_progress(&client, session.user_id).await?;
  let now = Utc::now();
  let stages = stages_of_kind(StageKind::Main)
    .into_iter()
    .filter(|stage| stage.area_id == params.area_id && stage_unlocked(stage, &progress, now))
    .map(|stage| {
      let entry = progress.get(&stage.id);
      let tasks = entry.map(|entry| entry.tasks).unwrap_or_default();
      QuestMainStageListItem {
        // The client wants the stage number inside the area, not the full id
        quest_stage_id: stage_number(stage.id),
        status: quest_status(entry),
        task1: tasks[0] as i32,
        task2: tasks[1] as i32,
        task3: tasks[2] as i32,
        challenge_count: entry.map(|entry| entry.daily_count).unwrap_or(0),
        difficulty: stage.mode,
      }
    })
    .collect::<Vec<_>>();

  Ok(Unsigned(QuestMainStageListResponse { quests: stages }))
}

/// Main stage ids are 1 + area (2 digits) + stage number (2 digits) + mode (1 digit), e.g. 101021.
fn stage_number(id: i32) -> i32 {
  (id / 10) % 100
}

// See [Wonder_Api_BattleskipRequest_Fields]
#[derive(Debug, Deserialize)]
pub struct BattleSkipRequest {
  pub skip: Vec<SkipInfoRequestDto>,
  pub splitcount: i32,
  pub max_splitcount: i32,
}

// See [Wonder_Api_SkipInfoRequestDto_Fields]
#[derive(Debug, Deserialize)]
pub struct SkipInfoRequestDto {
  pub quest_id: i32,
  pub skip_count: i32,
}

// See [Wonder_Api_BattleskipResponseDto_Fields]
// See [Wonder_Api_BattlehuntingskipResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct BattleSkipResponse {
  pub lvup: i32,
  pub reward: Vec<BattleSkipReward>,
}

impl CallCustom for BattleSkipResponse {}

pub async fn battle_skip(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<BattleHuntingSkipRequest>,
) -> impl IntoHandlerResponse {
  Ok(Unsigned(skip_quests(&state, &session, &params.skip).await?))
}
