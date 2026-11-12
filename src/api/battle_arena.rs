//! Reference: https://youtu.be/kbyEkBIw4-U

use crate::api::battle::{BattleMember, BattleParty, BattleStartResponse};
use crate::api::party_info::{Party, PartyForm, SpecialSkillInfo};
use crate::api::surprise::BasicBattlePartyForm;
use crate::api::{MemberFameStats, NotificationData, RemoteDataItemType};
use crate::call::{CallCustom, CallResponse};
use crate::extractor::Params;
use crate::handler::{IntoHandlerResponse, Unsigned};
use crate::member::{FetchUserMemberSkillsIn, FetchUserMembers, FetchUserMembersIn, FetchUserParty, Member, MemberActiveSkill, MemberPrototype, MemberStrength};
use crate::user::session::Session;
use crate::AppState;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};
use crate::api::exchange::{get_pack, pack_contents};
use crate::api::master_all::get_master_manager;
use crate::api::quest::progress::{load_progress, parse_master_date, record_quest};
use crate::blob::IntoRemoteData;
use crate::user::grant::grant_items;
use crate::user::id::UserId;
use crate::user::wallet::Wallet;
use crate::database::QueryExecutor;
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;

// See [Wonder_Api_ScorechallengeinfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeInfoResponse {
  pub challengenum: i32,
  pub normalscore: i32,
  pub hardscore: i32,
  pub veryhardscore: i32,
  pub totalscore: i32,
  pub rank: i32,
  pub in_ranking_period: i32,
  pub beforeinfo: Vec<ScoreChallengeinfoBeforeInfo>,
  pub best_score_info: Vec<ScoreChallengeMissionBestScoreInfo>,
}

impl CallCustom for ScoreChallengeInfoResponse {}

// See [Wonder_Api_ScorechallengeinfoBeforeinfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeinfoBeforeInfo {
  pub scorechallengeid: i32,
  pub score: i32,
  pub rank: i32,
}

// See [Wonder_Api_ScorechallengeMissionBestScoreInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeMissionBestScoreInfo {
  pub elemental: String,
  pub scorechallenge_id: i32,
  pub best_score: i32,
  pub reward_type: i32,
}


// See [Wonder_Api_ScorechallengerankingResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeRankingResponse {
  pub ranking: Vec<ScoreChallengeRanking>,
}

impl CallCustom for ScoreChallengeRankingResponse {}

// See [Wonder_Api_ScorechallengerankingRankingResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeRanking {
  pub user_no: String,
  pub icon: i64,
  pub name: String,
  pub score: i32,
  pub rank: i32,
  pub scoremode: i32,
  pub honor_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct ScoreChallengeRankingRequest {
  /// 1 - around me, 2 - global.
  pub mode: i32,
  /// Start after: 1, 1k, 5k, 10k, 20k, 30k (29951), 0 for around me.
  /// Can be an arbitrary number if "View next" or "View previous" is pressed.
  pub ranking: i32,
}


// See [Wonder_Api_ScorechallengebestscorepartyResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeBestScorePartyResponse {
  pub user_name: String,
  pub icon: i64,
  pub rank: i32,
  pub bestscore: i32,
  /// Not displayed in-game.
  pub party_name: String,
  pub strength: i32,
  pub party: ScoreChallengeBestScoreParty,
  pub member: Vec<BestScorePartyMember>,
}

impl CallCustom for ScoreChallengeBestScorePartyResponse {}

// See [Wonder_Api_ScorechallengebestscorepartyMemberResponseDto_Fields]
// See [Wonder_Api_BasicBestScorePartyMemberResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct BestScorePartyMember {
  pub member_id: i64,
  pub lv: i32,
  pub ex_flg: i32,
}

// See [Wonder_Api_ScorechallengebestscorepartyPartyResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeBestScoreParty {
  pub party_forms: Vec<ScoreChallengeBestscorePartyFormInfoResponse>,
  pub assist: i32,
  pub sub_assists: Vec<i32>,
  pub party_passive_skill: ScoreChallengeBestscorePartyPassiveSkillInfoResponse,
}

// See [Wonder_Api_ScorechallengeBestscorePartyFormInfoResponseDto_Fields]
// See [Wonder_Api_BasicBestScorePartyFormInfoResponseDto_Fields]
/// Member IDs must be master IDs, not client-unique IDs, compared to other PartyForm structs.
#[derive(Debug, Serialize)]
pub struct ScoreChallengeBestscorePartyFormInfoResponse {
  pub form_no: i32,
  pub main: i32,
  pub sub1: i32,
  pub sub2: i32,
  pub weapon: i64,
  pub acc: i64,
  pub specialskill: i64,
  pub skill_pa_fame: i64,
}

// See [Wonder_Api_ScorechallengeBestscorePartyPassiveSkillInfoResponseDto_Fields]
// See [Wonder_Api_BasicBestScorePartyPassiveSkillInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeBestscorePartyPassiveSkillInfoResponse {
  pub skill_id: i64,
  pub member_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct ScoreChallengeBestScorePartyRequest {
  pub user_no: i32,
  pub scorechallengeid: i32,
}

pub async fn score_challenge_best_score_party(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ScoreChallengeBestScorePartyRequest>,
) -> impl IntoHandlerResponse {
  warn!(?params, "encountered stub: score_challenge_best_score_party");

  let client = state.get_database_client().await?;

  // Send only members that are used in the party, no hardlock occurs, but just to expose less data
  let fetch_members = FetchUserMembers::new(&client).await.unwrap();
  let members = fetch_members.run(session.user_id).await.unwrap();

  Ok(Unsigned(ScoreChallengeBestScorePartyResponse {
    user_name: "Megumin".to_string(),
    icon: 1083110,
    rank: 42,
    bestscore: 120_000_000,
    party_name: "Kazumin".to_string(),
    strength: 99999,
    party: ScoreChallengeBestScoreParty {
      party_forms: (1..=5)
        .map(|i| ScoreChallengeBestscorePartyFormInfoResponse {
          form_no: i as i32,
          main: members.get(i - 1).map_or(0, |m| m.id),
          sub1: 0,
          sub2: 0,
          weapon: 0,
          acc: 0,
          specialskill: 100001,
          skill_pa_fame: 0,
        })
        .collect::<Vec<_>>(),
      assist: 0,
      sub_assists: vec![],
      party_passive_skill: ScoreChallengeBestscorePartyPassiveSkillInfoResponse {
        skill_id: 0,
        member_id: 0,
      },
    },
    member: members
      .iter()
      .map(|m| BestScorePartyMember {
        member_id: m.id as i64,
        lv: m.level(),
        ex_flg: 0,
      })
      .collect::<Vec<_>>(),
  }))
}

// See [Wonder_Api_ScorechallengestartResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeStartResponse {
  pub party: BattleParty,
  pub members: Vec<BattleMember>,
}

impl CallCustom for ScoreChallengeStartResponse {}

#[derive(Debug, Deserialize)]
pub struct ScoreChallengeStartRequest {
  #[serde(with = "crate::bool_as_int")]
  pub is_practice: bool,
  pub quest_id: i32,
  #[serde(rename = "party_no")]
  pub party_id: i32,
}


// See [Wonder_Api_ScorechallengeresultResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeResultResponse {
  /// How many challenges to deduct?
  pub challengenum: i32,
  pub reward: Vec<ScoreChallengeResultReward>,
  pub totalrewardids: Vec<i32>,
  pub rank: i32,
  pub ranktype: i32,
  pub in_ranking_period: i32,
  pub totalscore: i32,
  pub previous_bestscore: i32,
  pub bestscore: i32,
  pub modescore: i32,
}

impl CallCustom for ScoreChallengeResultResponse {}

// See [Wonder_Api_ScorechallengeresultRewardResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeResultReward {
  pub itemtype: i32,
  pub itemid: i64,
  pub itemnum: i32,
  pub rewardtype: i32,
}

// body={"quest_id": "320391", "memcheckcount": "0", "score": "958528312", "party_no": "1", "seed": "958527983", "original_score": "727"}
#[derive(Debug, Deserialize)]
pub struct ScoreChallengeResultRequest {
  pub quest_id: i32,
  pub memcheckcount: i32,
  // See [_vst_an$$fhmcQTT]
  /// `decrypted_score = score ^ seed`.
  pub score: i64,
  pub party_no: i32,
  pub seed: i64,
  pub original_score: i32,
}


// See [Wonder_Api_ScorechallengeMissionResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeMissionResponse {
  pub best_score_info: Vec<ScoreChallengeMissionBestScoreInfo>,
  pub display_best_score_info: Vec<ScoreChallengeMissionDisplayBestScoreInfo>,
}

impl CallCustom for ScoreChallengeMissionResponse {}

// See [Wonder_Api_ScorechallengeMissionDisplayBestScoreInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeMissionDisplayBestScoreInfo {
  pub elemental: String,
  pub best_score: i32,
  pub previous_best_score: i32,
}

pub async fn score_challenge_mission() -> impl IntoHandlerResponse {
  warn!("encountered stub: score_challenge_mission");

  Ok(Unsigned(ScoreChallengeMissionResponse {
    best_score_info: vec![],
    display_best_score_info: vec![],
  }))
}

// See [Wonder_Api_ScorechallengeMissionListResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeMissionListResponse {
  pub scorechallenge_mission_list: Vec<ScoreChallengeMissionListInfo>,
}

impl CallCustom for ScoreChallengeMissionListResponse {}

// See [Wonder_Api_ScorechallengeMissionListInfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ScoreChallengeMissionListInfo {
  pub mission_id: i32,
  pub achieve_flag: i32,
}

pub async fn score_challenge_mission_list() -> impl IntoHandlerResponse {
  warn!("encountered stub: score_challenge_mission_list");

  Ok(Unsigned(ScoreChallengeMissionListResponse {
    scorechallenge_mission_list: vec![],
  }))
}

// See [Wonder_Api_EventScorechallengeLogRequest_Fields]
#[derive(Debug, Deserialize)]
pub struct ScoreChallengeLogRequest {
  // The client does not always send these: it is only a battle log, so don't fail without them
  #[serde(default)]
  pub event_id: Option<i32>,
  #[serde(default)]
  pub stage_id: Option<i32>,
  #[serde(rename = "party_no", default)]
  pub party_id: Option<i32>,
  #[serde(default)]
  pub log: String,
}

pub async fn score_challenge_log(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ScoreChallengeLogRequest>,
) -> anyhow::Result<impl IntoHandlerResponse> {
  warn!(?params, "encountered stub: scorechallenge_log");

  // See [Wonder_Api_EventScorechallengeLogResponseDto_Fields]
  Ok(Unsigned(()))
}

// ---------------------------------------------------------------------------------------------
// Battle Arena (score challenge): 3 attempts a day, a best score per difficulty, and the total of
// the three bests unlocks the arena's score rewards (Arena Medals and more). The boss is not meant
// to die: the score is the damage dealt before time runs out. With a single player, the ranking
// only has the player.

const DAILY_ATTEMPTS: i32 = 3;

fn int(row: &Value, key: &str) -> i32 {
  row[key].as_str().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// The arena the game shows: the last one that has started (they all ended when the game closed).
fn current_score_challenge(now: DateTime<Utc>) -> i32 {
  get_master_manager()
    .get_master("scorechallenge")
    .iter()
    .filter_map(|row| Some((parse_master_date(row["start_at"].as_str()?)?, int(row, "scorechallenge_id"))))
    .filter(|(start, _)| *start <= now)
    .max()
    .map(|(_, id)| id)
    .unwrap_or(0)
}

/// Stages of an arena as (stage id, mode: 1 normal, 2 hard, 3 very hard).
fn arena_stages(score_challenge_id: i32) -> Vec<(i32, i32)> {
  get_master_manager()
    .get_master("scorechallenge_stage")
    .iter()
    .filter(|row| int(row, "scorechallenge_id") == score_challenge_id)
    .map(|row| (int(row, "id"), int(row, "mode")))
    .collect()
}

fn arena_of_stage(quest_id: i32) -> Option<(i32, i32, i64)> {
  get_master_manager()
    .get_master("scorechallenge_stage")
    .iter()
    .find(|row| int(row, "id") == quest_id)
    .map(|row| {
      (
        int(row, "scorechallenge_id"),
        int(row, "mode"),
        row["pack_id"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0),
      )
    })
}

struct ArenaState {
  /// Best score by mode (index 0 = normal)
  best: [i64; 3],
  used_today: i32,
}

impl ArenaState {
  fn total(&self) -> i64 {
    self.best.iter().sum()
  }

  fn attempts_left(&self) -> i32 {
    (DAILY_ATTEMPTS - self.used_today).max(0)
  }
}

async fn load_arena<'a>(executor: impl Into<QueryExecutor<'a>>, user_id: UserId, score_challenge_id: i32) -> anyhow::Result<ArenaState> {
  let executor = executor.into();
  let stages = arena_stages(score_challenge_id);
  let ids = stages.iter().map(|(id, _)| *id).collect::<Vec<_>>();
  #[rustfmt::skip]
  let rows = executor.client().query(/* language=postgresql */ r#"
    select quest_id, best_score from user_score_challenge_best where user_id = $1 and quest_id = any($2)
  "#, &[&user_id, &ids]).await.context("failed to read arena scores")?;
  let scores = rows.iter().map(|row| (row.get::<_, i32>(0), row.get::<_, i64>(1))).collect::<HashMap<_, _>>();
  let mut best = [0i64; 3];
  for (id, mode) in &stages {
    if (1..=3).contains(mode) {
      best[(*mode - 1) as usize] = best[(*mode - 1) as usize].max(scores.get(id).copied().unwrap_or(0));
    }
  }
  let progress = load_progress(executor.client(), user_id).await?;
  let used_today = ids.iter().filter_map(|id| progress.get(id)).map(|entry| entry.daily_count).sum();
  Ok(ArenaState { best, used_today })
}

fn clamp_score(score: i64) -> i32 {
  score.clamp(0, i32::MAX as i64) as i32
}

pub async fn score_challenge_info(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let arena = load_arena(&client, session.user_id, current_score_challenge(Utc::now())).await?;
  Ok(Unsigned(ScoreChallengeInfoResponse {
    challengenum: arena.attempts_left(),
    normalscore: clamp_score(arena.best[0]),
    hardscore: clamp_score(arena.best[1]),
    veryhardscore: clamp_score(arena.best[2]),
    totalscore: clamp_score(arena.total()),
    rank: if arena.total() > 0 { 1 } else { 0 },
    in_ranking_period: 1,
    beforeinfo: vec![],
    best_score_info: vec![],
  }))
}

pub async fn score_challenge_ranking(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ScoreChallengeRankingRequest>,
) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let arena = load_arena(&client, session.user_id, current_score_challenge(Utc::now())).await?;
  #[rustfmt::skip]
  let user = client.query_one(/* language=postgresql */ r#"
    select coalesce(username, ''), favorite_member, honor from users where id = $1
  "#, &[&session.user_id]).await.context("failed to read user")?;
  let ranking = if arena.total() > 0 {
    vec![ScoreChallengeRanking {
      user_no: session.user_id.to_string(),
      icon: user.get(1),
      name: user.get(0),
      score: clamp_score(arena.total()),
      rank: 1,
      scoremode: params.mode,
      honor_id: user.get(2),
    }]
  } else {
    vec![]
  };
  Ok(Unsigned(ScoreChallengeRankingResponse { ranking }))
}

pub async fn score_challenge_start(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ScoreChallengeStartRequest>,
) -> impl IntoHandlerResponse {
  let client = state.pool.get().await.context("failed to get database connection")?;

  // A real attempt (not a practice) uses one of the day's attempts. With none left, the battle
  // still runs but counts as a practice (no score, no rewards), so the client does not get stuck.
  let mut is_practice = params.is_practice;
  if !is_practice {
    let arena = load_arena(&client, session.user_id, current_score_challenge(Utc::now())).await?;
    if arena.attempts_left() > 0 {
      record_quest(&client, session.user_id, params.quest_id, 1, [false; 3]).await?;
    } else {
      warn!(quest_id = params.quest_id, "no arena attempts left today, counting the battle as practice");
      is_practice = true;
    }
  }
  #[rustfmt::skip]
  client.execute(/* language=postgresql */ r#"
    insert into user_score_challenge_runs (user_id, quest_id, is_practice) values ($1, $2, $3)
    on conflict (user_id) do update set quest_id = excluded.quest_id, is_practice = excluded.is_practice, started_at = now()
  "#, &[&session.user_id, &params.quest_id, &is_practice]).await.context("failed to save arena run")?;
  info!(quest_id = params.quest_id, is_practice, "arena battle started");

  let party = FetchUserParty::new(&client)
    .await?
    .run(session.user_id, params.party_id as i64)
    .await?;

  // We must send only members that are used in the party, otherwise hardlock occurs
  let fetch_members = FetchUserMembersIn::new(&client).await?;
  #[rustfmt::skip]
  let mut members = fetch_members.run(
    session.user_id,
    &party.party_forms.iter().map(|form| form.main as i64).collect::<Vec<_>>(),
  ).await?;
  FetchUserMemberSkillsIn::new(&client)
    .await?
    .run(session.user_id, &mut members.iter_mut().collect::<Vec<_>>())
    .await?;

  Ok(Unsigned(ScoreChallengeStartResponse {
    party: party.to_battle_party(),
    members: members
      .into_iter()
      .filter_map(|member| {
        let form = party.party_forms.iter().find(|form| form.main == member.id)?;
        Some(member.to_battle_member(form))
      })
      .collect(),
  }))
}

pub async fn score_challenge_result(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ScoreChallengeResultRequest>,
) -> anyhow::Result<impl IntoHandlerResponse> {
  let score = (params.score ^ params.seed) as i32;
  if score != params.original_score {
    warn!(score, original_score = params.original_score, "arena score check failed, using the decrypted score");
  }
  let score = score.max(0) as i64;
  let (score_challenge_id, mode, stage_pack) = arena_of_stage(params.quest_id).context("unknown arena stage")?;

  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;
  #[rustfmt::skip]
  let practice = transaction.query_opt(/* language=postgresql */ r#"
    delete from user_score_challenge_runs where user_id = $1 returning is_practice
  "#, &[&session.user_id]).await.context("failed to read arena run")?
    .map(|row| row.get::<_, bool>(0))
    .unwrap_or(false);

  let before = load_arena(&transaction, session.user_id, score_challenge_id).await?;
  let previous_best = before.best.get((mode - 1).clamp(0, 2) as usize).copied().unwrap_or(0);
  let mut reward = Vec::new();
  let mut total_reward_ids = Vec::new();
  let mut remote = Vec::new();
  let mut wallet = Wallet::load(&transaction, session.user_id).await?;
  let after = if practice {
    before
  } else {
    #[rustfmt::skip]
    transaction.execute(/* language=postgresql */ r#"
      insert into user_score_challenge_best (user_id, quest_id, best_score) values ($1, $2, $3)
      on conflict (user_id, quest_id) do update
        set best_score = greatest(user_score_challenge_best.best_score, excluded.best_score), updated_at = now()
    "#, &[&session.user_id, &params.quest_id, &score]).await.context("failed to save arena score")?;
    let after = load_arena(&transaction, session.user_id, score_challenge_id).await?;

    // Participation reward (Arena Medals), then the score rewards reached for the first time
    let mut items = Vec::new();
    for item in get_pack(&stage_pack.to_string()).map(pack_contents).unwrap_or_default() {
      reward.push(ScoreChallengeResultReward { itemtype: item.0, itemid: item.1, itemnum: item.2, rewardtype: 1 });
      items.push(item);
    }
    for row in get_master_manager().get_master("scorechallenge_reward_achievement") {
      if int(row, "scorechallenge_id") != score_challenge_id {
        continue;
      }
      let needed = int(row, "score") as i64;
      if before.total() < needed && needed <= after.total() {
        total_reward_ids.push(int(row, "id"));
        for item in row["pack_id"].as_str().and_then(get_pack).map(pack_contents).unwrap_or_default() {
          reward.push(ScoreChallengeResultReward { itemtype: item.0, itemid: item.1, itemnum: item.2, rewardtype: 2 });
          items.push(item);
        }
      }
    }
    remote.extend(grant_items(&transaction, session.user_id, &mut wallet, &items).await?);
    after
  };
  wallet.save(&transaction, session.user_id).await?;
  transaction.commit().await.context("failed to commit transaction")?;
  info!(quest_id = params.quest_id, score, practice, total = after.total(), "arena battle finished");

  let mut response = CallResponse::new_success(Box::new(ScoreChallengeResultResponse {
    challengenum: after.attempts_left(),
    reward,
    totalrewardids: total_reward_ids,
    rank: if after.total() > 0 { 1 } else { 0 },
    ranktype: 1,
    in_ranking_period: 1,
    totalscore: clamp_score(after.total()),
    previous_bestscore: clamp_score(previous_best),
    bestscore: clamp_score(previous_best.max(if practice { 0 } else { score })),
    modescore: clamp_score(score),
  }));
  response.remote.extend(remote);
  response.remote.extend(wallet.to_remote_data());
  Ok(Unsigned(response))
}
