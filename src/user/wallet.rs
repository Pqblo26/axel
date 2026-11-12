//! Currencies the client keeps as a single number: eris, paid and free quartz, and stamina.
//! They live in `user_wallet` (see migration 20261112000004).

use crate::api::master_all::get_master_manager;
use crate::api::{RemoteData, RemoteDataCommand, RemoteDataItemType};
use crate::database::QueryExecutor;
use crate::user::id::UserId;
use anyhow::Context;
use chrono::{DateTime, Duration, Utc};
use std::sync::OnceLock;

/// One stamina point every 3 minutes, as in the original game.
pub const STAMINA_RECOVER_SECONDS: i64 = 180;

/// Maximum stamina for a player rank, from the "exp" master (10 at rank 1, 200 from rank 41).
pub fn stamina_max(rank: i32) -> i32 {
  static MAX: OnceLock<Vec<(i32, i32)>> = OnceLock::new();
  let table = MAX.get_or_init(|| {
    get_master_manager()
      .get_master("exp")
      .iter()
      .filter_map(|row| {
        Some((
          row["rank"].as_str()?.parse().ok()?,
          row["stamina"].as_str()?.parse().ok()?,
        ))
      })
      .collect()
  });
  table
    .iter()
    .filter(|(row_rank, _)| *row_rank <= rank)
    .max_by_key(|(row_rank, _)| *row_rank)
    .map(|(_, stamina)| *stamina)
    .unwrap_or(200)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wallet {
  pub money: i64,
  pub real_money: i32,
  pub real_money_free: i32,
  pub stamina: i32,
  pub stamina_updated_at: DateTime<Utc>,
  /// Player rank (from user_progress, not saved here), sets the maximum stamina
  pub rank: i32,
}

/// Something could not be paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotEnough {
  pub kind: RemoteDataItemType,
  pub owned: i64,
  pub needed: i64,
}

impl std::fmt::Display for NotEnough {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "not enough {:?}: owned {}, needed {}",
      self.kind, self.owned, self.needed
    )
  }
}

impl std::error::Error for NotEnough {}

impl Wallet {
  /// Reads the wallet, creating it with the default amounts the first time, and applies stamina
  /// recovery. Inside a transaction the row stays locked until commit.
  pub async fn load<'a>(executor: impl Into<QueryExecutor<'a>>, user_id: UserId) -> anyhow::Result<Self> {
    let executor = executor.into();
    let client = executor.client();
    #[rustfmt::skip]
    client.execute(/* language=postgresql */ r#"
      insert into user_wallet (user_id) values ($1) on conflict (user_id) do nothing
    "#, &[&user_id]).await.context("failed to create wallet")?;
    #[rustfmt::skip]
    let row = client.query_one(/* language=postgresql */ r#"
      select w.money, w.real_money, w.real_money_free, w.stamina, w.stamina_updated_at, coalesce(p.rank, 1)
      from user_wallet w
        left join user_progress p on p.user_id = w.user_id
      where w.user_id = $1
      for update of w
    "#, &[&user_id]).await.context("failed to read wallet")?;

    let mut wallet = Self {
      money: row.get(0),
      real_money: row.get(1),
      real_money_free: row.get(2),
      stamina: row.get(3),
      stamina_updated_at: row.get(4),
      rank: row.get(5),
    };
    wallet.recover_stamina(Utc::now());
    Ok(wallet)
  }

  pub async fn save<'a>(&self, executor: impl Into<QueryExecutor<'a>>, user_id: UserId) -> anyhow::Result<()> {
    let executor = executor.into();
    #[rustfmt::skip]
    executor.client().execute(/* language=postgresql */ r#"
      update user_wallet
      set money = $2, real_money = $3, real_money_free = $4, stamina = $5, stamina_updated_at = $6
      where user_id = $1
    "#, &[
      &user_id,
      &self.money,
      &self.real_money,
      &self.real_money_free,
      &self.stamina,
      &self.stamina_updated_at,
    ]).await.context("failed to save wallet")?;
    Ok(())
  }

  /// Adds the stamina recovered since the last update, up to the maximum for the player rank.
  /// Stamina above the maximum (from items or purchases) is kept but does not recover.
  fn recover_stamina(&mut self, now: DateTime<Utc>) {
    self.recover_stamina_up_to(now, self.stamina_max());
  }

  fn recover_stamina_up_to(&mut self, now: DateTime<Utc>, max: i32) {
    if self.stamina >= max {
      self.stamina_updated_at = now;
      return;
    }
    let elapsed = (now - self.stamina_updated_at).num_seconds().max(0);
    let recovered = elapsed / STAMINA_RECOVER_SECONDS;
    if self.stamina as i64 + recovered >= max as i64 {
      self.stamina = max;
      self.stamina_updated_at = now;
    } else {
      self.stamina += recovered as i32;
      self.stamina_updated_at += Duration::seconds(recovered * STAMINA_RECOVER_SECONDS);
    }
  }

  pub fn stamina_max(&self) -> i32 {
    stamina_max(self.rank)
  }

  /// After a rank up: stamina is refilled with the new maximum, on top of what is left, like the
  /// original game.
  pub fn rank_up(&mut self, new_rank: i32) {
    self.recover_stamina(Utc::now());
    self.rank = new_rank;
    self.stamina = (self.stamina as i64 + self.stamina_max() as i64).min(i32::MAX as i64) as i32;
    self.stamina_updated_at = Utc::now();
  }

  pub fn quartz(&self) -> i64 {
    self.real_money as i64 + self.real_money_free as i64
  }

  pub fn get(&self, kind: RemoteDataItemType) -> Option<i64> {
    match kind {
      RemoteDataItemType::Money => Some(self.money),
      RemoteDataItemType::RealMoney => Some(self.real_money as i64),
      RemoteDataItemType::RealMoneyFree => Some(self.real_money_free as i64),
      RemoteDataItemType::Stamina => Some(self.stamina as i64),
      _ => None,
    }
  }

  /// Adds (or with a negative amount, takes) a currency. Fails without changing anything if the
  /// result would be negative. Returns false for item types that are not part of the wallet.
  pub fn add(&mut self, kind: RemoteDataItemType, amount: i64) -> Result<bool, NotEnough> {
    let Some(owned) = self.get(kind) else {
      return Ok(false);
    };
    let new = owned + amount;
    if new < 0 {
      return Err(NotEnough {
        kind,
        owned,
        needed: -amount,
      });
    }
    let clamp = |value: i64| value.min(i32::MAX as i64) as i32;
    match kind {
      RemoteDataItemType::Money => self.money = new,
      RemoteDataItemType::RealMoney => self.real_money = clamp(new),
      RemoteDataItemType::RealMoneyFree => self.real_money_free = clamp(new),
      RemoteDataItemType::Stamina => {
        let max = self.stamina_max();
        if owned >= max as i64 && new < max as i64 {
          // Recovery starts now that stamina dropped below the maximum
          self.stamina_updated_at = Utc::now();
        }
        self.stamina = clamp(new)
      }
      _ => unreachable!(),
    }
    Ok(true)
  }

  /// Spends quartz, free quartz first. Returns how much (free, paid) was spent.
  pub fn spend_quartz(&mut self, amount: i32) -> Result<(i32, i32), NotEnough> {
    if self.quartz() < amount as i64 {
      return Err(NotEnough {
        kind: RemoteDataItemType::RealMoneyFree,
        owned: self.quartz(),
        needed: amount as i64,
      });
    }
    let free = amount.min(self.real_money_free);
    let paid = amount - free;
    self.real_money_free -= free;
    self.real_money -= paid;
    Ok((free, paid))
  }

  /// Remote data that sets the four values on the client, whatever it had before.
  /// Used at login (after the client params are cleared) and after every change.
  pub fn to_remote_data(&self) -> Vec<RemoteData> {
    [
      (RemoteDataItemType::Money, self.money.min(i32::MAX as i64) as i32),
      (RemoteDataItemType::RealMoney, self.real_money),
      (RemoteDataItemType::RealMoneyFree, self.real_money_free),
      (RemoteDataItemType::Stamina, self.stamina),
    ]
    .into_iter()
    .flat_map(|(kind, value)| set_singleton(kind, value))
    .collect()
  }
}

/// Sets a single-number value on the client: removes what it had (delete clamps at zero) and adds
/// the new amount, the same trick [crate::item::CountedItem] uses for items.
pub fn set_singleton(kind: RemoteDataItemType, value: i32) -> Vec<RemoteData> {
  let make = |cmd: RemoteDataCommand, item_num: i32| RemoteData {
    cmd: cmd as i32,
    uid: None,
    item_type: kind.into(),
    item_id: 0,
    item_num,
    uniqid: 0,
    lv: 0,
    tag: String::from("-"),
    member_parameter: None,
    character_parameter: None,
    is_trial: None,
  };
  vec![
    make(RemoteDataCommand::UserParamDelete, i32::MAX),
    make(RemoteDataCommand::UserParamAdd, value),
  ]
}

#[cfg(test)]
mod tests {
  use super::*;

  fn wallet(stamina: i32, updated_secs_ago: i64, now: DateTime<Utc>) -> Wallet {
    Wallet {
      money: 0,
      real_money: 10,
      real_money_free: 5,
      stamina,
      stamina_updated_at: now - Duration::seconds(updated_secs_ago),
      rank: 45,
    }
  }

  #[test]
  fn spends_free_quartz_first() {
    let now = Utc::now();
    let mut w = wallet(0, 0, now);
    assert_eq!(w.spend_quartz(7), Ok((5, 2)));
    assert_eq!((w.real_money_free, w.real_money), (0, 8));
    assert!(w.spend_quartz(9).is_err());
    assert_eq!((w.real_money_free, w.real_money), (0, 8));
  }

  #[test]
  fn recovers_stamina_over_time() {
    let now = Utc::now();
    let mut w = wallet(10, 7 * 60, now);
    w.recover_stamina_up_to(now, 200);
    // 7 minutes: 2 points, and the minute left over keeps counting
    assert_eq!(w.stamina, 12);
    assert_eq!(w.stamina_updated_at, now - Duration::seconds(60));

    let mut w = wallet(199, 3600, now);
    w.recover_stamina_up_to(now, 200);
    assert_eq!((w.stamina, w.stamina_updated_at), (200, now));

    let mut w = wallet(419, 3600, now);
    w.recover_stamina_up_to(now, 200);
    assert_eq!(w.stamina, 419);
  }

  #[test]
  fn add_refuses_negative_result() {
    let now = Utc::now();
    let mut w = wallet(3, 0, now);
    assert!(w.add(RemoteDataItemType::Money, -1).is_err());
    assert_eq!(w.add(RemoteDataItemType::Money, 5), Ok(true));
    assert_eq!(w.add(RemoteDataItemType::SkipTicket, 5), Ok(false));
    assert_eq!(w.money, 5);
  }
}
