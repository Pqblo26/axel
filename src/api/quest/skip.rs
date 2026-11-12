//! Skipping quests with skip tickets (main, free and event quests share the same rules).

use crate::AppState;
use crate::api::dungeon::BattleSkipReward;
use crate::api::quest::quest_hunting::SkipInfoRequestDto;
use crate::api::quest::quest_main::BattleSkipResponse;
use crate::api::quest::stage::stage_info;
use crate::api::{RemoteData, RemoteDataItemType};
use crate::blob::IntoRemoteData;
use crate::call::CallResponse;
use crate::item::UpdateItemCountBy;
use crate::user::session::Session;
use crate::user::wallet::Wallet;
use anyhow::Context;
use std::collections::BTreeMap;
use tracing::{info, warn};

/// Skip tickets are a single item: type 8, id 1.
const SKIP_TICKET_ID: i64 = 1;

/// Each skipped run costs the stage's skip tickets and stamina, and gives the same eris and items
/// as winning the stage. If anything is missing nothing is spent or given.
pub async fn skip_quests(
  state: &AppState,
  session: &Session,
  skips: &[SkipInfoRequestDto],
) -> anyhow::Result<CallResponse<BattleSkipResponse>> {
  let empty = || {
    CallResponse::new_success(Box::new(BattleSkipResponse {
      lvup: 0,
      reward: vec![],
    }))
  };

  let mut tickets = 0i64;
  let mut stamina = 0i64;
  let mut money = 0i64;
  let mut runs = Vec::new();
  for skip in skips {
    let count = skip.skip_count.max(0);
    let Some(stage) = stage_info(skip.quest_id) else {
      warn!(quest_id = skip.quest_id, "unknown quest stage, cannot skip");
      return Ok(empty());
    };
    tickets += stage.skip_tickets as i64 * count as i64;
    stamina += stage.stamina as i64 * count as i64;
    money += stage.money as i64 * count as i64;
    for _ in 0..count {
      runs.push(stage);
    }
  }
  if runs.is_empty() {
    return Ok(empty());
  }

  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;

  #[rustfmt::skip]
  let owned_tickets: i32 = transaction.query_opt(/* language=postgresql */ r#"
    select quantity from user_items where user_id = $1 and item_type = $2 and item_id = $3
  "#, &[&session.user_id, &(i32::from(RemoteDataItemType::SkipTicket) as i64), &SKIP_TICKET_ID]).await?
    .map(|row| row.get(0))
    .unwrap_or(0);
  if (owned_tickets as i64) < tickets {
    warn!(?owned_tickets, needed = tickets, "not enough skip tickets");
    return Ok(empty());
  }

  let mut wallet = Wallet::load(&transaction, session.user_id).await?;
  if let Err(error) = wallet.add(RemoteDataItemType::Stamina, -stamina) {
    warn!(%error, "not enough stamina to skip");
    return Ok(empty());
  }
  wallet.add(RemoteDataItemType::Money, money)?;
  wallet.save(&transaction, session.user_id).await?;

  let update_items = UpdateItemCountBy::new(&transaction).await?;
  // Last known amount of each item, sent once at the end
  let mut items = BTreeMap::new();
  if tickets > 0 {
    let item = update_items
      .run(
        session.user_id,
        (RemoteDataItemType::SkipTicket, SKIP_TICKET_ID),
        -tickets as i32,
      )
      .await?;
    items.insert(item.item, item);
  }

  let mut reward = Vec::new();
  for (index, stage) in runs.iter().enumerate() {
    // Eris and player exp go on the first line of each run only, so they are not counted twice
    let mut first = true;
    for item in &stage.rewards {
      let counted = update_items
        .run(
          session.user_id,
          (RemoteDataItemType::from(item.item_type), item.item_id),
          item.item_num,
        )
        .await?;
      items.insert(counted.item, counted);
      reward.push(BattleSkipReward {
        dropnum: index as i32 + 1,
        exp: if first { stage.player_exp } else { 0 },
        money: if first { stage.money } else { 0 },
        itemtype: item.item_type,
        itemid: item.item_id as i32,
        itemnum: item.item_num,
      });
      first = false;
    }
  }
  transaction.commit().await.context("failed to commit transaction")?;
  info!(runs = runs.len(), ?tickets, ?stamina, ?money, "quests skipped");

  let mut response = CallResponse::new_success(Box::new(BattleSkipResponse { lvup: 0, reward }));
  response.remote.extend(
    items
      .into_values()
      .flat_map(|item| item.into_remote_data())
      .collect::<Vec<RemoteData>>(),
  );
  response.remote.extend(wallet.to_remote_data());
  Ok(response)
}
