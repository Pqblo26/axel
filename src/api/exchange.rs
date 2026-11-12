use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};
use tracing::{debug, info, trace, warn};

use crate::AppState;
use crate::api::RemoteDataItemType;
use crate::api::master_all::{get_master_manager, get_masters, get_masters_definitely_initialized};
use crate::api::present::{Grant, grant_kind};
use crate::blob::{DeleteMember, IntoRemoteData};
use crate::user::wallet::Wallet;
use crate::database::QueryExecutor;
use crate::user::id::UserId;
use crate::call::{CallCustom, CallResponse};
use crate::extractor::Params;
use crate::handler::{IntoHandlerResponse, Signed};
use crate::item::UpdateItemCountBy;
use crate::member::MemberPrototype;
use crate::user::session::Session;

// See [Wonder_Api_ExchangelistResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ExchangeList {
  pub exchange_master_id: i32,
  pub items: Vec<ExchangeItem>,
}

impl CallCustom for ExchangeList {}

// See [Wonder_Api_ExchangelistItemsResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ExchangeItem {
  pub exchange_reward_master_id: i32,
  pub limit: i32,
  pub exchange_num: i32,
}

#[derive(Debug, Deserialize)]
pub struct ExchangeListRequest {
  pub exchange_master_id: i32,
}

pub async fn exchange_list(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ExchangeListRequest>,
) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;
  let masters = get_masters().await;
  let master_items: Vec<Value> = serde_json::from_str(&masters["exchange_item"].master_decompressed).unwrap();
  let mut items = Vec::new();
  for item in &master_items {
    let Some(entry) = ExchangeEntry::from_master(item) else { continue };
    if entry.exchange_id != params.exchange_master_id {
      continue;
    }
    let exchanged = count_exchanged(&client, session.user_id, &entry).await?;
    items.push(ExchangeItem {
      exchange_reward_master_id: entry.id,
      // 0 = no limit; the client shows "limit - exchange_num left"
      limit: entry.limit,
      exchange_num: exchanged,
    });
  }

  Ok(Signed(
    ExchangeList {
      exchange_master_id: params.exchange_master_id,
      items,
    },
    session,
  ))
}

/// One row of the `exchange_item` master.
#[derive(Debug, Clone)]
struct ExchangeEntry {
  id: i32,
  exchange_id: i32,
  /// Cost of the first exchange: (item type, item id, amount)
  first_cost: Vec<(i32, i64, i32)>,
  /// Cost of every exchange after the first one
  cost: Vec<(i32, i64, i32)>,
  pack_id: String,
  /// "0" (no limit), "NUM" (ever), "MONTH", "WEEK"
  limit_type: String,
  limit: i32,
}

impl ExchangeEntry {
  fn from_master(item: &Value) -> Option<Self> {
    let int = |name: &str| item[name].as_str().and_then(|value| value.parse::<i64>().ok()).unwrap_or(0);
    let costs = |suffix: &str, id_prefix: &str, num_prefix: &str| {
      (1..=3)
        .map(|n| {
          let item_type = int(&format!("item_type{n}{suffix}")) as i32;
          let item_id = int(&format!("{id_prefix}{n}"));
          let num = int(&format!("{num_prefix}{n}{suffix}")) as i32;
          (item_type, item_id, num)
        })
        .filter(|&(item_type, _, num)| item_type != 0 && num > 0)
        .collect::<Vec<_>>()
    };
    Some(Self {
      id: item["id"].as_str()?.parse().ok()?,
      exchange_id: item["exchange_id"].as_str()?.parse().ok()?,
      first_cost: costs("_first", "first_id", "num"),
      cost: costs("", "id", "num"),
      pack_id: item["pack_id"].as_str()?.to_owned(),
      limit_type: item["limit_type"].as_str().unwrap_or("0").to_owned(),
      limit: if item["limit_type"].as_str().unwrap_or("0") == "0" { 0 } else { int("limit") as i32 },
    })
  }
}

/// How many times this item was exchanged in the current limit period.
async fn count_exchanged<'a>(
  executor: impl Into<QueryExecutor<'a>>,
  user_id: UserId,
  entry: &ExchangeEntry,
) -> anyhow::Result<i32> {
  let period = match entry.limit_type.as_str() {
    "MONTH" => "month",
    "WEEK" => "week",
    _ => "",
  };
  let executor = executor.into();
  #[rustfmt::skip]
  let row = executor.client().query_one(/* language=postgresql */ r#"
    select coalesce(sum(num), 0)::int
    from user_exchanges
    where user_id = $1
      and exchange_item_id = $2
      and ($3 = '' or exchanged_at >= date_trunc($3, now()))
  "#, &[&user_id, &entry.id, &period]).await.context("failed to count exchanges")?;
  Ok(row.get(0))
}

/// `pack` master (what an exchange or shop item gives), indexed by ID. Large, so parsed once.
pub(crate) fn get_pack(pack_id: &str) -> Option<&'static Value> {
  static PACKS: OnceLock<HashMap<String, Value>> = OnceLock::new();
  PACKS
    .get_or_init(|| {
      let packs: Vec<Value> =
        serde_json::from_str(&get_masters_definitely_initialized()["pack"].master_decompressed).unwrap();
      packs
        .into_iter()
        .filter_map(|pack| Some((pack["id"].as_str()?.to_owned(), pack)))
        .collect()
    })
    .get(pack_id)
}

/// Items in a pack: the main item and up to 5 bonuses.
pub(crate) fn pack_contents(pack: &Value) -> Vec<(i32, i64, i32)> {
  let int = |name: &str| pack[name].as_str().and_then(|value| value.parse::<i64>().ok()).unwrap_or(0);
  let mut contents = vec![(int("item_type") as i32, int("item_id"), int("item_num") as i32)];
  for n in 1..=5 {
    contents.push((
      int(&format!("bonus{n}_item_type")) as i32,
      int(&format!("bonus{n}_item_id")),
      int(&format!("bonus{n}_item_num")) as i32,
    ));
  }
  contents
    .into_iter()
    .filter(|&(item_type, _, num)| item_type != 0 && num > 0)
    .collect()
}

#[derive(Debug, Deserialize)]
pub struct LeaveMembersRequest {
  #[serde(deserialize_with = "crate::serde_compat::comma_separated_i64")]
  pub ids: Vec<i64>,
}

pub async fn leave_members(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<LeaveMembersRequest>,
) -> impl IntoHandlerResponse {
  let medal_rate = get_master_manager()
    .get_master("member_medal_rate")
    .into_iter()
    .map(|m| {
      let rarity = m["rare"].as_str().unwrap().parse::<i32>().unwrap();
      let medals = m["medal_rate"].as_str().unwrap().parse::<i32>().unwrap();
      (rarity, medals)
    })
    .collect::<HashMap<_, _>>();

  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;

  #[rustfmt::skip]
  let statement = transaction.prepare(/* language=postgresql */ r#"
    delete from user_members_reserve
    where user_id = $1
      and id = any($2)
    returning id, member_id
  "#).await.context("failed to prepare statement")?;
  let rows = transaction
    .query(&statement, &[&session.user_id, &params.ids])
    .await
    .context("failed to execute statement")?;

  // See [Wonder_Api_LeavemenbersResponseDto_Fields]
  let mut response = CallResponse::new_success_empty();

  let mut total_medal_count = 0;
  for row in rows {
    let id: i64 = row.get(0);
    let member_id: i64 = row.get(1);

    let prototype = MemberPrototype::load_from_id(member_id);
    let medal_count = *medal_rate
      .get(&prototype.rarity)
      .context(format!("no medal rate found for member rarity {}", prototype.rarity))?;
    trace!(
      ?id,
      ?member_id,
      ?medal_count,
      "calculated medals for releasing reserve member"
    );

    total_medal_count += medal_count;
    response.remote.extend(DeleteMember::new(id, member_id).into_remote_data());
  }

  const ADVENTURER_MEDAL_ID: i64 = 1001;
  let update = UpdateItemCountBy::new(&transaction).await?;
  let item = update
    .run(
      session.user_id,
      (RemoteDataItemType::ExchangeMedal, ADVENTURER_MEDAL_ID),
      total_medal_count,
    )
    .await
    .context("failed to execute query")?;
  info!(members = ?params.ids, ?item, "granted medals for releasing reserve members");
  response.remote.extend(item.into_remote_data());

  transaction.commit().await.context("failed to commit transaction")?;

  Ok(Signed(response, session))
}

#[derive(Debug, Deserialize)]
pub struct ExchangeRequest {
  pub exchange_reward_master_id: i32,
  pub num: i32,
}

// exchange_reward_master_id=10001
// num=1
pub async fn exchange(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ExchangeRequest>,
) -> impl IntoHandlerResponse {
  // See [Wonder_Api_ExchangeResponseDto_Fields]: only remote data matters
  let mut response = CallResponse::new_success_empty();
  if params.num <= 0 {
    return Ok(Signed(response, session));
  }

  let masters = get_masters().await;
  let master_items: Vec<Value> = serde_json::from_str(&masters["exchange_item"].master_decompressed).unwrap();
  let Some(entry) = master_items
    .iter()
    .filter_map(ExchangeEntry::from_master)
    .find(|entry| entry.id == params.exchange_reward_master_id)
  else {
    warn!(?params, "unknown exchange item");
    return Ok(Signed(response, session));
  };

  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;

  let exchanged = count_exchanged(&transaction, session.user_id, &entry).await?;
  if entry.limit > 0 && exchanged + params.num > entry.limit {
    warn!(?params, ?exchanged, limit = entry.limit, "exchange limit reached");
    return Ok(Signed(response, session));
  }

  // Total cost: first-time price for the very first exchange, normal price for the rest
  let mut cost: BTreeMap<(i32, i64), i32> = BTreeMap::new();
  for unit in 0..params.num {
    let unit_cost = if exchanged == 0 && unit == 0 { &entry.first_cost } else { &entry.cost };
    for &(item_type, item_id, num) in unit_cost {
      *cost.entry((item_type, item_id)).or_default() += num;
    }
  }

  // Check and pay
  let update_items = UpdateItemCountBy::new(&transaction).await?;
  let mut wallet = Wallet::load(&transaction, session.user_id).await?;
  for (&(item_type, item_id), &num) in &cost {
    let kind = RemoteDataItemType::from(item_type);
    if matches!(grant_kind(kind), Grant::Singleton) {
      if let Err(error) = wallet.add(kind, -(num as i64)) {
        warn!(?params, %error, "not enough to exchange");
        return Ok(Signed(CallResponse::new_success_empty(), session));
      }
      continue;
    }
    #[rustfmt::skip]
    let owned: i32 = transaction.query_opt(/* language=postgresql */ r#"
      select quantity from user_items where user_id = $1 and item_type = $2 and item_id = $3
    "#, &[&session.user_id, &(item_type as i64), &item_id]).await.context("failed to read item")?
      .map(|row| row.get(0))
      .unwrap_or(0);
    if owned < num {
      warn!(?params, ?item_type, ?item_id, ?owned, needed = num, "not enough items to exchange");
      return Ok(Signed(CallResponse::new_success_empty(), session));
    }
    let item = update_items.run(session.user_id, (kind, item_id), -num).await?;
    response.remote.extend(item.into_remote_data());
  }

  // Receive
  let Some(pack) = get_pack(&entry.pack_id) else {
    warn!(?params, pack_id = entry.pack_id, "exchange pack not found");
    return Ok(Signed(CallResponse::new_success_empty(), session));
  };
  for (item_type, item_id, num) in pack_contents(pack) {
    let kind = RemoteDataItemType::from(item_type);
    let num = num * params.num;
    match grant_kind(kind) {
      Grant::Singleton => {
        wallet.add(kind, num as i64)?;
      }
      Grant::Item => {
        let item = update_items.run(session.user_id, (kind, item_id), num).await?;
        response.remote.extend(item.into_remote_data());
      }
      Grant::Unsupported => {
        warn!(?params, ?item_type, ?item_id, ?num, "exchange reward type not supported yet, not given");
      }
    }
  }

  #[rustfmt::skip]
  transaction.execute(/* language=postgresql */ r#"
    insert into user_exchanges (user_id, exchange_item_id, num) values ($1, $2, $3)
  "#, &[&session.user_id, &entry.id, &params.num]).await.context("failed to log exchange")?;
  wallet.save(&transaction, session.user_id).await?;
  transaction.commit().await.context("failed to commit transaction")?;
  response.remote.extend(wallet.to_remote_data());
  info!(?params, ?cost, pack_id = entry.pack_id, "exchanged");

  Ok(Signed(response, session))
}
