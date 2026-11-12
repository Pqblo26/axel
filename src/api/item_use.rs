//! Using items from the inventory. So far only stamina items (meat).

use crate::api::master_all::get_master_manager;
use crate::api::RemoteDataItemType;
use crate::blob::IntoRemoteData;
use crate::call::CallResponse;
use crate::extractor::Params;
use crate::handler::{IntoHandlerResponse, Unsigned};
use crate::item::UpdateItemCountBy;
use crate::user::session::Session;
use crate::user::wallet::Wallet;
use crate::AppState;
use anyhow::Context;
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn};

// num=1
// item_type=29
// item_id=1
#[derive(Debug, Deserialize)]
pub struct ItemUseRequest {
  pub num: i32,
  pub item_type: i32,
  pub item_id: i64,
}

/// Stamina restored by one stamina item, from the "stamina_item" master (id 1: 100, id 2: 10).
fn stamina_per_item(item_id: i64) -> Option<i32> {
  get_master_manager()
    .get_master("stamina_item")
    .iter()
    .find(|row| row["id"].as_str() == Some(&item_id.to_string()))
    .and_then(|row| row["stamina"].as_str()?.parse().ok())
}

pub async fn item_use(
  state: Arc<AppState>,
  session: Arc<Session>,
  Params(params): Params<ItemUseRequest>,
) -> impl IntoHandlerResponse {
  let empty = || Ok(Unsigned(CallResponse::new_success_empty()));
  let kind = RemoteDataItemType::from(params.item_type);
  let num = params.num.max(0);
  if kind != RemoteDataItemType::StaminaItem || num == 0 {
    warn!(?params, "using this item is not supported yet");
    return empty();
  }
  let Some(stamina) = stamina_per_item(params.item_id) else {
    warn!(?params, "unknown stamina item");
    return empty();
  };

  let mut client = state.get_database_client().await?;
  let transaction = client.transaction().await.context("failed to start transaction")?;
  #[rustfmt::skip]
  let owned: i32 = transaction.query_opt(/* language=postgresql */ r#"
    select quantity from user_items where user_id = $1 and item_type = $2 and item_id = $3
  "#, &[&session.user_id, &(params.item_type as i64), &params.item_id]).await?
    .map(|row| row.get(0))
    .unwrap_or(0);
  if owned < num {
    warn!(?params, ?owned, "not enough stamina items");
    return empty();
  }
  let item = UpdateItemCountBy::new(&transaction)
    .await?
    .run(session.user_id, (kind, params.item_id), -num)
    .await?;
  let mut wallet = Wallet::load(&transaction, session.user_id).await?;
  wallet.add(RemoteDataItemType::Stamina, stamina as i64 * num as i64)?;
  wallet.save(&transaction, session.user_id).await?;
  transaction.commit().await.context("failed to commit transaction")?;
  info!(?params, stamina = wallet.stamina, left = item.quantity, "stamina items used");

  let mut response = CallResponse::new_success_empty();
  response.remote.extend(item.into_remote_data());
  response.remote.extend(wallet.to_remote_data());
  Ok(Unsigned(response))
}
