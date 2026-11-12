use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Context;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::AppState;
use crate::api::RemoteDataItemType;
use crate::api::interaction::Character;
use crate::api::master_all::get_master_manager;
use crate::blob::IntoRemoteData;
use crate::user::wallet::Wallet;
use crate::call::{CallCustom, CallResponse};
use crate::database::QueryExecutor;
use crate::extractor::Params;
use crate::handler::{IntoHandlerResponse, Signed, Unsigned};
use crate::item::UpdateItemCountBy;
use crate::user::id::UserId;
use crate::user::session::Session;

/// One reward "count" every 864 seconds, see [Wonder.UI.Data.ExpeditionData$$IsReward].
const SECONDS_PER_COUNT: i64 = 864;
/// `expedition_count` master stops giving rewards at count 51, so 50 counts (12 hours) is the maximum.
const MAX_COUNTS: i64 = 50;

/// Format used by the client for `starttime`.
fn format_time(time: DateTime<Utc>) -> String {
  time.format("%Y/%m/%d %H:%M").to_string()
}

fn counts_since(started_at: DateTime<Utc>) -> i64 {
  ((Utc::now() - started_at).num_seconds() / SECONDS_PER_COUNT).clamp(0, MAX_COUNTS)
}

/// Whether any job has reached the maximum, used for the Jobs button badge.
pub async fn any_expedition_complete<'a>(
  executor: impl Into<QueryExecutor<'a>>,
  user_id: UserId,
) -> anyhow::Result<bool> {
  let executor = executor.into();
  #[rustfmt::skip]
  let row = executor.client().query_one(/* language=postgresql */ r#"
    select exists(
      select 1 from user_expeditions
      where user_id = $1 and started_at <= now() - make_interval(secs => $2)
    )
  "#, &[&user_id, &((SECONDS_PER_COUNT * MAX_COUNTS) as f64)]).await?;
  Ok(row.get(0))
}

// See [Wonder_Api_ExpeditiontopResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ExpeditionTop {
  #[serde(rename = "expeditioninfo")]
  pub expeditions: Vec<ExpeditionInfo>,
  #[serde(rename = "bonuspack")]
  pub bonus_pack: i32,
}

impl CallCustom for ExpeditionTop {}

// See [Wonder_Api_ExpeditiontopExpeditioninfoResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ExpeditionInfo {
  pub expedition_id: i32,
  pub character_id: i64,
  /// - 0 - invalid;
  /// - 1 - "Working", claim available if >= 864 seconds passed since [start_time];
  /// - 2 - "Now Hiring", character can be assigned;
  /// - 3 - "Job Complete", claim available (max).
  pub status: i32,
  // See [Wonder.UI.Data.ExpeditionData$$GetElapsedTime].
  // See [Wonder.UI.Data.ExpeditionData$$IsReward]:
  // > `return System_TimeSpan__get_TotalSeconds(v11, 0LL) >= 864.0;`
  /// Start time, relative to server time.
  #[serde(rename = "starttime")]
  pub start_time: String,
}

// Reference: https://youtu.be/yhJJ8oCST-4
pub async fn expedition_top(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  #[rustfmt::skip]
  let rows = client.query(/* language=postgresql */ r#"
    select expedition_id, character_id, started_at
    from user_expeditions
    where user_id = $1
  "#, &[&session.user_id]).await.context("failed to fetch expeditions")?;
  let working = rows
    .iter()
    .map(|row| {
      let expedition_id: i32 = row.get("expedition_id");
      let character_id: i64 = row.get("character_id");
      let started_at: DateTime<Utc> = row.get("started_at");
      (expedition_id, (character_id, started_at))
    })
    .collect::<BTreeMap<_, _>>();

  let expeditions = get_master_manager()
    .get_master("expedition")
    .iter()
    .filter(|expedition| expedition["enable"].as_str() == Some("1"))
    .filter_map(|expedition| expedition["expedition_id"].as_str()?.parse::<i32>().ok())
    .map(|expedition_id| match working.get(&expedition_id) {
      Some(&(character_id, started_at)) => ExpeditionInfo {
        expedition_id,
        character_id,
        status: if counts_since(started_at) >= MAX_COUNTS { 3 } else { 1 },
        start_time: format_time(started_at),
      },
      None => ExpeditionInfo {
        expedition_id,
        character_id: 0,
        status: 2,
        start_time: "".to_string(),
      },
    })
    .collect();

  Ok(Signed(
    ExpeditionTop {
      expeditions,
      // 1 shows "2x Job Rewards currently applied"; rewards are not doubled, so keep it off
      bonus_pack: 0,
    },
    session,
  ))
}

// See [Wonder_Api_ExpeditioncharacterResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ExpeditionCharacter {
  pub characters: Vec<ExpeditionCharacterInfo>,
}

impl CallCustom for ExpeditionCharacter {}

// See [Wonder_Api_ExpeditioncharacterCharactersResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ExpeditionCharacterInfo {
  /// Opaque value to uniquely identify selected character.
  /// Used in [ExpeditionSetRequest] and [ExpeditionUpdate].
  pub id: i32,
  pub character_id: i64,
  pub rank: i32,
  pub rank_progress: i32,
}

pub async fn expedition_character(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  #[rustfmt::skip]
  let statement = client
    .prepare(/* language=postgresql */ r#"
      select
        character_id,
        intimacy
      from user_characters
      where user_id = $1
    "#)
    .await
    .context("failed to prepare statement")?;
  let rows = client
    .query(&statement, &[&session.user_id])
    .await
    .context("failed to execute query")?;

  Ok(Unsigned(ExpeditionCharacter {
    characters: rows
      .iter()
      .map(|row| {
        Character::new(
          row.get(0),
          row.get(1),
          "".to_string(),
          0,
          0,
          [0, 0, 0, 0],
          "".to_string(),
        )
      })
      .map(|character| ExpeditionCharacterInfo {
        id: character.character_id as i32,
        character_id: character.character_id,
        rank: character.rank(),
        rank_progress: character.rank_progress,
      })
      .collect(),
  }))
}

// See [Wonder_Api_ExpeditionsetResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ExpeditionSet {
  /// Does not seem to be used anywhere
  pub updates: Vec<ExpeditionUpdate>,
  /// Displayed only if [items] is not empty
  pub money: i32,
  /// If not empty, shows a "Job Payment Received" popup, also see [expedition_set] docs.
  pub items: Vec<ExpeditionItem>,
}

impl CallCustom for ExpeditionSet {}

// See [Wonder_Api_ExpeditionsetUpdatesResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ExpeditionUpdate {
  pub expedition_id: i32,
  pub user_character_id: i32,
  pub love: i32,
  #[serde(rename = "starttime")]
  pub start_time: String,
}

// See [Wonder_Api_ExpeditionsetItemsResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ExpeditionItem {
  pub item_type: i32,
  pub item_id: i64,
  pub item_num: i32,
}

/// If both [expedition_id] and [user_character_id] are zero, "Claim All" is performed.
#[derive(Debug, Deserialize)]
pub struct ExpeditionSetRequest {
  pub expedition_id: i32,
  pub user_character_id: i32,
}

/// Called when either assigning a character or claiming rewards.
/// If claiming rewards, "Job Payment Received" popup is always shown even if [ExpeditionSet.items] is empty.
///
/// - `expedition_id = 0, user_character_id = 0`: "Accept All", claims every job and keeps the characters working.
/// - job without a character: assigns `user_character_id` to it.
/// - job with a character: claims its rewards; the job keeps going with `user_character_id`
///   (or is left empty if it is 0).
pub async fn expedition_set(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ExpeditionSetRequest>,
) -> impl IntoHandlerResponse {
  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;

  #[rustfmt::skip]
  let rows = transaction.query(/* language=postgresql */ r#"
    select expedition_id, character_id, started_at
    from user_expeditions
    where user_id = $1 and ($2 = 0 or expedition_id = $2)
    for update
  "#, &[&session.user_id, &params.expedition_id]).await.context("failed to fetch expeditions")?;

  let masters = get_master_manager();
  let mut updates = Vec::new();
  let mut money = 0;
  let mut items: BTreeMap<(i32, i64), i32> = BTreeMap::new();
  let mut intimacy: BTreeMap<i64, i32> = BTreeMap::new();

  if rows.is_empty() && params.expedition_id != 0 && params.user_character_id != 0 {
    // Assign a character to an empty job
    #[rustfmt::skip]
    transaction.execute(/* language=postgresql */ r#"
      insert into user_expeditions (user_id, expedition_id, character_id, started_at)
      values ($1, $2, $3, now())
    "#, &[&session.user_id, &params.expedition_id, &(params.user_character_id as i64)]).await
      .context("failed to assign expedition")?;
    info!(?params, "character assigned to job");
    updates.push(ExpeditionUpdate {
      expedition_id: params.expedition_id,
      user_character_id: params.user_character_id,
      love: 0,
      start_time: format_time(Utc::now()),
    });
  }

  for row in &rows {
    let expedition_id: i32 = row.get("expedition_id");
    let character_id: i64 = row.get("character_id");
    let started_at: DateTime<Utc> = row.get("started_at");
    let counts = counts_since(started_at);

    // Money and affinity per count, from the `expedition_count` master
    for entry in masters.get_master("expedition_count") {
      let field = |name: &str| entry[name].as_str().and_then(|value| value.parse::<i64>().ok()).unwrap_or(0);
      if field("expedition_id") != expedition_id as i64 {
        continue;
      }
      // Rows are "from count N on, give X": only the first row (count 1) gives anything.
      if field("count_money") == 1 {
        money += (field("money") * counts) as i32;
      }
      if field("count_intimacy") == 1 {
        *intimacy.entry(character_id).or_default() += (field("intimacy") * counts) as i32;
      }
    }

    // One item roll per count, weighted by `expedition_drop`
    let drops = masters
      .get_master("expedition_drop")
      .iter()
      .filter(|drop| drop["expedition_id"].as_str() == Some(&expedition_id.to_string()))
      .filter_map(|drop| {
        let field = |name: &str| drop[name].as_str()?.parse::<i64>().ok();
        Some((field("lv1_item_type")? as i32, field("lv1_item_id")?, field("lv1_item_num")? as i32, field("lv1_weight")?))
      })
      .filter(|&(item_type, _, _, weight)| item_type != 0 && weight > 0)
      .collect::<Vec<_>>();
    let total_weight: i64 = drops.iter().map(|drop| drop.3).sum();
    if total_weight > 0 {
      let mut rng = rand::rng();
      for _ in 0..counts {
        let mut roll = rng.random_range(0..total_weight);
        for &(item_type, item_id, item_num, weight) in &drops {
          if roll < weight {
            *items.entry((item_type, item_id)).or_default() += item_num;
            break;
          }
          roll -= weight;
        }
      }
    }

    // Keep working (same or new character) or leave the job empty
    let new_character = if params.expedition_id == 0 {
      character_id
    } else {
      params.user_character_id as i64
    };
    if new_character == 0 {
      #[rustfmt::skip]
      transaction.execute(/* language=postgresql */ r#"
        delete from user_expeditions where user_id = $1 and expedition_id = $2
      "#, &[&session.user_id, &expedition_id]).await.context("failed to clear expedition")?;
    } else {
      #[rustfmt::skip]
      transaction.execute(/* language=postgresql */ r#"
        update user_expeditions
        set character_id = $3, started_at = now()
        where user_id = $1 and expedition_id = $2
      "#, &[&session.user_id, &expedition_id, &new_character]).await.context("failed to restart expedition")?;
    }
    updates.push(ExpeditionUpdate {
      expedition_id,
      user_character_id: new_character as i32,
      love: intimacy.get(&character_id).copied().unwrap_or(0),
      start_time: format_time(Utc::now()),
    });
    info!(?expedition_id, ?character_id, ?counts, "job rewards claimed");
  }

  // Affinity
  for (&character_id, &amount) in &intimacy {
    #[rustfmt::skip]
    transaction.execute(/* language=postgresql */ r#"
      update user_characters set intimacy = intimacy + $3
      where user_id = $1 and character_id = $2
    "#, &[&session.user_id, &character_id, &amount]).await.context("failed to add affinity")?;
  }

  // Items
  let update_items = UpdateItemCountBy::new(&transaction).await?;
  let mut response_items = Vec::new();
  let mut remote = Vec::new();
  for (&(item_type, item_id), &item_num) in &items {
    let item = update_items
      .run(session.user_id, (RemoteDataItemType::from(item_type), item_id), item_num)
      .await
      .context("failed to grant job reward")?;
    remote.extend(item.into_remote_data());
    response_items.push(ExpeditionItem {
      item_type,
      item_id,
      item_num,
    });
  }
  // Eris: also shown in `money`
  let mut wallet = Wallet::load(&transaction, session.user_id).await?;
  wallet.add(RemoteDataItemType::Money, money as i64)?;
  wallet.save(&transaction, session.user_id).await?;
  remote.extend(wallet.to_remote_data());

  transaction.commit().await.context("failed to commit transaction")?;
  info!(?money, ?items, ?intimacy, "job rewards");

  let mut response = CallResponse::new_success(Box::new(ExpeditionSet {
    updates,
    money,
    items: response_items,
  }));
  response.add_remote_data(remote);
  Ok(Signed(response, session))
}
