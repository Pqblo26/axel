//! Player rank and experience, stored in `user_progress` (see migration 20261112000005).

use crate::api::master_all::get_master_manager;
use crate::api::{RemoteData, RemoteDataItemType};
use crate::database::QueryExecutor;
use crate::user::id::UserId;
use crate::blob::{AddSingletonItem, IntoRemoteData};
use anyhow::Context;
use std::sync::OnceLock;

/// (rank, total exp needed to reach it), sorted by rank, from the "exp" master.
fn rank_table() -> &'static [(i32, i32)] {
  static TABLE: OnceLock<Vec<(i32, i32)>> = OnceLock::new();
  TABLE.get_or_init(|| {
    let mut table: Vec<(i32, i32)> = get_master_manager()
      .get_master("exp")
      .iter()
      .filter_map(|row| {
        Some((
          row["rank"].as_str()?.parse().ok()?,
          row["allexp"].as_str()?.parse().ok()?,
        ))
      })
      .collect();
    table.sort_unstable();
    table
  })
}

/// Player rank that unlocks a part of the game (master `content_release`, e.g. "SUBPARTY",
/// "BACKPARTY", "DUNGEON"). 0 if it is not locked by rank.
pub fn content_unlock_rank(content: &str) -> i32 {
  get_master_manager()
    .get_master("content_release")
    .iter()
    .find(|row| row["content"].as_str() == Some(content))
    .and_then(|row| row["player_rank"].as_str()?.parse().ok())
    .unwrap_or(0)
}

pub fn max_rank() -> i32 {
  rank_table().last().map(|(rank, _)| *rank).unwrap_or(1)
}

/// Rank reached with a total amount of experience.
pub fn rank_for_exp(exp: i32) -> i32 {
  rank_for_exp_in(rank_table(), exp)
}

fn rank_for_exp_in(table: &[(i32, i32)], exp: i32) -> i32 {
  table
    .iter()
    .take_while(|(_, needed)| *needed <= exp)
    .last()
    .map(|(rank, _)| *rank)
    .unwrap_or(1)
}

/// Total experience needed for a rank.
pub fn exp_for_rank(rank: i32) -> i32 {
  rank_table()
    .iter()
    .find(|(row_rank, _)| *row_rank == rank)
    .map(|(_, exp)| *exp)
    .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerProgress {
  pub rank: i32,
  pub exp: i32,
}

impl PlayerProgress {
  /// Reads the player's rank, creating it at rank 1 the first time.
  pub async fn load<'a>(executor: impl Into<QueryExecutor<'a>>, user_id: UserId) -> anyhow::Result<Self> {
    let executor = executor.into();
    let client = executor.client();
    #[rustfmt::skip]
    client.execute(/* language=postgresql */ r#"
      insert into user_progress (user_id) values ($1) on conflict (user_id) do nothing
    "#, &[&user_id]).await.context("failed to create player progress")?;
    #[rustfmt::skip]
    let row = client.query_one(/* language=postgresql */ r#"
      select rank, exp from user_progress where user_id = $1 for update
    "#, &[&user_id]).await.context("failed to read player progress")?;
    Ok(Self {
      rank: row.get(0),
      exp: row.get(1),
    })
  }

  pub async fn save<'a>(&self, executor: impl Into<QueryExecutor<'a>>, user_id: UserId) -> anyhow::Result<()> {
    let executor = executor.into();
    #[rustfmt::skip]
    executor.client().execute(/* language=postgresql */ r#"
      update user_progress set rank = $2, exp = $3 where user_id = $1
    "#, &[&user_id, &self.rank, &self.exp]).await.context("failed to save player progress")?;
    Ok(())
  }

  /// Adds experience. Returns how many ranks were gained.
  pub fn add_exp(&mut self, amount: i32) -> i32 {
    let max_exp = exp_for_rank(max_rank());
    self.exp = (self.exp + amount.max(0)).min(max_exp.max(self.exp));
    let new_rank = rank_for_exp(self.exp).max(self.rank);
    let gained = new_rank - self.rank;
    self.rank = new_rank;
    gained
  }

  /// Remote data that brings the client's rank and experience from `before` to these values.
  ///
  /// Only what was gained is added: setting them absolutely (delete everything, then add) makes
  /// the client go through rank 0 on every result, so the result screen showed "Player Rank 1" and
  /// replayed the tutorial of every feature unlocked by rank.
  pub fn remote_since(&self, before: &PlayerProgress) -> Vec<RemoteData> {
    let mut remote = Vec::new();
    if self.rank > before.rank {
      remote.extend(AddSingletonItem::new(RemoteDataItemType::Level, self.rank - before.rank).into_remote_data());
    }
    if self.exp > before.exp {
      remote.extend(AddSingletonItem::new(RemoteDataItemType::Exp, self.exp - before.exp).into_remote_data());
    }
    remote
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ranks_from_exp() {
    let table = [(1, 0), (2, 5), (3, 13), (4, 25)];
    assert_eq!(rank_for_exp_in(&table, 0), 1);
    assert_eq!(rank_for_exp_in(&table, 4), 1);
    assert_eq!(rank_for_exp_in(&table, 5), 2);
    assert_eq!(rank_for_exp_in(&table, 24), 3);
    assert_eq!(rank_for_exp_in(&table, 1000), 4);
  }
}
