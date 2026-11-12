//! What happens when a quest battle ends (main, free and event quests): rewards, eris, player
//! rank, member experience, affinity, crowns and their rewards, and daily attempts.

use crate::api::battle::{apply_reward_multiplier, make_battle_member_exp_and_character_love};
use crate::api::battle_multi::{BattleCharacterLove, BattleClearReward, BattleMemberExp};
use crate::api::party_info::Party;
use crate::api::quest::progress::{area_daily_count, load_progress, main_stage_daily_limit, record_quest, tasks_from_missions};
use crate::api::quest::quest_hunting::{hunting_area_limit, BattleReward};
use crate::api::quest::stage::{stage_info, StageInfo, StageKind};
use crate::api::{RemoteData, RemoteDataItemType};
use crate::blob::{IntoRemoteData, UpdateMember};
use crate::level::get_member_level_calculator;
use crate::member::{materialize_member_row_impl, FetchUserMemberSkillsIn, FetchUserParty, MemberPrototype};
use crate::user::grant::grant_items;
use crate::user::id::UserId;
use crate::user::progress::PlayerProgress;
use crate::user::session::Session;
use crate::user::wallet::Wallet;
use crate::AppState;
use anyhow::Context;
use crate::api::exchange::{get_pack, pack_contents};
use tracing::{info, warn};

/// Items of a reward pack ("pack" master): the main item and up to 5 bonus items.
pub fn pack_items(pack_id: i64) -> Vec<(i32, i64, i32)> {
  get_pack(&pack_id.to_string()).map(pack_contents).unwrap_or_default()
}

/// Main members of the party, each once.
fn party_main_members(party: &Party) -> Vec<i64> {
  let mut members = Vec::new();
  for form in &party.party_forms {
    let member_id = form.main as i64;
    if member_id != 0 && !members.contains(&member_id) {
      members.push(member_id);
    }
  }
  members
}

/// Gives experience to the main members of the party (saved), up to their maximum level.
pub async fn save_member_exp(
  transaction: &deadpool_postgres::Transaction<'_>,
  user_id: UserId,
  party: &Party,
  gain: i32,
) -> anyhow::Result<(Vec<BattleMemberExp>, Vec<RemoteData>)> {
  let calculator = get_member_level_calculator();
  let mut member_exp = Vec::new();
  let mut remote = Vec::new();
  for member_id in party_main_members(party) {
    #[rustfmt::skip]
    let Some(row) = transaction.query_opt(/* language=postgresql */ r#"
      select xp, promotion_level from user_members where user_id = $1 and member_id = $2 for update
    "#, &[&user_id, &member_id]).await.context("failed to read member")? else {
      continue;
    };
    let xp: i32 = row.get(0);
    let promotion_level: i32 = row.get(1);
    let prototype = MemberPrototype::load_from_id(member_id);
    let max_level = calculator.get_max_level(prototype.rarity, promotion_level);
    let max_xp = calculator.get_xp_for_level(max_level, prototype.rarity);
    let new_xp = (xp + gain.max(0)).min(max_xp).max(xp);
    if new_xp != xp {
      #[rustfmt::skip]
      transaction.execute(/* language=postgresql */ r#"
        update user_members set xp = $3 where user_id = $1 and member_id = $2
      "#, &[&user_id, &member_id, &new_xp]).await.context("failed to save member exp")?;
    }
    member_exp.push(BattleMemberExp {
      member_id,
      exp: new_xp - xp,
    });

    let mut member = materialize_member_row_impl(member_id, new_xp, promotion_level, prototype);
    FetchUserMemberSkillsIn::new(transaction)
      .await?
      .run(user_id, &mut [&mut member])
      .await?;
    remote.extend(UpdateMember::new(member.to_member_parameter_wire()).into_remote_data());
  }
  Ok((member_exp, remote))
}

/// Daily attempts used on a stage (or its free quest area) after this battle, 0 if it has no limit.
/// The game shows the attempts left as the daily limit minus this.
fn attempts_used(stage: &StageInfo, progress: &crate::api::quest::progress::ProgressMap) -> i32 {
  if stage.kind == StageKind::Hunting {
    let limit = hunting_area_limit(stage.area_id);
    if limit > 0 {
      return area_daily_count(progress, StageKind::Hunting, stage.area_id).min(limit);
    }
  }
  let limit = main_stage_daily_limit(stage);
  if limit > 0 {
    return progress.get(&stage.id).map(|entry| entry.daily_count).unwrap_or(0).min(limit);
  }
  0
}

#[derive(Debug, Default)]
pub struct QuestBattleOutcome {
  pub exp: i32,
  pub lvup: i32,
  pub money: i32,
  pub love: Vec<BattleCharacterLove>,
  pub member_exp: Vec<BattleMemberExp>,
  pub reward: Vec<BattleReward>,
  pub clearreward: Vec<BattleClearReward>,
  pub firstclear: bool,
  pub limit: i32,
  pub remote: Vec<RemoteData>,
}

pub async fn finish_quest_battle(
  state: &AppState,
  session: &Session,
  quest_id: i32,
  party_id: i32,
  win: bool,
  clearquestmission: &[i32],
) -> anyhow::Result<QuestBattleOutcome> {
  let stage = stage_info(quest_id).context("unknown quest stage")?;
  let user_id = session.user_id;
  let mut client = state.get_database_client().await?;
  let party = FetchUserParty::new(&client).await?.run(user_id, party_id as i64).await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;
  let mut outcome = QuestBattleOutcome::default();

  if !win {
    record_quest(&transaction, user_id, quest_id, 0, [false; 3]).await?;
    transaction.commit().await.context("failed to commit transaction")?;
    let (_, love) = make_battle_member_exp_and_character_love(&party, &client, session, 0).await?;
    outcome.love = love;
    outcome.member_exp = party_main_members(&party)
      .into_iter()
      .map(|member_id| BattleMemberExp { member_id, exp: 0 })
      .collect();
    info!(?quest_id, "quest lost");
    return Ok(outcome);
  }

  let mut wallet = Wallet::load(&transaction, user_id).await?;
  let mut player = PlayerProgress::load(&transaction, user_id).await?;

  // Stage drops and eris
  let mut rewards = stage.rewards.clone();
  apply_reward_multiplier(&transaction, session, &mut rewards).await?;
  let items = rewards
    .iter()
    .map(|item| (item.item_type, item.item_id, item.item_num))
    .collect::<Vec<_>>();
  outcome.remote.extend(grant_items(&transaction, user_id, &mut wallet, &items).await?);
  outcome.reward = rewards
    .iter()
    .map(|item| BattleReward {
      itemtype: item.item_type,
      itemid: item.item_id,
      itemnum: item.item_num,
      is_rare: item.item_rare,
    })
    .collect();
  wallet.add(RemoteDataItemType::Money, stage.money as i64)?;
  outcome.money = stage.money;

  // Progress and crowns: each crown's reward pack is given the first time it is achieved
  let record = record_quest(&transaction, user_id, quest_id, 1, tasks_from_missions(clearquestmission)).await?;
  outcome.firstclear = record.first_clear;
  for index in 0..3 {
    if !record.new_tasks[index] || stage.crown_packs[index] == 0 {
      continue;
    }
    let items = pack_items(stage.crown_packs[index]);
    if items.is_empty() {
      warn!(pack_id = stage.crown_packs[index], "crown reward pack not found");
    }
    outcome.remote.extend(grant_items(&transaction, user_id, &mut wallet, &items).await?);
    outcome
      .clearreward
      .extend(items.iter().map(|&(itemtype, itemid, itemnum)| BattleClearReward {
        itemtype,
        itemid,
        itemnum,
        mission: stage.missions[index],
      }));
  }

  // Player rank: a rank up refills stamina
  outcome.exp = stage.player_exp;
  outcome.lvup = player.add_exp(stage.player_exp);
  if outcome.lvup > 0 {
    info!(rank = player.rank, "player rank up");
    wallet.rank_up(player.rank);
  }

  let (member_exp, member_remote) = save_member_exp(&transaction, user_id, &party, stage.member_exp).await?;
  outcome.member_exp = member_exp;
  outcome.remote.extend(member_remote);

  let progress = load_progress(&transaction, user_id).await?;
  outcome.limit = attempts_used(stage, &progress);

  wallet.save(&transaction, user_id).await?;
  player.save(&transaction, user_id).await?;
  transaction.commit().await.context("failed to commit transaction")?;

  let (_, love) = make_battle_member_exp_and_character_love(&party, &client, session, stage.intimacy_exp).await?;
  outcome.love = love;
  outcome.remote.extend(wallet.to_remote_data());
  outcome.remote.extend(player.to_remote_data());
  info!(
    ?quest_id,
    firstclear = outcome.firstclear,
    crowns = ?record.progress.tasks,
    rank = player.rank,
    exp = player.exp,
    "quest cleared"
  );
  Ok(outcome)
}
