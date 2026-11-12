//! Giving members and characters to a user outside the gacha: starting members, the tutorial gacha,
//! crown rewards and reward packs.

use crate::api::master_all::get_master_manager;
use crate::api::present::{grant_kind, Grant};
use crate::api::{CharacterParameter, RemoteData, RemoteDataItemType, SpSkill};
use crate::item::UpdateItemCountBy;
use crate::user::wallet::Wallet;
use crate::blob::{AddCharacter, AddMember, IntoRemoteData};
use crate::level::get_intimacy_level_calculator;
use crate::member::MemberPrototype;
use crate::user::id::UserId;
use anyhow::Context;
use tracing::{info, warn};

/// Members every new account starts with: Kazuma, Aqua, Megumin and Darkness.
pub const STARTING_MEMBERS: [i64; 4] = [1001100, 1011100, 1021100, 1031100];

/// What the tutorial gacha gives (src/gacha-tutorial-reward.json).
pub const TUTORIAL_GACHA_MEMBERS: [i64; 10] = [
  1063113, 1034100, 1152102, 1083110, 1122100, 1093100, 1132100, 1002102, 1282100, 1064217,
];

/// Special skills of a character, as (group id, skill id).
fn character_special_skills(character_id: i64) -> Vec<(i32, i64)> {
  get_master_manager()
    .get_master("skill_sp")
    .iter()
    .filter(|skill| skill["character_id"].as_str().and_then(|id| id.parse::<i64>().ok()) == Some(character_id))
    .filter_map(|skill| {
      Some((
        skill["skill_group_id"].as_str()?.parse().ok()?,
        skill["skill_id"].as_str()?.parse().ok()?,
      ))
    })
    .collect()
}

/// Adds the characters the user does not have yet, with their special skills at level 1.
/// Returns the remote data for the new characters.
pub async fn ensure_characters(
  transaction: &deadpool_postgres::Transaction<'_>,
  user_id: UserId,
  character_ids: &[i64],
) -> anyhow::Result<Vec<RemoteData>> {
  let mut remote = Vec::new();
  for &character_id in character_ids {
    #[rustfmt::skip]
    let inserted = transaction.execute(/* language=postgresql */ r#"
      insert into user_characters (user_id, character_id, intimacy) values ($1, $2, 0)
      on conflict do nothing
    "#, &[&user_id, &character_id]).await.context("failed to add character")?;
    if inserted == 0 {
      continue;
    }

    let skills = character_special_skills(character_id);
    for (_, skill_id) in &skills {
      #[rustfmt::skip]
      transaction.execute(/* language=postgresql */ r#"
        insert into user_character_special_skills (user_id, character_id, skill_id, level) values ($1, $2, $3, 1)
        on conflict do nothing
      "#, &[&user_id, &character_id, skill_id]).await.context("failed to add special skill")?;
    }
    info!(%user_id, character_id, "granted character");

    remote.extend(
      AddCharacter::new(
        character_id as i32,
        CharacterParameter {
          id: character_id,
          character_id,
          rank: get_intimacy_level_calculator().get_level(0),
          rank_progress: 0,
          sp_skill: skills
            .iter()
            .map(|&(group_id, id)| SpSkill {
              group_id,
              id,
              lv: 1,
              is_trial: false,
            })
            .collect(),
          character_enhance_stage_id_list: vec![0, 0, 0, 0],
          character_piece_board_stage_id_list: vec![100001001, 100002002, 100003003, 100004004],
          is_trial: false,
        },
      )
      .into_remote_data(),
    );
  }
  Ok(remote)
}

/// Gives a member: a new member if the user does not have it, otherwise a reserve copy (used for
/// limit breaks). Also adds its character and skills. Returns the remote data for the client.
pub async fn grant_member(
  transaction: &deadpool_postgres::Transaction<'_>,
  user_id: UserId,
  member_id: i64,
) -> anyhow::Result<Vec<RemoteData>> {
  let prototype = MemberPrototype::load_from_id(member_id);
  let mut remote = ensure_characters(transaction, user_id, &[prototype.character_id]).await?;

  #[rustfmt::skip]
  let inserted = transaction.execute(/* language=postgresql */ r#"
    insert into user_members (user_id, member_id, xp, promotion_level) values ($1, $2, 0, 0)
    on conflict do nothing
  "#, &[&user_id, &member_id]).await.context("failed to add member")?;

  let mut member = prototype.create_member(member_id as i32);
  if inserted == 0 {
    #[rustfmt::skip]
    transaction.execute(/* language=postgresql */ r#"
      insert into user_members_reserve (user_id, member_id) values ($1, $2)
    "#, &[&user_id, &member_id]).await.context("failed to add reserve member")?;
    info!(%user_id, member_id, "granted reserve member");
    remote.extend(AddMember::reserve(member.to_member_parameter_wire()).into_remote_data());
  } else {
    for skill in prototype.active_skills.iter().flatten() {
      #[rustfmt::skip]
      transaction.execute(/* language=postgresql */ r#"
        insert into user_member_skills (user_id, member_id, skill_id, level) values ($1, $2, $3, 1)
        on conflict do nothing
      "#, &[&user_id, &member_id, &skill.id]).await.context("failed to add member skill")?;
    }
    crate::member::FetchUserMemberSkillsIn::new(transaction)
      .await?
      .run(user_id, &mut [&mut member])
      .await?;
    info!(%user_id, member_id, "granted member");
    remote.extend(AddMember::normal(member.to_member_parameter_wire()).into_remote_data());
  }
  Ok(remote)
}

/// Gives items of any kind, as (item type, item id, amount): members, currencies (into [wallet],
/// saved by the caller) or items. Kinds not supported yet are skipped with a warning.
pub async fn grant_items(
  transaction: &deadpool_postgres::Transaction<'_>,
  user_id: UserId,
  wallet: &mut Wallet,
  items: &[(i32, i64, i32)],
) -> anyhow::Result<Vec<RemoteData>> {
  let update = UpdateItemCountBy::new(transaction).await?;
  let mut remote = Vec::new();
  for &(item_type, item_id, item_num) in items {
    let kind = RemoteDataItemType::from(item_type);
    match grant_kind(kind) {
      // Sent with the rest of the wallet
      Grant::Singleton => {
        wallet.add(kind, item_num as i64)?;
      }
      Grant::Member => {
        for _ in 0..item_num.max(1) {
          remote.extend(grant_member(transaction, user_id, item_id).await?);
        }
      }
      Grant::Item => {
        let counted = update.run(user_id, (kind, item_id), item_num).await?;
        remote.extend(counted.into_remote_data());
      }
      Grant::Unsupported => {
        warn!(?item_type, ?item_id, ?item_num, "reward type not supported yet, not given");
      }
    }
  }
  Ok(remote)
}
