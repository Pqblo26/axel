use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::{info, warn};

use crate::api::exchange::{get_pack, pack_contents};
use crate::api::master_all::{get_master_manager, get_masters};
use crate::api::present::{grant_kind, Grant};
use crate::api::RemoteDataItemType;
use crate::blob::IntoRemoteData;
use crate::call::{CallCustom, CallResponse, STATUS_QUARTZ_NOT_ENOUGH};
use crate::extractor::Params;
use crate::handler::{IntoHandlerResponse, Unsigned};
use crate::item::UpdateItemCountBy;
use crate::user::id::UserId;
use crate::user::session::Session;
use crate::user::wallet::Wallet;
use crate::AppState;

// See [Wonder_Api_AdvertisementRewardStatusResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct AdvertisementRewardStatus {
  pub advertisement_data_list: Vec<AdvertisementData>,
}

impl CallCustom for AdvertisementRewardStatus {}

// See [Wonder_Api_AdvertisementDataResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct AdvertisementData {
  pub id: i32,
  pub reward_type: i32,
  pub status: i32,
}

#[derive(Debug, Deserialize)]
pub struct AdvertisementRewardStatusRequest {
  pub reward_type_list: Vec<i32>,
}

// reward_type_list=[3]
pub async fn advertisement_reward_status(
  Params(params): Params<AdvertisementRewardStatusRequest>,
) -> impl IntoHandlerResponse {
  let masters = get_masters().await;
  let ad_rewards: Vec<Value> = serde_json::from_str(&masters["ad_reward"].master_decompressed).unwrap();

  warn!(?params, "encountered stub: advertisement_reward_status");

  Ok(Unsigned(AdvertisementRewardStatus {
    advertisement_data_list: ad_rewards
      .iter()
      .map(|ad_reward| AdvertisementData {
        id: ad_reward["id"].as_str().unwrap().parse().unwrap(),
        reward_type: ad_reward["reward_type"].as_str().unwrap().parse().unwrap(),
        status: 0,
      })
      .collect(),
  }))
}

// See [Wonder_Api_ShopitemlistResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ShopItemList {
  pub items: Vec<ShopItem>,
}

impl CallCustom for ShopItemList {}

// See [Wonder_Api_ShopitemlistItemsResponseDto_Fields]
#[derive(Debug, Serialize)]
pub struct ShopItem {
  pub shop_item_master_id: i32,
  pub interval_time: i32,
  pub buycount: i32,
}

#[derive(Debug, Deserialize)]
pub struct ShopItemListRequest {
  pub shop_master_id: i32,
}

// shop_master_id=4
pub async fn shop_item_list(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ShopItemListRequest>,
) -> impl IntoHandlerResponse {
  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;
  let mut items = Vec::new();
  for item in get_master_manager().get_master("shop_item") {
    if item["shop_id"].as_str() != Some(&params.shop_master_id.to_string()) {
      continue;
    }
    // Real-money items need Google Play Billing, which hangs the Shop (see patch_master "shop_item")
    if !matches!(item["google_yen_price"].as_str(), None | Some("0") | Some("")) {
      continue;
    }
    items.push(ShopItem {
      shop_item_master_id: item["id"].as_str().unwrap().parse().unwrap(),
      interval_time: 0,
      buycount: bought_in_period(&transaction, session.user_id, item).await?,
    });
  }

  Ok(Unsigned(ShopItemList { items }))
}

// See [Wonder_Api_BuyResponseDto_Fields]
#[derive(Debug, Default, Serialize)]
pub struct BuyResponse {
  pub buymoney: i32,
  pub buyrealmoney: i32,
  pub buyrealmoneyfree: i32,
  pub money: i32,
  pub realmoney: i32,
  pub realmoneyfree: i32,
}

impl CallCustom for BuyResponse {}

#[derive(Debug, Deserialize)]
pub struct BuyRequest {
  pub money_type: i32,
  pub count: i32,
  pub shop_item_master_id: i32,
}

// money_type=2
// count=2
// shop_item_master_id=11000
/// Buys a shop item with eris or quartz: stamina (shop 4), dungeon challenges (8), dungeon benefit
/// re-draws (7) and Fame Potions (9). Real-money items never get here (see [shop_item_list]).
pub async fn buy(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<BuyRequest>,
) -> impl IntoHandlerResponse {
  Ok(Unsigned(purchase(&state, &session, &params).await?))
}

#[derive(Debug, Deserialize)]
pub struct FameSupplementBuyRequest {
  pub buy_count: i32,
}

/// Fame Quest "+" button: buys Fame Potions (shop item 14003) with quartz.
pub async fn fame_supplement_buy(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<FameSupplementBuyRequest>,
) -> impl IntoHandlerResponse {
  let request = BuyRequest {
    money_type: 2,
    count: params.buy_count,
    shop_item_master_id: FAME_SUPPLEMENT_SHOP_ITEM_ID,
  };
  Ok(Unsigned(purchase(&state, &session, &request).await?))
}

const FAME_SUPPLEMENT_SHOP_ITEM_ID: i32 = 14003;

async fn purchase(state: &AppState, session: &Session, params: &BuyRequest) -> anyhow::Result<CallResponse<BuyResponse>> {
  let Some(item) = get_master_manager()
    .get_master("shop_item")
    .iter()
    .find(|item| item["id"].as_str() == Some(&params.shop_item_master_id.to_string()))
  else {
    warn!(?params, "unknown shop item");
    return Ok(CallResponse::new_success(Box::new(BuyResponse::default())));
  };
  let field = |name: &str| item[name].as_str().and_then(|value| value.parse::<i32>().ok()).unwrap_or(0);
  let count = params.count.max(1);

  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;
  let mut wallet = Wallet::load(&transaction, session.user_id).await?;
  let totals = |wallet: &Wallet, spent: (i32, i32, i32)| BuyResponse {
    buymoney: spent.0,
    buyrealmoney: spent.1,
    buyrealmoneyfree: spent.2,
    money: wallet.money.min(i32::MAX as i64) as i32,
    realmoney: wallet.real_money,
    realmoneyfree: wallet.real_money_free,
  };

  let limit = field("limit");
  let bought = bought_in_period(&transaction, session.user_id, item).await?;
  if limit > 0 && bought + count > limit {
    warn!(?params, ?bought, ?limit, "shop buy limit reached");
    return Ok(CallResponse::new_success(Box::new(totals(&wallet, (0, 0, 0)))));
  }

  // The first purchase of the period may have its own price
  let price = |kind: &str| {
    let first = field(&format!("pay_{kind}_first"));
    let second = field(&format!("pay_{kind}_second"));
    if bought == 0 { first + second * (count - 1) } else { second * count }
  };
  let spent = if params.money_type == 1 {
    let cost = price("money");
    if wallet.add(RemoteDataItemType::Money, -(cost as i64)).is_err() {
      warn!(?params, ?cost, money = wallet.money, "not enough eris");
      return Ok(CallResponse::new_custom(
        STATUS_QUARTZ_NOT_ENOUGH,
        Box::new(totals(&wallet, (0, 0, 0))),
      ));
    }
    (cost, 0, 0)
  } else {
    let cost = price("realmoneyfree").max(price("realmoney"));
    match wallet.spend_quartz(cost) {
      Ok((free, paid)) => (0, paid, free),
      Err(error) => {
        warn!(?params, %error, "not enough quartz");
        return Ok(CallResponse::new_custom(
          STATUS_QUARTZ_NOT_ENOUGH,
          Box::new(totals(&wallet, (0, 0, 0))),
        ));
      }
    }
  };

  // Give the pack
  let mut remote = Vec::new();
  let update_items = UpdateItemCountBy::new(&transaction).await?;
  let pack_id = item["pack_id"].as_str().unwrap_or_default();
  let Some(pack) = get_pack(pack_id) else {
    warn!(?params, ?pack_id, "shop pack not found, nothing bought");
    return Ok(CallResponse::new_success(Box::new(BuyResponse::default())));
  };
  for (item_type, item_id, num) in pack_contents(pack) {
    let kind = RemoteDataItemType::from(item_type);
    let num = num * count;
    match grant_kind(kind) {
      Grant::Singleton => {
        wallet.add(kind, num as i64)?;
      }
      Grant::Item => {
        let item = update_items.run(session.user_id, (kind, item_id), num).await?;
        remote.extend(item.into_remote_data());
      }
      Grant::Unsupported => {
        warn!(?params, ?item_type, ?item_id, ?num, "shop pack item type not supported yet, not given");
      }
    }
  }
  wallet.save(&transaction, session.user_id).await?;

  #[rustfmt::skip]
  transaction.execute(/* language=postgresql */ r#"
    insert into user_shop_purchases (user_id, shop_item_id, num) values ($1, $2, $3)
  "#, &[&session.user_id, &params.shop_item_master_id, &count]).await.context("failed to save purchase")?;
  transaction.commit().await.context("failed to commit transaction")?;
  info!(?params, ?spent, stamina = wallet.stamina, "shop item bought");

  let mut response = CallResponse::new_success(Box::new(totals(&wallet, spent)));
  response.remote.extend(remote);
  response.remote.extend(wallet.to_remote_data());
  Ok(response)
}

/// How many of a shop item the user bought in the current limit period: today (Japan time, like the
/// original server) for daily limits, ever for the rest.
async fn bought_in_period(
  transaction: &deadpool_postgres::Transaction<'_>,
  user_id: UserId,
  item: &Value,
) -> anyhow::Result<i32> {
  let shop_item_id: i32 = item["id"].as_str().and_then(|id| id.parse().ok()).unwrap_or(0);
  let daily = item["limit_type"].as_str() == Some("DAILYLIMIT");
  #[rustfmt::skip]
  let row = transaction.query_one(/* language=postgresql */ r#"
    select coalesce(sum(num), 0)::integer
    from user_shop_purchases
    where user_id = $1 and shop_item_id = $2
      and (not $3 or (purchased_at at time zone 'Asia/Tokyo')::date = (now() at time zone 'Asia/Tokyo')::date)
  "#, &[&user_id, &shop_item_id, &daily]).await.context("failed to count purchases")?;
  Ok(row.get(0))
}
