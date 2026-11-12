//! What a quest stage costs and gives, whatever kind of quest it is.
//! Stage ids do not overlap between kinds: 1xxxxx main, 4xxxxx free (hunting), 5xxxxx event,
//! 7xxxxx fame.

use crate::api::master_all::get_master_manager;
use crate::api::quest::{parse_reward_items, QuestRewardItem};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
  Main,
  Hunting,
  Event,
  Marathon,
  Fame,
}

impl StageKind {
  /// (stage master, item reward master, id column in the reward master)
  fn masters(self) -> (&'static str, &'static str, &'static str) {
    match self {
      StageKind::Main => ("mainquest_stage", "mainquest_stage_itemreward", "id"),
      StageKind::Hunting => ("huntingquest_stage", "huntingquest_stage_itemreward", "id"),
      StageKind::Event => ("event_quest_stage", "event_quest_stage_itemreward", "id"),
      StageKind::Marathon => ("event_marathon_quest_stage", "event_quest_stage_itemreward", "id"),
      StageKind::Fame => ("fame_quest_stage", "fame_quest_stage_itemreward", "fame_quest_id"),
    }
  }

  const ALL: [StageKind; 5] = [
    StageKind::Main,
    StageKind::Hunting,
    StageKind::Event,
    StageKind::Marathon,
    StageKind::Fame,
  ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageInfo {
  pub id: i32,
  pub kind: StageKind,
  pub stamina: i32,
  /// Skip tickets per skipped run
  pub skip_tickets: i32,
  pub money: i32,
  pub player_exp: i32,
  pub intimacy_exp: i32,
  /// Times per day it can be played, 0 = no limit
  pub limit: i32,
  pub rewards: Vec<QuestRewardItem>,
  pub area_id: i32,
  /// 1 normal, 2 hard, 3 expert
  pub mode: i32,
  /// Member experience each main member of the party gets
  pub member_exp: i32,
  /// Quest missions (crowns): main mission and the two sub missions
  pub missions: [i32; 3],
  /// Reward pack ("pack" master) given the first time each crown is achieved, 0 = none
  pub crown_packs: [i64; 3],
  /// Stages that must be cleared first, 0 = none
  pub unlock_stages: [i32; 2],
  pub unlock_date_start: String,
}

fn int(row: &Value, key: &str) -> Option<i32> {
  row.get(key)?.as_str()?.parse().ok()
}

fn long(row: &Value, key: &str) -> Option<i64> {
  row.get(key)?.as_str()?.parse().ok()
}

fn load() -> HashMap<i32, StageInfo> {
  let masters = get_master_manager();
  let mut stages = HashMap::new();
  for kind in StageKind::ALL {
    let (stage_master, reward_master, reward_key) = kind.masters();
    let rewards = masters
      .get_master(reward_master)
      .iter()
      .filter_map(|row| Some((int(row, reward_key)?, row)))
      .collect::<HashMap<_, _>>();
    for row in masters.get_master(stage_master) {
      let Some(id) = int(row, "id") else { continue };
      stages.entry(id).or_insert_with(|| StageInfo {
        id,
        kind,
        stamina: int(row, "stamina").unwrap_or(0),
        // Main quest stages have no column for it: one ticket per run
        skip_tickets: int(row, "skipticket").unwrap_or(1),
        money: int(row, "money").unwrap_or(0),
        player_exp: int(row, "player_exp").unwrap_or(0),
        intimacy_exp: int(row, "intimacy_exp").unwrap_or(0),
        limit: int(row, "limit").unwrap_or(0),
        rewards: rewards.get(&id).map(|row| parse_reward_items(row)).unwrap_or_default(),
        area_id: int(row, "area_id").unwrap_or(0),
        mode: int(row, "mode").unwrap_or(1),
        member_exp: int(row, "member_exp").unwrap_or(0),
        missions: [
          int(row, "mainmission").unwrap_or(0),
          int(row, "submission1").unwrap_or(0),
          int(row, "submission2").unwrap_or(0),
        ],
        crown_packs: [
          long(row, "mainmission_reward1_packid").unwrap_or(0),
          long(row, "mainmission_reward2_packid").unwrap_or(0),
          long(row, "mainmission_reward3_packid").unwrap_or(0),
        ],
        unlock_stages: [
          int(row, "unlock_clearstage").unwrap_or(0),
          int(row, "unlock_clearstage2").unwrap_or(0),
        ],
        unlock_date_start: row
          .get("unlock_date_start")
          .and_then(|v| v.as_str())
          .unwrap_or("0")
          .to_owned(),
      });
    }
  }
  stages
}

fn stages() -> &'static HashMap<i32, StageInfo> {
  static STAGES: OnceLock<HashMap<i32, StageInfo>> = OnceLock::new();
  STAGES.get_or_init(load)
}

pub fn stage_info(quest_id: i32) -> Option<&'static StageInfo> {
  stages().get(&quest_id)
}

/// Stages of one kind, sorted by id.
pub fn stages_of_kind(kind: StageKind) -> Vec<&'static StageInfo> {
  let mut list = stages().values().filter(|stage| stage.kind == kind).collect::<Vec<_>>();
  list.sort_by_key(|stage| stage.id);
  list
}
