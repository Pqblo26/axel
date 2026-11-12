use crate::api::quest::finish::finish_quest_battle;
use crate::api::battle_multi::{BattleCharacterLove, BattleClearReward, BattleMemberExp};
use crate::api::master_all::get_master_manager;
use crate::api::party_info::{Party, PartyPassiveSkillInfo};
use crate::api::quest::quest_hunting::BattleReward;
use crate::api::quest::stage::stage_info;
use crate::api::quest::QuestRewardItem;
use crate::api::surprise::BasicBattlePartyForm;
use crate::api::{ApiRequest, RemoteData, RemoteDataItemType};
use crate::blob::IntoRemoteData;
use crate::call::{CallCustom, CallResponse};
use crate::extractor::Params;
use crate::handler::{IntoHandlerResponse, Unsigned};
use crate::item::UpdateItemCountBy;
use crate::member::{
  FetchUserMemberSkillsIn, FetchUserMembersIn, FetchUserParty,
};
use crate::user::overrides::FetchUserOverrides;
use crate::user::session::Session;
use crate::user::wallet::Wallet;
use crate::AppState;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

// See [Wonder_Api_BattlestartMembersResponseDto_Fields]
// See [Wonder_Api_SurpriseQuestStartMembersResponseDto_Fields]
// See [Wonder_Api_ScorechallengestartMembersResponseDto_Fields]
// See [Wonder_Api_MarathonMultiStartMembersResponseDto_Fields]
// See [Wonder_Api_MarathonSingleStartMembersResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct BattleMember {
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
}

// See [Wonder_Api_BattlestartResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct BattleStartResponse {
  pub chest: String,
  pub party: BattleParty,
  pub members: Vec<BattleMember>,
}

impl CallCustom for BattleStartResponse {}

// See [Wonder_Api_BattlestartPartyResponseDto_Fields]
// See [Wonder_Api_ScorechallengestartPartyResponseDto_Fields]
// See [Wonder_Api_MarathonMultiStartPartyResponseDto_Fields]
// See [Wonder_Api_MarathonSingleStartPartyResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct BattleParty {
  pub party_forms: [BasicBattlePartyForm; 5],
  pub assist: i32,
  pub sub_assists: Vec<i32>,
  pub party_passive_skill: PartyPassiveSkillInfo,
}

#[derive(Debug, Deserialize)]
pub struct BattleStartRequest {
  pub quest_id: i32,
  #[serde(rename = "party_no")]
  pub party_id: i32,
  pub auto_progression_info: AutoProgressionInfo,
}

#[derive(Debug, Deserialize)]
pub struct AutoProgressionInfo {
  pub is_start: bool,
  pub stop_setting: i32,
  pub incomplete_setting: i32,
}

/// Takes the stamina a quest costs (times [ratio]) from the wallet.
/// The client checks stamina before starting, so if the server has less (out of sync) it takes
/// what there is instead of failing the battle.
pub async fn pay_quest_stamina(
  client: &deadpool_postgres::Client,
  session: &Session,
  quest_id: i32,
  ratio: i32,
) -> anyhow::Result<Vec<RemoteData>> {
  let Some(stage) = stage_info(quest_id) else {
    warn!(?quest_id, "unknown quest stage, no stamina taken");
    return Ok(Vec::new());
  };
  let cost = stage.stamina as i64 * ratio.max(1) as i64;
  let mut wallet = Wallet::load(client, session.user_id).await?;
  if (wallet.stamina as i64) < cost {
    warn!(?quest_id, ?cost, stamina = wallet.stamina, "not enough stamina on the server, taking what there is");
  }
  let taken = cost.min(wallet.stamina as i64);
  wallet.add(RemoteDataItemType::Stamina, -taken)?;
  wallet.save(client, session.user_id).await?;
  info!(?quest_id, ?cost, stamina = wallet.stamina, "stamina spent");
  Ok(wallet.to_remote_data())
}

/// Adds the eris earned in a quest. Returns remote data with the new wallet.
pub async fn pay_quest_money(
  client: &deadpool_postgres::Client,
  session: &Session,
  money: i64,
) -> anyhow::Result<Vec<RemoteData>> {
  let mut wallet = Wallet::load(client, session.user_id).await?;
  wallet.add(RemoteDataItemType::Money, money)?;
  wallet.save(client, session.user_id).await?;
  Ok(wallet.to_remote_data())
}

/// [stamina]: quest id and stamina multiplier to charge, if the battle costs stamina.
pub async fn make_battle_start(
  state: &AppState,
  session: &Session,
  party_id: i32,
  stamina: Option<(i32, i32)>,
) -> impl IntoHandlerResponse + use<> {
  let client = state
    .get_database_client()
    .await
    .context("failed to get database connection")?;
  let stamina_remote = match stamina {
    Some((quest_id, ratio)) => pay_quest_stamina(&client, session, quest_id, ratio).await?,
    None => Vec::new(),
  };
  let party = crate::api::battle::fetch_battle_party(&client, session.user_id, party_id as i64).await?;

  // We must send only members that are used in the party, otherwise hardlock occurs
  #[rustfmt::skip]
  let mut members = FetchUserMembersIn::new(&client)
    .await?
    .run(session.user_id, &party.member_ids())
    .await?;
  FetchUserMemberSkillsIn::new(&client)
    .await?
    .run(session.user_id, &mut members.iter_mut().collect::<Vec<_>>())
    .await?;

  let mut response = CallResponse::new_success(Box::new(BattleStartResponse {
    chest: "10101111,10101120,10101131".to_owned(),
    party: party.to_battle_party(),
    members: members
      .into_iter()
      .filter_map(|member| Some(member.to_battle_member(party.form_of(member.id)?)))
      .collect(),
  }));
  response.remote.extend(stamina_remote);
  Ok(Unsigned(response))
}

/// A party as it fights: without the slots the player's rank has not unlocked yet.
pub async fn fetch_battle_party(
  client: &deadpool_postgres::Client,
  user_id: crate::user::id::UserId,
  party_id: i64,
) -> anyhow::Result<Party> {
  let mut party = FetchUserParty::new(client).await?.run(user_id, party_id).await?;
  let progress = crate::user::progress::PlayerProgress::load(client, user_id).await?;
  party.restrict_to_rank(progress.rank);
  Ok(party)
}

// quest_id=101011
// party_no=1
// auto_progression_info={"is_start":false,"stop_setting":0,"incomplete_setting":0}
pub async fn battle_start(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<BattleStartRequest>,
) -> impl IntoHandlerResponse {
  make_battle_start(&state, &session, params.party_id, Some((params.quest_id, 1))).await
}

#[derive(Debug, Deserialize)]
pub struct LiveMember {
  pub id: i64,
  pub hp: i32,
  pub form_no: i32,
}

#[derive(Debug, Deserialize)]
pub struct ResumeInfo {
  pub resumeMembers: Vec<ResumeMember>,
  pub assistRemainCount: i32,
  pub assistCoolTime: i32,
  pub reportInfo: ReportInfo,
}

// See [Wonder_Battle_ResumeReportInfo_Fields]
#[derive(Debug, Deserialize)]
pub struct ReportInfo {
  pub PendingBonusIds: Vec<i32>,
  pub SuccessBonusIds: Vec<i32>,
}

// See [Wonder_Battle_ResumeMember_Fields]
#[derive(Debug, Deserialize)]
pub struct ResumeMember {
  pub memberId: i64,
  pub spLevel: f32,
  pub skill1Time: i32,
  pub skill2Time: i32,
  pub stateInfoArray: Vec<StateInfo>,
  pub CurrentAgiLevel: f32,
  pub CurrentAgiLevelSecondary: f32,
  pub PassiveInfoArray: Vec<ResumePassiveInfo>,
}

// See [Wonder_Battle_StateInfo_Fields]
#[derive(Debug, Deserialize)]
pub struct StateInfo {
  pub skillId: i64,
  pub AffectValue: i32,
  pub SecondaryAffectValue: i32,
  pub TertiaryAffectValue: i32,
  pub Accuracy: i32,
  pub Attr: String,
  pub TargetNum: i32,
  pub Type: String,
  pub AffectTurn: i32,
  pub AffectTime: f32,
  pub Target: String,
  pub ShakeMode: String,
  pub EffectSize: i32,
  pub BuffRank: i32,
  pub IsBlow: bool,
  pub IsWater: bool,
  pub PassedTime: f32,
  pub AdditionalTime: f32,
  pub Remain: i32,
  pub IntervalTime: f32,
  pub UnitId: i32,
}

// See [Wonder_Battle_ResumePassiveInfo_Fields]
#[derive(Debug, Deserialize)]
pub struct ResumePassiveInfo {
  pub SkillId: i64,
  pub ExecuteCount: i32,
  pub NeedCount: i32,
  pub RemainCount: i32,
  pub PassiveCount: i32,
  pub MaxStackCount: i32,
  pub StackCount: i32,
}

// wave=2
// livemembers=[{"id":11,"hp":245,"form_no":1},{"id":12,"hp":252,"form_no":2},{"id":10,"hp":208,"form_no":3}]
// battletime=12
// resume_info={"resumeMembers":[{"memberId":1001100,"spLevel":0.4000000059604645,"skill1Time":0,"skill2Time":0,"stateInfoArray":[],"CurrentAgiLevel":10023.0,"CurrentAgiLevelSecondary":10008.0,"PassiveInfoArray":[]},{"memberId":1011100,"spLevel":0.5,"skill1Time":0,"skill2Time":0,"stateInfoArray":[],"CurrentAgiLevel":10032.0,"CurrentAgiLevelSecondary":5742.0,"PassiveInfoArray":[]},{"memberId":1064217,"spLevel":0.6000000238418579,"skill1Time":18,"skill2Time":0,"stateInfoArray":[],"CurrentAgiLevel":10005.0,"CurrentAgiLevelSecondary":34.5,"PassiveInfoArray":[]}],"assistRemainCount":0,"assistCoolTime":0,"reportInfo":{"PendingBonusIds":[],"SuccessBonusIds":[]}}
pub async fn battle_wave_result(request: ApiRequest) -> impl IntoHandlerResponse {
  let wave: i32 = request.body["wave"].parse().unwrap();
  let live_members: Vec<LiveMember> = serde_json::from_str(&request.body["livemembers"]).unwrap();
  let battle_time: i64 = request.body["battletime"].parse().unwrap();
  let resume_info: ResumeInfo = serde_json::from_str(&request.body["resume_info"]).unwrap();

  let response: CallResponse<dyn CallCustom> = CallResponse::new_success(Box::new(json!({
    "chest": 10101120
  })));
  Ok(Unsigned(response))
}

// See [Wonder_Api_ResultResponseDto_Fields]
// See [Wonder_Api_BattlemarathonquestresultResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct BattleResultResponse {
  pub limit: i32,
  pub exp: i32,
  pub lvup: i32,
  pub money: i32,
  pub storyunlock: Vec<i32>,
  /// Must contain all characters used in the battle
  pub love: Vec<BattleCharacterLove>,
  /// Must contain all members used in the battle
  pub member_exp: Vec<BattleMemberExp>,
  pub mission: Vec<i32>,
  pub reward: Vec<BattleReward>,
  pub clearreward: Vec<BattleClearReward>,
  pub auto_progression_result: AutoProgressionResultResponse,
  pub firstclear: bool,
}

impl CallCustom for BattleResultResponse {}

#[derive(Debug, Serialize)]
pub struct AutoProgressionResultResponse {
  pub auto_count: i32,
  pub is_continue: bool,
  pub stop_reason: i32,
  pub stamina_all: i32,
  pub reward_all: Vec<BattleReward>,
  pub clearreward_all: Vec<BattleClearReward>,
}

// body={"wave": "3", "party_no": "1", "win": "1", "clearquestmission": "[12,0,0]", "memcheckcount": "0", "quest_id": "104041", "auto_progression_stop": "1"}
#[derive(Debug, Deserialize)]
pub struct BattleResultRequest {
  pub quest_id: i32,
  #[serde(rename = "party_no")]
  pub party_id: i32,
  pub win: i32,
  pub wave: i32,
  pub clearquestmission: Vec<i32>,
  pub auto_progression_stop: i32,
  pub memcheckcount: i32,
}

pub async fn grant_rewards(
  transaction: &deadpool_postgres::Transaction<'_>,
  session: &Session,
  rewards: &[QuestRewardItem],
) -> anyhow::Result<Vec<RemoteData>> {
  let update = UpdateItemCountBy::new(transaction).await?;
  let mut update_items = Vec::new();
  for item in rewards {
    let item = update
      .run(
        session.user_id,
        (RemoteDataItemType::from(item.item_type), item.item_id),
        item.item_num,
      )
      .await
      .context("failed to execute query")?;
    debug!(?item, "granted battle reward");

    update_items.push(item.into_remote_data());
  }

  Ok(update_items.into_iter().flatten().collect())
}

pub async fn apply_reward_multiplier(
  transaction: &deadpool_postgres::Transaction<'_>,
  session: &Session,
  rewards: &mut Vec<QuestRewardItem>,
) -> anyhow::Result<()> {
  let multiplier = FetchUserOverrides::new(transaction)
    .await?
    .run(session.user_id)
    .await?
    .get("battle_reward_multiplier")
    .and_then(|v| v.parse::<f32>().ok());
  if let Some(multiplier) = multiplier {
    info!(?multiplier, "applying battle reward multiplier");
    for item in rewards {
      item.item_num = ((item.item_num as f32) * multiplier).ceil() as i32;
    }
  }

  Ok(())
}

/// [intimacy_gain]: affinity each character in the party earns (saved to the database).
pub async fn make_battle_member_exp_and_character_love(
  party: &Party,
  client: &deadpool_postgres::Client,
  session: &Session,
  intimacy_gain: i32,
) -> anyhow::Result<(Vec<BattleMemberExp>, Vec<BattleCharacterLove>)> {
  // We must send only members that are used in the party, otherwise hardlock occurs
  #[rustfmt::skip]
  let members = FetchUserMembersIn::new(client)
    .await?
    .run(session.user_id, &party.member_ids())
    .await?;

  let characters = party
    .party_forms
    .iter()
    .map(|form| {
      vec![
        members
          .iter()
          .find(|member| member.id == form.main)
          .map(|m| m.prototype.character_id),
        members
          .iter()
          .find(|member| member.id == form.sub1)
          .map(|m| m.prototype.character_id),
        members
          .iter()
          .find(|member| member.id == form.sub2)
          .map(|m| m.prototype.character_id),
      ]
    })
    .flatten()
    .flatten()
    .collect::<Vec<_>>();
  let characters = {
    let mut character_ids = characters.clone();
    character_ids.sort_unstable();
    character_ids.dedup();
    #[rustfmt::skip]
    let statement = client
      .prepare(/* language=postgresql */ r#"
        update user_characters
        set intimacy = intimacy + $3
        where user_id = $1 and character_id = any($2)
        returning character_id, intimacy
      "#)
      .await
      .context("failed to prepare statement")?;
    let rows = client
      .query(&statement, &[&session.user_id, &character_ids, &intimacy_gain.max(0)])
      .await
      .context("failed to execute query")?;
    rows
      .iter()
      .map(|row| {
        let character_id: i64 = row.get(0);
        let intimacy: i32 = row.get(1);
        (character_id, intimacy)
      })
      .filter(|(id, _)| characters.contains(id))
      .collect::<HashMap<_, _>>()
  };

  let member_exp = members
    .iter()
    .map(|member| BattleMemberExp {
      // TODO: I don't know whether client wants user-id or prototype-id here
      member_id: member.id as i64,
      exp: 230,
    })
    .collect();
  let character_love = characters
    .iter()
    .map(|(character_id, intimacy)| BattleCharacterLove {
      character_id: *character_id,
      love: *intimacy,
    })
    .collect();
  Ok((member_exp, character_love))
}

pub async fn battle_result(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<BattleResultRequest>,
) -> impl IntoHandlerResponse {
  let outcome = finish_quest_battle(
    &state,
    &session,
    params.quest_id,
    params.party_id,
    params.win != 0,
    &params.clearquestmission,
  )
  .await?;
  let mut response = CallResponse::new_success(Box::new(BattleResultResponse {
    limit: outcome.limit,
    exp: outcome.exp,
    lvup: outcome.lvup,
    money: outcome.money,
    storyunlock: vec![],
    love: outcome.love,
    member_exp: outcome.member_exp,
    mission: params.clearquestmission,
    reward: outcome.reward,
    clearreward: outcome.clearreward,
    auto_progression_result: AutoProgressionResultResponse {
      auto_count: 0,
      is_continue: false,
      stop_reason: 0,
      stamina_all: 0,
      reward_all: vec![],
      clearreward_all: vec![],
    },
    firstclear: outcome.firstclear,
  }));
  response.remote.extend(outcome.remote);
  Ok(Unsigned(response))
}

// See [Wonder_Api_BattleretireResponseDto_Fields]
pub async fn battle_retire(request: ApiRequest) -> impl IntoHandlerResponse {
  let response: CallResponse<dyn CallCustom> = CallResponse::new_success(Box::new(json!({})));
  Ok(Unsigned(response))
}

// See [Wonder_Api_MarathonSingleStartRequest_Fields]
#[derive(Debug, Deserialize)]
pub struct MarathonSingleStartRequest {
  pub quest_id: i32,
  #[serde(rename = "party_no")]
  pub party_id: i32,
  pub event_id: i32,
  pub ticket_ratio: i32,
}

// See [Wonder_Api_MarathonSingleStartResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct MarathonSingleStartResponse {
  pub chest: String,
  pub party: BattleParty,
  pub members: Vec<BattleMember>,
  pub get_log: bool,
}

impl CallCustom for MarathonSingleStartResponse {}

pub async fn marathon_single_start(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<MarathonSingleStartRequest>,
) -> impl IntoHandlerResponse {
  let mut client = state.pool.get().await.context("failed to get database connection")?;
  let ticket_remote = pay_boss_tickets(&mut client, &session, &params).await?;

  let party = crate::api::battle::fetch_battle_party(&client, session.user_id, params.party_id as i64).await?;

  let fetch_members = FetchUserMembersIn::new(&client).await.unwrap();
  #[rustfmt::skip]
  let mut members = fetch_members.run(
    session.user_id,
    &party.member_ids(),
  ).await.unwrap();
  FetchUserMemberSkillsIn::new(&client)
    .await?
    .run(session.user_id, &mut members.iter_mut().collect::<Vec<_>>())
    .await?;

  let mut response = CallResponse::new_success(Box::new(MarathonSingleStartResponse {
    chest: "10101111,10101120,10101131".to_string(),
    party: party.to_battle_party(),
    // We must send only members that are used in the party, otherwise hardlock occurs?
    members: members
      .into_iter()
      .filter_map(|member| Some(member.to_battle_member(party.form_of(member.id)?)))
      .collect(),
    get_log: true,
  }));
  response.remote.extend(ticket_remote);
  Ok(Unsigned(response))
}

/// Takes the boss tickets an event boss costs: the stage's "bossticket" times the multiplier the
/// player chose, of the event's ticket item. Like stamina, the client checks first, so a shortage
/// on the server only takes what there is.
async fn pay_boss_tickets(
  client: &mut deadpool_postgres::Client,
  session: &Session,
  params: &MarathonSingleStartRequest,
) -> anyhow::Result<Vec<RemoteData>> {
  let masters = get_master_manager();
  let cost = masters
    .get_master("event_marathon_quest_stage_boss_single")
    .iter()
    .find(|row| row["id"].as_str() == Some(&params.quest_id.to_string()))
    .and_then(|row| row["bossticket"].as_str()?.parse::<i32>().ok())
    .unwrap_or(0)
    * params.ticket_ratio.max(1);
  let ticket_id = masters
    .get_master("event_config")
    .iter()
    .find(|row| row["event_id"].as_str() == Some(&params.event_id.to_string()))
    .and_then(|row| row["boss_ticket_id"].as_str()?.parse::<i64>().ok());
  let Some(ticket_id) = ticket_id.filter(|_| cost > 0) else {
    warn!(?params, "event boss without ticket cost");
    return Ok(Vec::new());
  };

  let transaction = client.transaction().await.context("failed to start transaction")?;
  #[rustfmt::skip]
  let owned: i32 = transaction.query_opt(/* language=postgresql */ r#"
    select quantity from user_items where user_id = $1 and item_type = $2 and item_id = $3
  "#, &[&session.user_id, &(i32::from(RemoteDataItemType::BossTicket) as i64), &ticket_id]).await?
    .map(|row| row.get(0))
    .unwrap_or(0);
  if owned < cost {
    warn!(?params, ?owned, ?cost, "not enough boss tickets on the server, taking what there is");
  }
  let item = UpdateItemCountBy::new(&transaction)
    .await?
    .run(session.user_id, (RemoteDataItemType::BossTicket, ticket_id), -cost.min(owned))
    .await?;
  transaction.commit().await.context("failed to commit transaction")?;
  info!(?params, ?cost, left = item.quantity, "boss tickets spent");
  Ok(item.into_remote_data())
}

// See [Wonder_Api_MarathonSingleLogRequest_Fields]
#[derive(Debug, Deserialize)]
pub struct MarathonSingleLogRequest {
  pub event_id: i32,
  pub quest_id: i32,
  #[serde(deserialize_with = "crate::serde_compat::comma_separated_string")]
  pub log: Vec<String>,
}

pub async fn marathon_single_log(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<MarathonSingleLogRequest>,
) -> impl IntoHandlerResponse {
  warn!(?params, "encountered stub: marathon_single_log");

  // See [Wonder_Api_MarathonSingleLogResponseDto_Fields]
  Ok(Unsigned(()))
}

// See [Wonder_Api_MarathonSingleResultRequest_Fields]
#[derive(Debug, Deserialize)]
pub struct MarathonSingleResultRequest {
  pub quest_id: i32,
  #[serde(rename = "party_no")]
  pub party_id: i32,
  pub clearquestmission: Vec<i32>,
  pub win: i32,
  pub wave: i32,
  pub memcheckcount: i32,
  pub event_id: i32,
}

// See [Wonder_Api_MarathonSingleResultResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct MarathonSingleResultResponse {
  pub exp: i32,
  pub lvup: i32,
  pub money: i32,
  pub love: Vec<BattleCharacterLove>,
  pub member_exp: Vec<BattleMemberExp>,
  pub reward: Vec<BattleReward>,
}

impl CallCustom for MarathonSingleResultResponse {}

pub async fn marathon_single_result(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<MarathonSingleResultRequest>,
) -> impl IntoHandlerResponse {
  warn!(?params, "encountered stub: marathon_single_result");

  let client = state.pool.get().await.context("failed to get database connection")?;
  let party = crate::api::battle::fetch_battle_party(&client, session.user_id, params.party_id as i64).await?;

  // Boss stages have no affinity reward in the masters
  let (member_exp, love) = make_battle_member_exp_and_character_love(&party, &client, &session, 0).await?;
  Ok(Unsigned(BattleResultResponse {
    limit: 0,
    exp: 5,
    lvup: 0,
    // Eris are not stored for event bosses (no amount in the masters), don't show any
    money: 0,
    storyunlock: vec![],
    love,
    member_exp,
    mission: params.clearquestmission,
    reward: vec![],
    clearreward: vec![],
    auto_progression_result: AutoProgressionResultResponse {
      auto_count: 0,
      is_continue: false,
      stop_reason: 0,
      stamina_all: 0,
      reward_all: vec![],
      clearreward_all: vec![],
    },
    firstclear: true,
  }))
}
