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
}

fn int(row: &Value, key: &str) -> Option<i32> {
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
      });
    }
  }
  stages
}

pub fn stage_info(quest_id: i32) -> Option<&'static StageInfo> {
  static STAGES: OnceLock<HashMap<i32, StageInfo>> = OnceLock::new();
  STAGES.get_or_init(load).get(&quest_id)
}
