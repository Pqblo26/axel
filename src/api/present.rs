//! Gift box ("Gifts" on the home screen), backed by the `user_presents` table.
//!
//! Presents are added to the database (by hand, by a script or by the server itself, e.g. the
//! welcome gift) and the client lists and accepts them here.

use std::sync::Arc;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::AppState;
use crate::api::{NotificationData, RemoteDataItemType};
use crate::blob::IntoRemoteData;
use crate::user::wallet::Wallet;
use crate::call::{CallCustom, CallResponse};
use crate::database::QueryExecutor;
use crate::extractor::Params;
use crate::handler::{IntoHandlerResponse, Signed};
use crate::item::UpdateItemCountBy;
use crate::user::id::UserId;
use crate::user::session::Session;

/// The client can hold up to 100 presents at a time.
const MAX_PRESENTS: i64 = 100;

/// Template ID of the present. The client does not seem to care as long as `msg` is set.
const PRESENT_TEMPLATE_ID: i32 = 48;

/// See [Wonder.Util.NotificationDataSet$$ClearBadge]: badge key 2 is the Gifts button counter.
const PRESENT_BADGE_KEY: i32 = 2;

/// Badge with the number of presents waiting in the gift box.
pub fn present_badge(count: i64) -> NotificationData {
  NotificationData::new(1, 7, PRESENT_BADGE_KEY, count as i32, "".to_owned(), "".to_owned())
}

/// Number of presents that can still be accepted (not received and not expired).
pub async fn count_pending_presents<'a>(
  executor: impl Into<QueryExecutor<'a>>,
  user_id: UserId,
) -> anyhow::Result<i64> {
  let executor = executor.into();
  #[rustfmt::skip]
  let row = executor.client().query_one(/* language=postgresql */ r#"
    select count(*)
    from user_presents
    where user_id = $1
      and received_at is null
      and (expires_at is null or expires_at > now())
  "#, &[&user_id]).await.context("failed to count presents")?;
  Ok(row.get(0))
}

/// How a present is granted when accepted.
pub(crate) enum Grant {
  /// Currencies and stamina, which the client keeps as a single number: stored in `user_wallet`.
  Singleton,
  /// Countable items stored in `user_items` (tickets, potions, materials, medals...).
  Item,
  /// Members (a reserve copy if already owned), see [crate::user::grant::grant_member].
  Member,
  /// Equipment and other unique things: not supported yet, stay in the gift box.
  Unsupported,
}

pub(crate) fn grant_kind(item_type: RemoteDataItemType) -> Grant {
  use RemoteDataItemType::*;
  match item_type {
    Money | RealMoney | RealMoneyFree | Stamina => Grant::Singleton,
    Member => Grant::Member,
    Weapon | Accessory | Character | SpecialSkill | MemberCostume | MemberBackground | Assist | Exp
    | Level | FameRank => Grant::Unsupported,
    _ => Grant::Item,
  }
}

/// Far-future expiry date for presents that never expire.
fn expiry_timestamp(expires_at: Option<DateTime<Utc>>) -> i64 {
  expires_at
    .unwrap_or_else(|| Utc::now() + chrono::Duration::days(365))
    .timestamp()
}

// See [Wonder_Api_PresentlistResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct PresentList {
  pub presents: Vec<Present>,
}

impl CallCustom for PresentList {}

// See [Wonder_Api_PresentlistPresentsResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct Present {
  pub id: i32,
  pub present_id: i32,
  pub senddate: i64,
  pub expireddate: i64,
  pub item_type: i32,
  /// Must be `1` even for singleton items.
  pub item_id: i64,
  pub item_num: i32,
  pub msg: String,
}

#[derive(Debug, Deserialize)]
pub struct PresentListRequest {
  pub start: i32,
  pub end: i32,
}

pub async fn present_list(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<PresentListRequest>,
) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;

  #[rustfmt::skip]
  let rows = client.query(/* language=postgresql */ r#"
    select id, item_type, item_id, item_num, message, sent_at, expires_at
    from user_presents
    where user_id = $1
      and received_at is null
      and (expires_at is null or expires_at > now())
    order by sent_at desc, id desc
    limit $2
  "#, &[&session.user_id, &MAX_PRESENTS]).await.context("failed to fetch presents")?;

  let presents = rows
    .iter()
    .map(|row| {
      let sent_at: DateTime<Utc> = row.get("sent_at");
      Present {
        id: row.get::<_, i64>("id") as i32,
        present_id: PRESENT_TEMPLATE_ID,
        senddate: sent_at.timestamp(),
        expireddate: expiry_timestamp(row.get("expires_at")),
        item_type: row.get("item_type"),
        item_id: row.get("item_id"),
        item_num: row.get("item_num"),
        msg: row.get("message"),
      }
    })
    .collect::<Vec<_>>();
  info!(?params.start, ?params.end, count = presents.len(), "present list");

  let count = count_pending_presents(&client, session.user_id).await?;
  let mut response = CallResponse::new_success(Box::new(PresentList { presents }));
  response.add_notifications(vec![present_badge(count)]);

  Ok(Signed(response, session))
}

// See [Wonder_Api_PresentloglistResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct PresentLogList {
  pub presents: Vec<PresentLog>,
}

impl CallCustom for PresentLogList {}

// See [Wonder_Api_PresentloglistPresentsResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct PresentLog {
  pub id: i32,
  pub present_id: i32,
  pub senddate: i64,
  pub recveddate: i64,
  pub item_type: i32,
  /// Must be `1` even for singleton items.
  pub item_id: i64,
  pub item_num: i32,
  pub msg: String,
}

#[derive(Debug, Deserialize)]
pub struct PresentLogListRequest {
  pub start: i32,
  pub end: i32,
}

pub async fn present_log_list(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<PresentLogListRequest>,
) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;

  #[rustfmt::skip]
  let rows = client.query(/* language=postgresql */ r#"
    select id, item_type, item_id, item_num, message, sent_at, received_at
    from user_presents
    where user_id = $1
      and received_at is not null
    order by received_at desc, id desc
    limit $2
  "#, &[&session.user_id, &MAX_PRESENTS]).await.context("failed to fetch present log")?;

  let presents = rows
    .iter()
    .map(|row| {
      let sent_at: DateTime<Utc> = row.get("sent_at");
      let received_at: DateTime<Utc> = row.get("received_at");
      PresentLog {
        id: row.get::<_, i64>("id") as i32,
        present_id: PRESENT_TEMPLATE_ID,
        senddate: sent_at.timestamp(),
        recveddate: received_at.timestamp(),
        item_type: row.get("item_type"),
        item_id: row.get("item_id"),
        item_num: row.get("item_num"),
        msg: row.get("message"),
      }
    })
    .collect::<Vec<_>>();
  info!(?params.start, ?params.end, count = presents.len(), "present log list");

  let response = CallResponse::new_success(Box::new(PresentLogList { presents }));
  Ok(Signed(response, session))
}

// See [Wonder_Api_PresentgetResponseDto_Fields]
#[derive(Debug, Serialize)]
struct PresentGet {
  pub presents: Vec<PresentGetReceived>,
  pub unrecvpresents: Vec<PresentGetUnreceived>,
}

impl CallCustom for PresentGet {}

// See [Wonder_Api_PresentgetPresentsResponseDto_Fields]
#[derive(Debug, Serialize)]
struct PresentGetReceived {
  pub id: i32,
  pub present_id: i32,
  pub senddate: i64,
  pub expireddate: i64,
  pub item_type: i32,
  /// Must be `1` even for singleton items.
  pub item_id: i64,
  pub item_num: i32,
}

// See [Wonder_Api_PresentgetUnrecvpresentsResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct PresentGetUnreceived {
  pub id: i32,
  pub present_id: i32,
  pub senddate: i64,
  pub expireddate: i64,
  pub item_type: i32,
  /// Must be `1` even for singleton items.
  pub item_id: i64,
  pub item_num: i32,
}

#[derive(Debug, Deserialize)]
pub struct PresentGetRequest {
  #[serde(deserialize_with = "crate::serde_compat::comma_separated_i32")]
  pub ids: Vec<i32>,
}

/// Called by both "Accept" (one ID) and "Accept All" (every listed ID).
pub async fn present_get(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<PresentGetRequest>,
) -> impl IntoHandlerResponse {
  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;

  let ids = params.ids.iter().map(|&id| id as i64).collect::<Vec<_>>();
  #[rustfmt::skip]
  let rows = transaction.query(/* language=postgresql */ r#"
    select id, item_type, item_id, item_num, sent_at, expires_at
    from user_presents
    where user_id = $1
      and id = any($2)
      and received_at is null
      and (expires_at is null or expires_at > now())
    order by id
    for update
  "#, &[&session.user_id, &ids]).await.context("failed to fetch presents")?;

  let update_items = UpdateItemCountBy::new(&transaction).await?;
  let mut wallet = Wallet::load(&transaction, session.user_id).await?;
  #[rustfmt::skip]
  let mark_received = transaction.prepare(/* language=postgresql */ r#"
    update user_presents
    set received_at = now()
    where id = $1
  "#).await.context("failed to prepare statement")?;

  let mut received = Vec::new();
  let mut unreceived = Vec::new();
  let mut remote = Vec::new();
  for row in &rows {
    let id: i64 = row.get("id");
    let item_type_raw: i32 = row.get("item_type");
    let item_type = RemoteDataItemType::from(item_type_raw);
    let item_id: i64 = row.get("item_id");
    let item_num: i32 = row.get("item_num");
    let sent_at: DateTime<Utc> = row.get("sent_at");
    let expireddate = expiry_timestamp(row.get("expires_at"));

    match grant_kind(item_type) {
      Grant::Singleton => {
        wallet.add(item_type, item_num as i64)?;
      }
      Grant::Item => {
        let item = update_items
          .run(session.user_id, (item_type, item_id), item_num)
          .await
          .context("failed to grant present item")?;
        remote.extend(item.into_remote_data());
      }
      Grant::Member => {
        for _ in 0..item_num.max(1) {
          remote.extend(crate::user::grant::grant_member(&transaction, session.user_id, item_id).await?);
        }
      }
      Grant::Unsupported => {
        warn!(
          ?id,
          ?item_type,
          ?item_id,
          ?item_num,
          "present type not supported yet, left in the gift box"
        );
        unreceived.push(PresentGetUnreceived {
          id: id as i32,
          present_id: PRESENT_TEMPLATE_ID,
          senddate: sent_at.timestamp(),
          expireddate,
          item_type: item_type_raw,
          item_id,
          item_num,
        });
        continue;
      }
    }

    transaction
      .execute(&mark_received, &[&id])
      .await
      .context("failed to mark present as received")?;
    received.push(PresentGetReceived {
      id: id as i32,
      present_id: PRESENT_TEMPLATE_ID,
      senddate: sent_at.timestamp(),
      expireddate,
      item_type: item_type_raw,
      item_id,
      item_num,
    });
  }

  wallet.save(&transaction, session.user_id).await?;
  remote.extend(wallet.to_remote_data());
  let count = count_pending_presents(&transaction, session.user_id).await?;
  transaction.commit().await.context("failed to commit transaction")?;
  info!(
    requested = ?params.ids,
    received = received.len(),
    unreceived = unreceived.len(),
    pending = count,
    "presents accepted"
  );

  let mut response = CallResponse::new_success(Box::new(PresentGet {
    presents: received,
    unrecvpresents: unreceived,
  }));
  response.add_remote_data(remote);
  response.add_notifications(vec![present_badge(count)]);

  Ok(Signed(response, session))
}

/// Presents every user gets once (see `add_user_welcome_presents` in migrations.rs).
/// (source, item type, item id, amount, message)
pub const WELCOME_PRESENTS: &[(&str, RemoteDataItemType, i64, i32, &str)] = &[
  (
    "welcome:quartz",
    RemoteDataItemType::RealMoneyFree,
    1,
    3000,
    "¡Bienvenido a tu servidor!",
  ),
  (
    "welcome:stamina_item",
    RemoteDataItemType::StaminaItem,
    1,
    5,
    "¡Bienvenido a tu servidor!",
  ),
  (
    "welcome:power_potion",
    RemoteDataItemType::PowerPotion,
    3,
    10,
    "¡Bienvenido a tu servidor!",
  ),
  (
    "welcome:skip_tickets",
    RemoteDataItemType::SkipTicket,
    1,
    20,
    "¡Bienvenido a tu servidor!",
  ),
];
