//! Reference: https://youtu.be/5xRIW8bDzc4, https://youtu.be/ViMtYimwca4

use crate::api::battle::BattleParty;
use crate::api::exchange::{get_pack, pack_contents};
use crate::api::master_all::get_master_manager;
use crate::api::party_info::{PartyPassiveSkillInfo, SpecialSkillInfo};
use crate::api::quest::parse_reward_items;
use crate::api::quest::quest_hunting::BattleReward;
use crate::api::surprise::BasicBattlePartyForm;
use crate::api::RemoteDataItemType;
use crate::blob::IntoRemoteData;
use crate::call::{CallCustom, CallResponse};
use crate::database::QueryExecutor;
use crate::extractor::Params;
use crate::handler::{IntoHandlerResponse, Unsigned};
use crate::item::UpdateItemCountBy;
use crate::member::{FetchUserMemberSkillsIn, FetchUserMembers, Member};
use crate::user::grant::grant_items;
use crate::user::id::UserId;
use crate::user::session::Session;
use crate::user::wallet::Wallet;
use crate::AppState;
use anyhow::Context;
use chrono::{DateTime, Utc};
use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

// See [Wonder_Api_DungeonStatusResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonStatusResponse {
  pub area_id: i32,
}

impl CallCustom for DungeonStatusResponse {}


// See [Wonder_Api_DungeonAreaTopResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonAreaTopResponse {
  pub is_practice: bool,
  pub stage_state: DungeonStageState,
  pub party_set: DungeonPartySet,
  pub clear_info: DungeonAreaClearInfo,
  /// Must have 3 elements; otherwise, client incorrectly displays empty slots.
  /// NULLs are not allowed.
  pub unchoosed_benefit_id_list: [i32; 3],
  /// Number of redraws used (not remaining).
  pub benefit_re_lottery_count: i32,
  pub is_allow_trial: bool,
}

impl CallCustom for DungeonAreaTopResponse {}

// See [Wonder_Api_DungeonAreaClearInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonAreaClearInfo {
  pub clear_rank: i32,
  pub reward_items: Vec<DungeonAreaClearRewardInfo>,
}

// See [Wonder_Api_DungeonAreaClearRewardInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonAreaClearRewardInfo {
  pub item_type: i32,
  pub item_id: i64,
  pub item_num: i32,
}

// See [Wonder_Api_DungeonStageStateResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonStageState {
  pub stage_id: i32,
  /// Whether to resume in-progress battle.
  pub is_challenge: bool,
  pub enemies: Vec<DungeonStageEnemyState>,
}

// See [Wonder_Api_DungeonStageEnemyStateResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonStageEnemyState {
  pub enemy_id: i32,
  pub current_hp: i32,
}

// See [Wonder_Api_DungeonEnemyInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonEnemyInfo {
  pub enemy_id: i64,
  pub current_hp: i32,
}

// See [Wonder_Api_DungeonPartySetResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonPartySet {
  pub stage_party_set: DungeonStagePartySet,
  pub team_members: Vec<PartyMember>,
  pub team_weapons: Vec<PartyWeapon>,
  pub team_accessories: Vec<PartyAccessory>,
}

// See [Wonder_Api_PartyinfoMembersResponseDto_Fields]
// Extended by [Wonder_Api_DungeonPartyMembersResponseDto_Fields]
// See [Wonder_Api_PartychangeMembersResponseDto_Fields]
#[derive(Debug, Serialize, Deserialize)]
pub struct PartyMember {
  pub id: i32,
  pub lv: i32,
  pub exp: i32,
  pub member_id: i64,
  pub ac_skill_lv_a: i32,
  pub ac_skill_val_a: i64,
  pub ac_skill_lv_b: i32,
  pub ac_skill_val_b: i64,
  pub ac_skill_lv_c: i32,
  pub ac_skill_val_c: i64,
  pub hp: i32,
  pub attack: i32,
  pub magicattack: i32,
  pub defense: i32,
  pub magicdefence: i32,
  pub agility: i32,
  pub dexterity: i32,
  pub luck: i32,
  pub limit_break: i32,
  pub character_id: i64,
  pub waiting_room: i32,
  pub ex_flg: i32,
  pub is_undead: i32,
}

// See [Wonder_Api_PartyinfoWeaponsResponseDto_Fields]
// Extended by [Wonder_Api_DungeonPartyWeaponsResponseDto_Fields]
// See [Wonder_Api_PartychangeWeaponsResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct PartyWeapon {
  pub id: i64,
  pub weapon_id: i64,
  pub trial: bool,
}

// See [Wonder_Api_PartyinfoAccessoriesResponseDto_Fields]
// See [Wonder_Api_DungeonPartyAccessoriesResponseDto_Fields]
// See [Wonder_Api_PartychangelistAccessoriesResponseDto_Fields]
// See [Wonder_Api_PartychangeAccessoriesResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct PartyAccessory {
  pub id: i64,
  pub accessory_id: i64,
}

// See [Wonder_Api_DungeonStagePartySetResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonStagePartySet {
  pub party: Vec<DungeonStagePartyForm>,
  pub reserved_party: Vec<DungeonStagePartyForm>,
  pub assist: i64,
  pub sub_assists: Vec<i64>,
  pub assist_remain_count: i32,
  pub party_passive_skill: PartyPassiveSkillInfo,
}

// See [Wonder_Api_DungeonStagePartyFormResponseDto_Fields]
// extends [Wonder_Api_DungeonPartyFormResponseDto_Fields]
// extends [Wonder_Api_BasicPartyFormResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonStagePartyForm {
  /* Wonder_Api_BasicPartyFormResponseDto_Fields */
  pub id: i32,
  pub form_no: i32,
  pub main: i32,
  pub sub1: i32,
  pub sub2: i32,
  pub weapon: i64,
  pub acc: i64,
  pub strength: i32,
  pub specialskill: SpecialSkillInfo,
  pub skill_pa_fame: i64,

  /* Wonder_Api_DungeonStagePartyFormResponseDto_Fields */
  pub current_hp: i32,
  pub max_hp: i32,
  pub current_sp: i32,
}

#[derive(Debug, Deserialize)]
pub struct DungeonAreaTopRequest {
  pub area_id: i32,
}



#[derive(Debug, Deserialize)]
pub struct DungeonTopRequest {
  pub dungeon_id: i32,
}

// See [Wonder_Api_DungeonTopResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonTopResponse {
  pub challenging_area_id: i32,
  pub area_info_list: Vec<DungeonAreaInfo>,
  pub challenge_count_info: DungeonChallengeCountInfo,
  pub has_new_dungeon: bool,
}

impl CallCustom for DungeonTopResponse {}

// See [Wonder_Api_DungeonAreaInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonAreaInfo {
  pub area_id: i32,
  pub clear_rank: i32,
  pub is_challengeable: bool,
}

// See [Wonder_Api_DungeonChallengeCountInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonChallengeCountInfo {
  pub bought_count: i32,
  pub available_buy_count: i32,
}


// See [Wonder_Api_DungeonListResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonListResponse {
  pub dungeon: DungeonInfo,
  pub regular_dungeons: RegularDungeonsInfo,
}

impl CallCustom for DungeonListResponse {}

// See [Wonder_Api_DungeonInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonInfo {
  pub dungeon_id: i32,
  pub is_completed: bool,
  pub is_new: bool,
}

// See [Wonder_Api_RegularDungeonsInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct RegularDungeonsInfo {
  pub dungeon_list: Vec<DungeonInfo>,
  pub challenge_count_info: DungeonChallengeCountInfo,
}


// See [Wonder_Api_DungeonTeamInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonTeamInfoResponse {
  pub team_set: DungeonTeamSet,
}

impl CallCustom for DungeonTeamInfoResponse {}

// See [Wonder_Api_DungeonTeamSetResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonTeamSet {
  /// Must be of size [master<dungeon_area>.member_num]
  pub party: Vec<DungeonStagePartyForm>,
  pub members: Vec<PartyMember>,
  pub weapons: Vec<PartyWeapon>,
  pub accessories: Vec<PartyAccessory>,
  pub assist: i64,
  pub sub_assists: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct DungeonTeamInfoRequest {
  pub area_id: i32,
}




// See [Wonder_Api_DungeonAreaChallengeResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonAreaChallengeResponse {
  pub stage_state: DungeonStageState,
  pub party_set: DungeonPartySet,
}

impl CallCustom for DungeonAreaChallengeResponse {}

#[derive(Debug, Deserialize)]
pub struct DungeonAreaChallengeRequest {
  pub area_id: i32,
  #[serde(with = "crate::bool_as_int")]
  pub is_practice: bool,
}


// See [Wonder_Api_DungeonStagePartyInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonStagePartyInfoResponse {
  pub party_set: DungeonPartySet,
  pub is_allow_trial: bool,
}

impl CallCustom for DungeonStagePartyInfoResponse {}

#[derive(Debug, Deserialize)]
pub struct DungeonStagePartyInfoRequest {
  pub area_id: i32,
}


// See [Wonder_Api_DungeonBattleStartResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonBattleStartResponse {
  pub chest: String,
  pub party: BattleParty,
  pub members: Vec<DungeonBattleMember>,
  pub resume_info: String,
  pub livemembers: Vec<DungeonBattleLiveMember>,
  pub benefit_id_list: Vec<i32>,
  pub enemy_info: Vec<DungeonEnemyInfo>,
  pub hp_increase: i32,
}

impl CallCustom for DungeonBattleStartResponse {}

// See [Wonder_Api_DungeonBattleStartMembersResponseDto_Fields]
// extends [Wonder_Api_BattlestartMembersResponseDto_Fields]
#[derive(Debug, Clone, Serialize)]
pub struct DungeonBattleMember {
  /* Wonder_Api_BattlestartMembersResponseDto_Fields */
  pub id: i32,
  pub lv: i32,
  pub exp: i32,
  pub member_id: i64,
  pub ac_skill_id_a: i64,
  pub ac_skill_lv_a: i32,
  pub ac_skill_val_a: i32,
  pub ac_skill_id_b: i64,
  pub ac_skill_lv_b: i32,
  pub ac_skill_val_b: i32,
  pub ac_skill_id_c: i64,
  pub ac_skill_lv_c: i32,
  pub ac_skill_val_c: i32,
  pub hp: i32,
  pub magicattack: i32,
  pub defense: i32,
  pub magicdefence: i32,
  pub agility: i32,
  pub dexterity: i32,
  pub luck: i32,
  pub limit_break: i32,
  pub character_id: i64,
  pub passiveskill: i64,
  pub specialattack: i64,
  pub resist_state: i32,
  pub resist_attr: i64,
  pub attack: i32,
  pub ex_flg: i32,
  pub is_undead: i32,
  pub special_skill_lv: i32,

  /* Wonder_Api_DungeonBattleStartMembersResponseDto_Fields */
  pub character_piece_board_stage_id_list: Vec<i32>,
}

// See [Wonder_Api_BattleresumeLivemembersResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonBattleLiveMember {
  pub id: i32,
  pub hp: i32,
  pub form_no: i32,
}

#[derive(Debug, Deserialize)]
pub struct DungeonBattleStartRequest {
  pub stage_id: i32,
}


#[derive(Debug, Deserialize)]
pub struct DungeonBattleDefeatRequest {
  #[serde(with = "crate::bool_as_int")]
  pub is_retire: bool,
  pub assist_remain_count: i32,
  pub enemy_info: Vec<DungeonEnemyInfoRequest>,
}

// See [Wonder_Api_DungeonEnemyInfoRequestDto_Fields]
#[derive(Debug, Deserialize)]
pub struct DungeonEnemyInfoRequest {
  pub enemy_id: i64,
  pub hp: i32,
}


// See [Wonder_Api_DungeonTeamOfferResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonTeamOfferResponse {
  pub team_set: DungeonTeamSet,
}

impl CallCustom for DungeonTeamOfferResponse {}

// body={"area_id": "11", "assist": "1", "sub": "1", "accessory_priority_resistances": "[\"none\",\"none\"]", "elemental": "[\"none\",\"none\"]", "priority_status": "strength", "equip": "1", "main": "1", "skill_pa_fame": "1", "weapon_priority_status": "attack"}
#[derive(Debug, Deserialize)]
pub struct DungeonTeamOfferRequest {
  pub area_id: i32,
  pub assist: i64,
  pub sub: i64,
  pub accessory_priority_resistances: Vec<String>,
  pub elemental: Vec<String>,
  pub priority_status: String,
  pub equip: i64,
  pub main: i64,
  pub skill_pa_fame: i64,
  pub weapon_priority_status: String,
}


// See [Wonder_Api_DungeonTeamResetResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonTeamResetResponse {
  pub team_set: DungeonTeamSet,
}

impl CallCustom for DungeonTeamResetResponse {}

#[derive(Debug, Deserialize)]
pub struct DungeonTeamResetRequest {
  pub area_id: i32,
}


// See [Wonder_Api_DungeonAreaSkipResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonAreaSkipResponse {
  pub reward: Vec<BattleSkipReward>,
}

impl CallCustom for DungeonAreaSkipResponse {}

// See [Wonder_Api_BattleskipRewardResponseDto_Fields]
// See [Wonder_Api_BattlehuntingskipRewardResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct BattleSkipReward {
  /// Stage number
  pub dropnum: i32,
  pub exp: i32,
  pub money: i32,
  pub itemtype: i32,
  pub itemid: i32,
  /// Negative values are not displayed :(
  pub itemnum: i32,
}

#[derive(Debug, Deserialize)]
pub struct DungeonAreaSkipRequest {
  pub area_id: i32,
  pub skip_count: i32,
}


// See [Wonder_Api_DungeonBattleResultResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonBattleResultResponse {
  pub reward: Vec<BattleReward>,
}

impl CallCustom for DungeonBattleResultResponse {}

// See [Wonder_Api_BattlewaveresultLivemembersRequestDto_Fields]
#[derive(Debug, Deserialize)]
pub struct DungeonBattleResultLiveMemberRequest {
  pub id: i32,
  pub hp: i32,
  pub form_no: i32,
}

// See [Wonder_Api_DungeonBattleResultRequest_Fields]
#[derive(Debug, Deserialize)]
pub struct DungeonBattleResultRequest {
  pub stage_id: i32,
  pub livemembers: Vec<DungeonBattleResultLiveMemberRequest>,
  pub resume_info: String,
}


// See [Wonder_Api_DungeonSelectBenefitRequest_Fields]
#[derive(Debug, Deserialize)]
pub struct DungeonSelectBenefitRequest {
  pub benefit_id: i32,
}

// See [Wonder_Api_DungeonSelectBenefitResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonSelectBenefitResponse {
  pub revival_character_id: i64,
}

impl CallCustom for DungeonSelectBenefitResponse {}


// See [Wonder_Api_DungeonBenefitListResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonBenefitListResponse {
  pub benefit_id_list: Vec<i32>,
}

impl CallCustom for DungeonBenefitListResponse {}

/// Currently active benefits.

// See [Wonder_Api_DungeonBenefitReLotteryResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct DungeonBenefitRelotteryResponse {
  pub benefit_id_list: [i32; 3],
}

impl CallCustom for DungeonBenefitRelotteryResponse {}


// ---------------------------------------------------------------------------------------------
// Dungeon runs (see migration 20261112000006_add_user_dungeons.sql).
//
// A dungeon has 3 areas (difficulties), each with 5 floors (stages). The player enters an area
// with a team of `member_num` members; HP carries over between floors, knocked out members stay
// out, and after each floor 3 benefits are offered. Winning the last floor clears the area with a
// rank that depends on how many members were knocked out.

fn int(row: &Value, key: &str) -> i32 {
  row[key].as_str().and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn area_row(area_id: i32) -> anyhow::Result<&'static Value> {
  get_master_manager()
    .get_master("dungeon_area")
    .iter()
    .find(|row| int(row, "id") == area_id)
    .with_context(|| format!("unknown dungeon area {area_id}"))
}

fn stage_row(stage_id: i32) -> anyhow::Result<&'static Value> {
  get_master_manager()
    .get_master("dungeon_stage")
    .iter()
    .find(|row| int(row, "id") == stage_id)
    .with_context(|| format!("unknown dungeon stage {stage_id}"))
}

/// Floors of an area in order (each one unlocks the next).
fn area_stages(area_id: i32) -> Vec<&'static Value> {
  let mut stages = get_master_manager()
    .get_master("dungeon_stage")
    .iter()
    .filter(|row| int(row, "area_id") == area_id)
    .collect::<Vec<_>>();
  stages.sort_by_key(|row| int(row, "id"));
  stages
}

fn next_stage(stage_id: i32) -> Option<i32> {
  get_master_manager()
    .get_master("dungeon_stage")
    .iter()
    .find(|row| int(row, "unlock_clear_stage") == stage_id)
    .map(|row| int(row, "id"))
}

/// Enemies of a floor, as (enemy id, max HP), from its wave in "battle_wave" and "battle_enemy".
fn stage_enemies(stage: &Value) -> Vec<(i64, i32)> {
  let masters = get_master_manager();
  let wave_id = stage["wave_id1"].as_str().unwrap_or("0");
  let Some(wave) = masters
    .get_master("battle_wave")
    .iter()
    .find(|row| row["wave_id"].as_str() == Some(wave_id))
  else {
    warn!(?wave_id, "dungeon wave not found");
    return Vec::new();
  };
  (1..=5)
    .filter_map(|index| {
      let enemy_id: i64 = wave[format!("enemy_id{index}").as_str()].as_str()?.parse().ok()?;
      if enemy_id == 0 {
        return None;
      }
      let hp = masters
        .get_master("battle_enemy")
        .iter()
        .find(|row| row["id"].as_str().and_then(|v| v.parse::<i64>().ok()) == Some(enemy_id))
        .map(|row| int(row, "hp"))
        .unwrap_or(1);
      Some((enemy_id, hp))
    })
    .collect()
}

/// The event dungeon that is open: the original ran one for a week at a time and they are all
/// past now, so they take turns by week. Only dungeons with banner images in the asset mirror
/// (1-10 and 15-27; 11-14 and 99 show a pink placeholder).
fn current_dungeon_id(now: DateTime<Utc>) -> i32 {
  let ids = get_master_manager()
    .get_master("dungeon")
    .iter()
    .filter(|row| int(row, "type") == 1 && int(row, "id") != 99)
    .map(|row| int(row, "id"))
    .collect::<Vec<_>>();
  if ids.is_empty() {
    return 1;
  }
  let week = now.timestamp().div_euclid(7 * 24 * 3600) as usize;
  ids[week % ids.len()]
}

/// 3 benefits of an area, for an area that is only being looked at.
fn preview_benefits(area: &Value) -> [i32; 3] {
  let mut list = [0; 3];
  for (index, id) in roll_benefits(area, &[]).into_iter().take(3).enumerate() {
    list[index] = id;
  }
  list
}

/// Benefits offered after a floor: 3 different level 1 benefits of the area's group, not counting
/// the ones already chosen.
fn roll_benefits(area: &Value, chosen: &[i32]) -> Vec<i32> {
  // Benefit types are <lot group (2 digits)><slot 1-3><number>, and the game shows one benefit of
  // each slot, in slot order (the slot is the icon_type column). A list in another order froze it.
  // A type already chosen comes back at its next level.
  let group = area["benefit_lot_group"].as_str().unwrap_or("0").to_owned();
  let levels = get_master_manager().get_master("dungeon_benefit_level");
  let level_of = |id: i32| levels.iter().find(|row| int(row, "id") == id).map(|row| (row["benefit_type"].as_str().unwrap_or("").to_owned(), int(row, "level")));
  let chosen_levels = chosen.iter().filter_map(|id| level_of(*id)).collect::<Vec<_>>();
  (1..=3)
    .map(|slot| {
      let prefix = format!("{group}{slot}");
      let candidates = levels
        .iter()
        .filter(|row| row["benefit_type"].as_str().is_some_and(|t| t.starts_with(&prefix)))
        .filter(|row| {
          let benefit_type = row["benefit_type"].as_str().unwrap_or("");
          let next = chosen_levels
            .iter()
            .filter(|(chosen_type, _)| chosen_type == benefit_type)
            .map(|(_, level)| level + 1)
            .max()
            .unwrap_or(1);
          int(row, "level") == next
        })
        .map(|row| int(row, "id"))
        .collect::<Vec<_>>();
      candidates.choose(&mut rand::rng()).copied().unwrap_or(0)
    })
    .collect()
}

fn benefit_info(benefit_id: i32) -> Option<(String, i32)> {
  let masters = get_master_manager();
  let level = masters
    .get_master("dungeon_benefit_level")
    .iter()
    .find(|row| int(row, "id") == benefit_id)?;
  let benefit_type = level["benefit_type"].as_str()?;
  let category = masters
    .get_master("dungeon_benefit")
    .iter()
    .find(|row| row["type"].as_str() == Some(benefit_type))?["type_category"]
    .as_str()?
    .to_owned();
  Some((category, int(level, "value")))
}

#[derive(Debug, Clone)]
struct DungeonRun {
  area_id: i32,
  stage_id: i32,
  is_practice: bool,
  members: Vec<i64>,
  member_hp: HashMap<i64, i32>,
  enemy_hp: HashMap<i64, i32>,
  benefits: Vec<i32>,
  offered: Vec<i32>,
  relottery_count: i32,
  knockouts: i32,
  clear_rank: Option<i32>,
  clear_rewards: Vec<(i32, i64, i32)>,
}

fn json_map(value: &Value) -> HashMap<i64, i32> {
  value
    .as_object()
    .map(|object| {
      object
        .iter()
        .filter_map(|(key, value)| Some((key.parse().ok()?, value.as_i64()? as i32)))
        .collect()
    })
    .unwrap_or_default()
}

fn map_json(map: &HashMap<i64, i32>) -> Value {
  Value::Object(map.iter().map(|(key, value)| (key.to_string(), Value::from(*value))).collect())
}

impl DungeonRun {
  async fn load<'a>(executor: impl Into<QueryExecutor<'a>>, user_id: UserId) -> anyhow::Result<Option<Self>> {
    let executor = executor.into();
    #[rustfmt::skip]
    let row = executor.client().query_opt(/* language=postgresql */ r#"
      select area_id, stage_id, is_practice, members, member_hp, enemy_hp, benefits, offered,
             relottery_count, knockouts, clear_rank, clear_rewards
      from user_dungeon_runs
      where user_id = $1
    "#, &[&user_id]).await.context("failed to read dungeon run")?;
    Ok(row.map(|row| DungeonRun {
      area_id: row.get(0),
      stage_id: row.get(1),
      is_practice: row.get(2),
      members: row.get(3),
      member_hp: json_map(&row.get::<_, Value>(4)),
      enemy_hp: json_map(&row.get::<_, Value>(5)),
      benefits: row.get(6),
      offered: row.get(7),
      relottery_count: row.get(8),
      knockouts: row.get(9),
      clear_rank: row.get(10),
      clear_rewards: serde_json::from_value(row.get::<_, Value>(11)).unwrap_or_default(),
    }))
  }

  async fn save<'a>(&self, executor: impl Into<QueryExecutor<'a>>, user_id: UserId) -> anyhow::Result<()> {
    let executor = executor.into();
    #[rustfmt::skip]
    executor.client().execute(/* language=postgresql */ r#"
      insert into user_dungeon_runs (user_id, area_id, stage_id, is_practice, members, member_hp, enemy_hp,
                                     benefits, offered, relottery_count, knockouts, clear_rank, clear_rewards)
      values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
      on conflict (user_id) do update set
        area_id = excluded.area_id, stage_id = excluded.stage_id, is_practice = excluded.is_practice,
        members = excluded.members, member_hp = excluded.member_hp, enemy_hp = excluded.enemy_hp,
        benefits = excluded.benefits, offered = excluded.offered, relottery_count = excluded.relottery_count,
        knockouts = excluded.knockouts, clear_rank = excluded.clear_rank, clear_rewards = excluded.clear_rewards
    "#, &[
      &user_id, &self.area_id, &self.stage_id, &self.is_practice, &self.members,
      &map_json(&self.member_hp), &map_json(&self.enemy_hp), &self.benefits, &self.offered,
      &self.relottery_count, &self.knockouts, &self.clear_rank, &serde_json::to_value(&self.clear_rewards)?,
    ]).await.context("failed to save dungeon run")?;
    Ok(())
  }

  async fn delete<'a>(executor: impl Into<QueryExecutor<'a>>, user_id: UserId) -> anyhow::Result<()> {
    let executor = executor.into();
    #[rustfmt::skip]
    executor.client().execute(/* language=postgresql */ r#"
      delete from user_dungeon_runs where user_id = $1
    "#, &[&user_id]).await.context("failed to delete dungeon run")?;
    Ok(())
  }

  fn offered_list(&self) -> [i32; 3] {
    let mut list = [0; 3];
    for (index, id) in self.offered.iter().take(3).enumerate() {
      list[index] = *id;
    }
    list
  }

  fn stage_state(&self) -> anyhow::Result<DungeonStageState> {
    let stage = stage_row(self.stage_id)?;
    Ok(DungeonStageState {
      stage_id: self.stage_id,
      is_challenge: false,
      enemies: stage_enemies(stage)
        .into_iter()
        .map(|(enemy_id, hp)| DungeonStageEnemyState {
          enemy_id: enemy_id as i32,
          current_hp: *self.enemy_hp.get(&enemy_id).unwrap_or(&hp),
        })
        .collect(),
    })
  }
}

/// Team members (with skills), strongest first, for a new run: `member_num` of the user's members.
async fn load_team(client: &deadpool_postgres::Client, user_id: UserId, member_ids: Option<&[i64]>, member_num: usize) -> anyhow::Result<Vec<Member>> {
  let mut members = FetchUserMembers::new(client).await?.run(user_id).await?;
  FetchUserMemberSkillsIn::new(client)
    .await?
    .run(user_id, &mut members.iter_mut().collect::<Vec<_>>())
    .await?;
  match member_ids {
    Some(ids) => {
      let mut team = Vec::new();
      for id in ids {
        if let Some(index) = members.iter().position(|member| member.id as i64 == *id) {
          team.push(members.swap_remove(index));
        }
      }
      Ok(team)
    }
    None => {
      // The team chosen with Suggest, if any; otherwise the strongest members
      #[rustfmt::skip]
      let saved: Vec<i64> = client.query_opt(/* language=postgresql */ r#"
        select members from user_dungeon_teams where user_id = $1
      "#, &[&user_id]).await.context("failed to read dungeon team")?.map(|row| row.get(0)).unwrap_or_default();
      let mut team = Vec::new();
      for id in saved {
        if let Some(index) = members.iter().position(|member| member.id as i64 == id) {
          team.push(members.swap_remove(index));
        }
      }
      members.sort_by_key(|member| std::cmp::Reverse(member.main_strength_value()));
      fill_team(&mut team, members, member_num);
      Ok(team)
    }
  }
}

/// Completes a team up to [member_num] from [candidates] in order, one member per character.
fn fill_team(team: &mut Vec<Member>, candidates: Vec<Member>, member_num: usize) {
  team.truncate(member_num);
  for member in candidates {
    if team.len() >= member_num {
      break;
    }
    if team.iter().all(|other| other.prototype.character_id != member.prototype.character_id) {
      team.push(member);
    }
  }
}

async fn save_team(client: &deadpool_postgres::Client, user_id: UserId, team: &[Member]) -> anyhow::Result<()> {
  let ids = team.iter().map(|member| member.id as i64).collect::<Vec<_>>();
  #[rustfmt::skip]
  client.execute(/* language=postgresql */ r#"
    insert into user_dungeon_teams (user_id, members) values ($1, $2)
    on conflict (user_id) do update set members = excluded.members, updated_at = now()
  "#, &[&user_id, &ids]).await.context("failed to save dungeon team")?;
  Ok(())
}

fn max_hp(member: &Member) -> i32 {
  member.to_party_member().hp
}

/// Party for the next battle: members still standing first (the first 5 fight), then the ones down.
fn build_party_set(team: &[Member], run: Option<&DungeonRun>) -> DungeonPartySet {
  let current_hp = |member: &Member| {
    run
      .and_then(|run| run.member_hp.get(&(member.id as i64)).copied())
      .unwrap_or_else(|| max_hp(member))
  };
  let form = |(index, member): (usize, &Member)| DungeonStagePartyForm {
    id: 1,
    form_no: index as i32 + 1,
    main: member.id,
    sub1: 0,
    sub2: 0,
    weapon: 0,
    acc: 0,
    strength: member.main_strength_value(),
    specialskill: SpecialSkillInfo {
      special_skill_id: crate::api::party_info::special_skill_for_member(member.id as i64, 0),
      trial: false,
    },
    skill_pa_fame: 0,
    current_hp: current_hp(member),
    max_hp: max_hp(member),
    current_sp: 0,
  };
  let (standing, down): (Vec<&Member>, Vec<&Member>) = team.iter().partition(|member| current_hp(member) > 0);
  let ordered = standing.into_iter().chain(down).collect::<Vec<_>>();
  DungeonPartySet {
    stage_party_set: DungeonStagePartySet {
      // The whole team goes in "party" (the game takes the first 5 into battle); a filled
      // reserved_party freezes the game when entering an area
      party: ordered.iter().copied().enumerate().map(form).collect(),
      reserved_party: vec![],
      assist: 0,
      sub_assists: vec![],
      assist_remain_count: 0,
      party_passive_skill: Default::default(),
    },
    team_members: team.iter().map(|member| member.to_party_member()).collect(),
    team_weapons: vec![],
    team_accessories: vec![],
  }
}

fn team_set_from(party_set: DungeonPartySet) -> DungeonTeamSet {
  let mut party = party_set.stage_party_set.party;
  party.extend(party_set.stage_party_set.reserved_party);
  DungeonTeamSet {
    party,
    members: party_set.team_members,
    weapons: vec![],
    accessories: vec![],
    assist: 0,
    sub_assists: vec![],
  }
}

async fn load_clears<'a>(executor: impl Into<QueryExecutor<'a>>, user_id: UserId) -> anyhow::Result<HashMap<i32, i32>> {
  let executor = executor.into();
  #[rustfmt::skip]
  let rows = executor.client().query(/* language=postgresql */ r#"
    select area_id, clear_rank from user_dungeon_clears where user_id = $1
  "#, &[&user_id]).await.context("failed to read dungeon clears")?;
  Ok(rows.iter().map(|row| (row.get(0), row.get(1))).collect())
}

/// Always "no area in progress". With an area id the game skips dungeon_list and dungeon_top and
/// freezes resuming the area; through dungeon_top (challenging_area_id) it resumes it fine.
pub async fn dungeon_status(_session: Arc<Session>) -> impl IntoHandlerResponse {
  Ok(Unsigned(CallResponse::new_success(Box::new(DungeonStatusResponse { area_id: 0 }))))
}

pub async fn dungeon_list(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let clears = load_clears(&client, session.user_id).await?;
  let dungeon_id = current_dungeon_id(Utc::now());
  let areas = get_master_manager()
    .get_master("dungeon_area")
    .iter()
    .filter(|area| int(area, "dungeon_id") == dungeon_id)
    .map(|area| int(area, "id"))
    .collect::<Vec<_>>();

  Ok(Unsigned(DungeonListResponse {
    dungeon: DungeonInfo {
      dungeon_id,
      is_completed: !areas.is_empty() && areas.iter().all(|area| clears.contains_key(area)),
      is_new: !areas.iter().any(|area| clears.contains_key(area)),
    },
    // The permanent dungeons (11-14) have no images in the asset mirror
    regular_dungeons: RegularDungeonsInfo {
      dungeon_list: vec![],
      challenge_count_info: DungeonChallengeCountInfo {
        bought_count: 0,
        available_buy_count: 0,
      },
    },
  }))
}

pub async fn dungeon_top(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonTopRequest>,
) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let clears = load_clears(&client, session.user_id).await?;
  let run = DungeonRun::load(&client, session.user_id).await?;
  let areas = get_master_manager()
    .get_master("dungeon_area")
    .iter()
    .filter(|area| int(area, "dungeon_id") == params.dungeon_id)
    .collect::<Vec<_>>();

  Ok(Unsigned(DungeonTopResponse {
    challenging_area_id: run
      .map(|run| run.area_id)
      .filter(|area_id| areas.iter().any(|area| int(area, "id") == *area_id))
      .unwrap_or(0),
    area_info_list: areas
      .iter()
      .map(|area| {
        let unlock_id = int(area, "unlock_id");
        DungeonAreaInfo {
          area_id: int(area, "id"),
          clear_rank: clears.get(&int(area, "id")).copied().unwrap_or(0),
          is_challengeable: unlock_id == 0 || clears.contains_key(&unlock_id),
        }
      })
      .collect(),
    challenge_count_info: DungeonChallengeCountInfo {
      bought_count: 0,
      available_buy_count: 0,
    },
    has_new_dungeon: false,
  }))
}

pub async fn dungeon_team_info(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonTeamInfoRequest>,
) -> impl IntoHandlerResponse {
  let member_num = int(area_row(params.area_id)?, "member_num").max(1) as usize;
  let client = state.get_database_client().await?;
  let run = DungeonRun::load(&client, session.user_id).await?.filter(|run| run.area_id == params.area_id);
  let team = load_team(&client, session.user_id, run.as_ref().map(|run| run.members.as_slice()), member_num).await?;
  Ok(Unsigned(DungeonTeamInfoResponse {
    team_set: team_set_from(build_party_set(&team, run.as_ref())),
  }))
}

pub async fn dungeon_team_offer(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonTeamOfferRequest>,
) -> impl IntoHandlerResponse {
  let member_num = int(area_row(params.area_id)?, "member_num").max(1) as usize;
  let client = state.get_database_client().await?;
  let mut members = FetchUserMembers::new(&client).await?.run(session.user_id).await?;
  FetchUserMemberSkillsIn::new(&client)
    .await?
    .run(session.user_id, &mut members.iter_mut().collect::<Vec<_>>())
    .await?;
  // Members of the chosen elements first, then by the chosen stat, one per character
  let elements = params
    .elemental
    .iter()
    .filter(|element| *element != "none" && !element.is_empty())
    .cloned()
    .collect::<Vec<_>>();
  members.sort_by_cached_key(|member| {
    let matches = !elements.is_empty()
      && crate::api::party::member_elements(member).iter().any(|element| elements.contains(element));
    (
      std::cmp::Reverse(matches),
      std::cmp::Reverse(crate::api::party::suggest_stat(member, &params.priority_status)),
    )
  });
  let mut team = Vec::new();
  fill_team(&mut team, members, member_num);
  save_team(&client, session.user_id, &team).await?;
  info!(area_id = params.area_id, ?elements, stat = %params.priority_status, team = ?team.iter().map(|m| m.id).collect::<Vec<_>>(), "dungeon team suggested");
  Ok(Unsigned(DungeonTeamOfferResponse {
    team_set: team_set_from(build_party_set(&team, None)),
  }))
}

pub async fn dungeon_team_reset(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonTeamResetRequest>,
) -> impl IntoHandlerResponse {
  let member_num = int(area_row(params.area_id)?, "member_num").max(1) as usize;
  let client = state.get_database_client().await?;
  #[rustfmt::skip]
  client.execute(/* language=postgresql */ r#"
    delete from user_dungeon_teams where user_id = $1
  "#, &[&session.user_id]).await.context("failed to reset dungeon team")?;
  let team = load_team(&client, session.user_id, None, member_num).await?;
  Ok(Unsigned(DungeonTeamResetResponse {
    team_set: team_set_from(build_party_set(&team, None)),
  }))
}

/// Enters an area: the run starts on its first floor with the strongest team and a full party.
pub async fn dungeon_area_challenge(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonAreaChallengeRequest>,
) -> impl IntoHandlerResponse {
  let area = area_row(params.area_id)?;
  let member_num = int(area, "member_num").max(1) as usize;
  let first_stage = area_stages(params.area_id).first().map(|stage| int(stage, "id")).context("area has no floors")?;
  let client = state.get_database_client().await?;
  let team = load_team(&client, session.user_id, None, member_num).await?;
  let run = DungeonRun {
    area_id: params.area_id,
    stage_id: first_stage,
    is_practice: params.is_practice,
    members: team.iter().map(|member| member.id as i64).collect(),
    member_hp: HashMap::new(),
    enemy_hp: HashMap::new(),
    benefits: vec![],
    // The area starts by choosing a benefit, like after each floor
    offered: roll_benefits(area_row(params.area_id)?, &[]),
    relottery_count: 0,
    knockouts: 0,
    clear_rank: None,
    clear_rewards: vec![],
  };
  run.save(&client, session.user_id).await?;
  info!(area_id = params.area_id, stage_id = first_stage, "dungeon run started");

  Ok(Unsigned(DungeonAreaChallengeResponse {
    stage_state: run.stage_state()?,
    party_set: build_party_set(&team, Some(&run)),
  }))
}

pub async fn dungeon_area_top(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonAreaTopRequest>,
) -> impl IntoHandlerResponse {
  let area = area_row(params.area_id)?;
  let member_num = int(area, "member_num").max(1) as usize;
  let client = state.get_database_client().await?;
  let run = DungeonRun::load(&client, session.user_id).await?.filter(|run| run.area_id == params.area_id);
  let Some(run) = run else {
    // Not in this area: show its first floor with a fresh team
    let team = load_team(&client, session.user_id, None, member_num).await?;
    let first_stage = area_stages(params.area_id).first().map(|stage| int(stage, "id")).unwrap_or(0);
    let preview = DungeonRun {
      area_id: params.area_id,
      stage_id: first_stage,
      is_practice: false,
      members: vec![],
      member_hp: HashMap::new(),
      enemy_hp: HashMap::new(),
      benefits: vec![],
      offered: vec![],
      relottery_count: 0,
      knockouts: 0,
      clear_rank: None,
      clear_rewards: vec![],
    };
    return Ok(Unsigned(DungeonAreaTopResponse {
      is_practice: false,
      stage_state: preview.stage_state()?,
      party_set: build_party_set(&team, None),
      clear_info: DungeonAreaClearInfo {
        clear_rank: 0,
        reward_items: vec![],
      },
      unchoosed_benefit_id_list: preview_benefits(area),
      benefit_re_lottery_count: 0,
      is_allow_trial: true,
    }));
  };
  let mut run = run;
  // The game freezes on a benefit list of zeros: there must always be 3 benefits to choose from
  if run.clear_rank.is_none() && run.offered.is_empty() {
    run.offered = roll_benefits(area, &run.benefits);
    run.save(&client, session.user_id).await?;
  }

  let team = load_team(&client, session.user_id, Some(&run.members), member_num).await?;
  let clear_info = DungeonAreaClearInfo {
    clear_rank: run.clear_rank.unwrap_or(0),
    reward_items: run
      .clear_rewards
      .iter()
      .map(|&(item_type, item_id, item_num)| DungeonAreaClearRewardInfo {
        item_type,
        item_id,
        item_num,
      })
      .collect(),
  };
  if run.clear_rank.is_some() {
    // The clear has been shown: the run is over
    DungeonRun::delete(&client, session.user_id).await?;
  }
  Ok(Unsigned(DungeonAreaTopResponse {
    is_practice: run.is_practice,
    stage_state: run.stage_state()?,
    party_set: build_party_set(&team, Some(&run)),
    clear_info,
    unchoosed_benefit_id_list: if run.offered.is_empty() { preview_benefits(area) } else { run.offered_list() },
    benefit_re_lottery_count: run.relottery_count,
    is_allow_trial: true,
  }))
}

pub async fn dungeon_area_retire(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  DungeonRun::delete(&client, session.user_id).await?;
  info!("dungeon run retired");
  // See [Wonder_Api_DungeonAreaRetireResponseDto_Fields]
  Ok(Unsigned(()))
}

pub async fn dungeon_stage_party_info(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonStagePartyInfoRequest>,
) -> impl IntoHandlerResponse {
  let member_num = int(area_row(params.area_id)?, "member_num").max(1) as usize;
  let client = state.get_database_client().await?;
  let run = DungeonRun::load(&client, session.user_id).await?.filter(|run| run.area_id == params.area_id);
  let team = load_team(&client, session.user_id, run.as_ref().map(|run| run.members.as_slice()), member_num).await?;
  Ok(Unsigned(DungeonStagePartyInfoResponse {
    party_set: build_party_set(&team, run.as_ref()),
    is_allow_trial: true,
  }))
}

pub async fn dungeon_battle_start(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonBattleStartRequest>,
) -> impl IntoHandlerResponse {
  let stage = stage_row(params.stage_id)?;
  let area_id = int(stage, "area_id");
  let member_num = int(area_row(area_id)?, "member_num").max(1) as usize;
  let client = state.get_database_client().await?;
  let run = DungeonRun::load(&client, session.user_id).await?.filter(|run| run.area_id == area_id);
  if run.as_ref().is_some_and(|run| run.stage_id != params.stage_id) {
    warn!(stage_id = params.stage_id, "dungeon battle on a floor that is not the current one");
  }
  let team = load_team(&client, session.user_id, run.as_ref().map(|run| run.members.as_slice()), member_num).await?;
  let party_set = build_party_set(&team, run.as_ref());
  let party = &party_set.stage_party_set.party;
  let fighting = party
    .iter()
    .filter(|form| form.current_hp > 0)
    .filter_map(|form| team.iter().find(|member| member.id == form.main))
    .collect::<Vec<_>>();

  let mut forms = (1..=5)
    .map(|form_no| BasicBattlePartyForm {
      party_no: 1,
      id: 1,
      form_no,
      main: 0,
      sub1: 0,
      sub2: 0,
      weapon: 0,
      acc: 0,
      skill_pa_fame: 0,
    })
    .collect::<Vec<_>>();
  for (index, member) in fighting.iter().take(5).enumerate() {
    forms[index].main = member.id;
  }

  Ok(Unsigned(DungeonBattleStartResponse {
    chest: "".to_string(),
    party: BattleParty {
      party_forms: forms.try_into().map_err(|_| anyhow::anyhow!("party must have 5 forms"))?,
      assist: 0,
      sub_assists: vec![],
      party_passive_skill: Default::default(),
    },
    members: fighting.iter().take(5).map(|member| member.to_dungeon_battle_member()).collect(),
    resume_info: "".to_string(),
    livemembers: party
      .iter()
      .filter(|form| form.current_hp > 0)
      .take(5)
      .enumerate()
      .map(|(index, form)| DungeonBattleLiveMember {
        id: form.main,
        hp: form.current_hp,
        form_no: index as i32 + 1,
      })
      .collect(),
    benefit_id_list: run.as_ref().map(|run| run.benefits.clone()).unwrap_or_default(),
    enemy_info: stage_enemies(stage)
      .into_iter()
      .map(|(enemy_id, hp)| DungeonEnemyInfo {
        enemy_id,
        current_hp: run
          .as_ref()
          .and_then(|run| run.enemy_hp.get(&enemy_id).copied())
          .unwrap_or(hp),
      })
      .collect(),
    hp_increase: 0,
  }))
}

/// Lost (or gave up) a floor: the members that fought are knocked out, the enemies keep the damage.
/// The run ends when nobody is left.
pub async fn dungeon_battle_defeat(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonBattleDefeatRequest>,
) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let Some(mut run) = DungeonRun::load(&client, session.user_id).await? else {
    return Ok(Unsigned(()));
  };
  let member_num = int(area_row(run.area_id)?, "member_num").max(1) as usize;
  let team = load_team(&client, session.user_id, Some(&run.members), member_num).await?;
  let party_set = build_party_set(&team, Some(&run));
  // The first 5 members still standing were the ones fighting
  for form in party_set.stage_party_set.party.iter().filter(|form| form.current_hp > 0).take(5) {
    run.member_hp.insert(form.main as i64, 0);
    run.knockouts += 1;
  }
  for enemy in &params.enemy_info {
    run.enemy_hp.insert(enemy.enemy_id, enemy.hp.max(0));
  }
  let standing = team
    .iter()
    .filter(|member| run.member_hp.get(&(member.id as i64)).is_none_or(|hp| *hp > 0))
    .count();
  if params.is_retire || standing == 0 {
    DungeonRun::delete(&client, session.user_id).await?;
    info!(standing, "dungeon run lost");
  } else {
    run.save(&client, session.user_id).await?;
    info!(standing, knockouts = run.knockouts, "dungeon floor lost, reserves continue");
  }
  Ok(Unsigned(()))
}

pub async fn dungeon_battle_result(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonBattleResultRequest>,
) -> impl IntoHandlerResponse {
  let stage = stage_row(params.stage_id)?;
  let area_id = int(stage, "area_id");
  let area = area_row(area_id)?;
  let rewards = get_master_manager()
    .get_master("dungeon_stage_item_reward")
    .iter()
    .find(|reward| int(reward, "id") == params.stage_id)
    .map(parse_reward_items)
    .unwrap_or_default();

  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;
  let mut run = DungeonRun::load(&transaction, session.user_id).await?.filter(|run| run.area_id == area_id);
  let mut wallet = Wallet::load(&transaction, session.user_id).await?;
  let items = rewards.iter().map(|item| (item.item_type, item.item_id, item.item_num)).collect::<Vec<_>>();
  let practice = run.as_ref().is_some_and(|run| run.is_practice);
  let mut remote = if practice {
    Vec::new()
  } else {
    grant_items(&transaction, session.user_id, &mut wallet, &items).await?
  };

  if let Some(run) = run.as_mut() {
    for live in &params.livemembers {
      let before = run.member_hp.get(&(live.id as i64)).copied();
      if live.hp <= 0 && before.is_none_or(|hp| hp > 0) {
        run.knockouts += 1;
      }
      run.member_hp.insert(live.id as i64, live.hp.max(0));
    }
    run.enemy_hp.clear();
    match next_stage(params.stage_id) {
      Some(next) => {
        run.stage_id = next;
        run.offered = roll_benefits(area, &run.benefits);
      }
      None => {
        // Last floor: clear rank 3 is the best (fewest knockouts)
        let ranks = get_master_manager()
          .get_master("dungeon_clear_rank")
          .iter()
          .filter(|row| row["clear_rank_group"].as_str() == area["clear_rank_group"].as_str())
          .map(|row| (int(row, "clear_rank"), int(row, "knockoutcount"), row["pack_id"].as_str().unwrap_or("0").to_owned()))
          .collect::<Vec<_>>();
        let rank = ranks
          .iter()
          .filter(|(_, max_knockouts, _)| run.knockouts <= *max_knockouts)
          .map(|(rank, _, _)| *rank)
          .max()
          .unwrap_or(1);
        let clears = load_clears(&transaction, session.user_id).await?;
        let previous = clears.get(&area_id).copied().unwrap_or(0);
        // Each rank's reward is given the first time it is reached
        let mut clear_rewards = Vec::new();
        if !run.is_practice {
          for (rank_value, _, pack_id) in &ranks {
            if *rank_value > previous && *rank_value <= rank {
              clear_rewards.extend(get_pack(pack_id).map(pack_contents).unwrap_or_default());
            }
          }
          remote.extend(grant_items(&transaction, session.user_id, &mut wallet, &clear_rewards).await?);
          #[rustfmt::skip]
          transaction.execute(/* language=postgresql */ r#"
            insert into user_dungeon_clears (user_id, area_id, clear_rank) values ($1, $2, $3)
            on conflict (user_id, area_id) do update
              set clear_rank = greatest(user_dungeon_clears.clear_rank, excluded.clear_rank), cleared_at = now()
          "#, &[&session.user_id, &area_id, &rank]).await.context("failed to save dungeon clear")?;
        }
        run.clear_rank = Some(rank);
        run.clear_rewards = clear_rewards;
        run.offered = vec![];
        info!(area_id, rank, knockouts = run.knockouts, "dungeon area cleared");
      }
    }
    run.save(&transaction, session.user_id).await?;
  }
  wallet.save(&transaction, session.user_id).await?;
  transaction.commit().await.context("failed to commit transaction")?;

  let mut response = CallResponse::new_success(Box::new(DungeonBattleResultResponse {
    reward: rewards
      .iter()
      .map(|item| BattleReward {
        itemtype: item.item_type,
        itemid: item.item_id,
        itemnum: item.item_num,
        is_rare: item.item_rare,
      })
      .collect(),
  }));
  response.remote.extend(remote);
  response.remote.extend(wallet.to_remote_data());
  Ok(Unsigned(response))
}

/// Picks one of the 3 offered benefits. Healing and revival apply now; the rest go to the battles.
pub async fn dungeon_select_benefit(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonSelectBenefitRequest>,
) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let mut revival_character_id = 0;
  if let Some(mut run) = DungeonRun::load(&client, session.user_id).await? {
    let member_num = int(area_row(run.area_id)?, "member_num").max(1) as usize;
    let team = load_team(&client, session.user_id, Some(&run.members), member_num).await?;
    match benefit_info(params.benefit_id) {
      Some((category, value)) if category == "HP_RECOVERY" => {
        for member in &team {
          let id = member.id as i64;
          let max = max_hp(member);
          if let Some(hp) = run.member_hp.get(&id).copied().filter(|hp| *hp > 0) {
            run.member_hp.insert(id, (hp + max * value / 100).min(max));
          }
        }
      }
      Some((category, _)) if category == "REVIVAL" => {
        if let Some(member) = team.iter().find(|member| run.member_hp.get(&(member.id as i64)) == Some(&0)) {
          run.member_hp.remove(&(member.id as i64));
          revival_character_id = member.prototype.character_id;
        }
      }
      Some(_) => run.benefits.push(params.benefit_id),
      None => warn!(benefit_id = params.benefit_id, "unknown dungeon benefit"),
    }
    run.offered = vec![];
    run.save(&client, session.user_id).await?;
  }
  Ok(Unsigned(DungeonSelectBenefitResponse { revival_character_id }))
}

pub async fn dungeon_benefit_list(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let run = DungeonRun::load(&client, session.user_id).await?;
  Ok(Unsigned(DungeonBenefitListResponse {
    benefit_id_list: run.map(|run| run.benefits).unwrap_or_default(),
  }))
}

pub async fn dungeon_benefit_re_lottery(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let mut offered = [0; 3];
  if let Some(mut run) = DungeonRun::load(&client, session.user_id).await? {
    run.offered = roll_benefits(area_row(run.area_id)?, &run.benefits);
    run.relottery_count += 1;
    run.save(&client, session.user_id).await?;
    offered = run.offered_list();
  }
  Ok(Unsigned(DungeonBenefitRelotteryResponse { benefit_id_list: offered }))
}

/// Skipping a cleared area: costs its skip tickets and gives every floor's drops.
pub async fn dungeon_area_skip(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<DungeonAreaSkipRequest>,
) -> impl IntoHandlerResponse {
  let area = area_row(params.area_id)?;
  let count = params.skip_count.max(0);
  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;
  let empty = || DungeonAreaSkipResponse { reward: vec![] };
  if !load_clears(&transaction, session.user_id).await?.contains_key(&params.area_id) || count == 0 {
    warn!(?params, "dungeon area not cleared, cannot skip");
    return Ok(Unsigned(CallResponse::new_success(Box::new(empty()))));
  }
  let tickets = int(area, "skip_ticket") * count;
  let update = UpdateItemCountBy::new(&transaction).await?;
  #[rustfmt::skip]
  let owned: i32 = transaction.query_opt(/* language=postgresql */ r#"
    select quantity from user_items where user_id = $1 and item_type = $2 and item_id = 1
  "#, &[&session.user_id, &(i32::from(RemoteDataItemType::SkipTicket) as i64)]).await?
    .map(|row| row.get(0))
    .unwrap_or(0);
  if owned < tickets {
    warn!(?params, owned, tickets, "not enough skip tickets for the dungeon");
    return Ok(Unsigned(CallResponse::new_success(Box::new(empty()))));
  }
  let mut remote = Vec::new();
  if tickets > 0 {
    remote.extend(update.run(session.user_id, (RemoteDataItemType::SkipTicket, 1), -tickets).await?.into_remote_data());
  }

  let mut wallet = Wallet::load(&transaction, session.user_id).await?;
  let reward_rows = get_master_manager().get_master("dungeon_stage_item_reward");
  let mut reward = Vec::new();
  for run in 0..count {
    for stage in area_stages(params.area_id) {
      let items = reward_rows
        .iter()
        .find(|row| int(row, "id") == int(stage, "id"))
        .map(parse_reward_items)
        .unwrap_or_default();
      let granted = items.iter().map(|item| (item.item_type, item.item_id, item.item_num)).collect::<Vec<_>>();
      remote.extend(grant_items(&transaction, session.user_id, &mut wallet, &granted).await?);
      reward.extend(items.iter().map(|item| BattleSkipReward {
        dropnum: run + 1,
        exp: 0,
        money: 0,
        itemtype: item.item_type,
        itemid: item.item_id as i32,
        itemnum: item.item_num,
      }));
    }
  }
  wallet.save(&transaction, session.user_id).await?;
  transaction.commit().await.context("failed to commit transaction")?;

  let mut response = CallResponse::new_success(Box::new(DungeonAreaSkipResponse { reward }));
  response.remote.extend(remote);
  response.remote.extend(wallet.to_remote_data());
  Ok(Unsigned(response))
}
