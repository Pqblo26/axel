use crate::api::battle::BattleParty;
use crate::api::dungeon::PartyMember;
use crate::api::party::PartyWire;
use crate::api::surprise::BasicBattlePartyForm;
use crate::call::CallCustom;
use crate::handler::{IntoHandlerResponse, Signed};
use crate::member::{FetchUserMemberSkillsIn, FetchUserMembers, FetchUserParties};
use crate::user::session::Session;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct PartyInfo {
  pub party: Vec<Party>,
  pub members: Vec<PartyMember>,
  pub weapons: Vec<()>,
  pub accessories: Vec<()>,
}

impl CallCustom for PartyInfo {}

// See [Wonder_Api_PartyinfoPartyResponseDto_Fields]
// See [Wonder_Api_PartychangePartyResponseDto_Fields]
// Thanks to https://youtu.be/Vv9r8wrDsZ8
#[derive(Debug, Serialize, Deserialize)]
pub struct Party {
  /// ## Party formations
  /// Front 1, Front 2, Front 3, Rear 1, Rear 2.
  /// Rear members enter battle after front member is down.
  pub party_forms: [PartyForm; 5],
  pub party_no: i32,
  /// Assist stats affect the stats of main party members.
  pub assist: i64,
  pub sub_assists: Vec<i64>,
  /// "Party Trait"
  pub party_passive_skill: PartyPassiveSkillInfo,
}

/// The special skill a member uses: its own if it has one (a different animation, master
/// `skill_sp` with its `member_id`), otherwise [current] if it belongs to the member's character,
/// otherwise the character's first one. 0 for an empty slot.
pub fn special_skill_for_member(member_id: i64, current: i32) -> i32 {
  if member_id == 0 {
    return 0;
  }
  let masters = crate::api::master_all::get_master_manager();
  let skills = masters.get_master("skill_sp");
  let int = |row: &serde_json::Value, key: &str| row[key].as_str().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
  let Some(character) = masters
    .get_master("member")
    .iter()
    .find(|row| int(row, "id") == member_id)
    .map(|row| int(row, "character_id"))
  else {
    return current;
  };
  let own = skills.iter().filter(|row| int(row, "member_id") == member_id).map(|row| int(row, "skill_id")).collect::<Vec<_>>();
  if !own.is_empty() {
    return if own.contains(&(current as i64)) { current } else { *own.iter().min().unwrap() as i32 };
  }
  let shared = |row: &&serde_json::Value| int(row, "character_id") == character && int(row, "member_id") == 0;
  if skills.iter().filter(shared).any(|row| int(row, "skill_id") == current as i64) {
    return current;
  }
  skills.iter().filter(shared).map(|row| int(row, "skill_id")).min().map_or(current, |skill| skill as i32)
}

impl Party {
  /// Every member in the party, main and sub members, without empty slots or repeats. The battle
  /// needs the data of all of them: a sub member missing from the battle members makes the game
  /// fail with "Data Load Failed".
  pub fn member_ids(&self) -> Vec<i64> {
    let mut ids = Vec::new();
    for form in &self.party_forms {
      for id in [form.main, form.sub1, form.sub2] {
        if id != 0 && !ids.contains(&(id as i64)) {
          ids.push(id as i64);
        }
      }
    }
    ids
  }

  /// Empties the slots the player's rank has not unlocked yet: sub members before rank 5 and the
  /// two rear formations before rank 8 (master `content_release`). The game hides them, so the
  /// battle must not use them either.
  pub fn restrict_to_rank(&mut self, rank: i32) {
    let subs_unlocked = rank >= crate::user::progress::content_unlock_rank("SUBPARTY");
    let rear_unlocked = rank >= crate::user::progress::content_unlock_rank("BACKPARTY");
    for (index, form) in self.party_forms.iter_mut().enumerate() {
      if index >= 3 && !rear_unlocked {
        form.main = 0;
        form.weapon = 0;
        form.acc = 0;
      }
      if (index >= 3 && !rear_unlocked) || !subs_unlocked {
        form.sub1 = 0;
        form.sub2 = 0;
      }
    }
  }

  /// Makes each formation's special skill one of its main member's. The team keeps the special
  /// skill of the slot when Suggest, Reset or a new account change the main member, and the game
  /// freezes when a character uses a special skill that is not theirs.
  pub fn fix_special_skills(&mut self) {
    for form in self.party_forms.iter_mut() {
      form.specialskill.special_skill_id = special_skill_for_member(form.main as i64, form.specialskill.special_skill_id);
    }
  }

  /// The formation a member is in, as main or sub member.
  pub fn form_of(&self, member_id: i32) -> Option<&PartyForm> {
    self
      .party_forms
      .iter()
      .find(|form| form.main == member_id)
      .or_else(|| self.party_forms.iter().find(|form| form.sub1 == member_id || form.sub2 == member_id))
  }

  pub fn new(party_forms: [PartyForm; 5], party_no: i32) -> Self {
    Self {
      party_forms,
      party_no,
      assist: 0,
      sub_assists: vec![],
      party_passive_skill: PartyPassiveSkillInfo {
        skill_id: 0,
        user_member_id: 0,
      },
    }
  }

  pub fn to_battle_party(&self) -> BattleParty {
    BattleParty {
      party_forms: self
        .party_forms
        .iter()
        .map(|ref form| form.to_basic_battle_party_form())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap(),
      assist: self.assist as i32,
      sub_assists: self.sub_assists.iter().map(|sub_assist| *sub_assist as i32).collect(),
      party_passive_skill: self.party_passive_skill.clone(),
    }
  }
}

// See [Wonder_Api_PartyinfoPartyFormResponseDto_Fields]
// extends [Wonder_Api_BasicPartyFormResponseDto_Fields]
#[derive(Debug, Serialize, Deserialize)]
pub struct PartyForm {
  /* Wonder_Api_BasicPartyFormResponseDto_Fields */
  pub id: i32,
  pub form_no: i32,
  pub main: i32,
  /// ## Sub-members
  /// 30% of sub-member stats and passive skills are applied to the main member.
  /// If sub-member and main member are the same character, main member gets additional 10% stat boost.
  pub sub1: i32,
  pub sub2: i32,
  pub weapon: i64,
  pub acc: i64,
  pub strength: i32,
  pub specialskill: SpecialSkillInfo,
  /// "Fame Trait"
  pub skill_pa_fame: i64,
  /* Wonder_Api_PartyinfoPartyFormResponseDto_Fields */
  pub party_no: i32,
  /// Must not be empty
  pub name: String,
}

impl PartyForm {
  pub fn new(id: i32, form_no: i32, party_no: i32, main: i32) -> Self {
    Self {
      id,
      form_no,
      main,
      sub1: 0,
      sub2: 0,
      weapon: 0,
      acc: 0,
      strength: 1,
      specialskill: SpecialSkillInfo {
        special_skill_id: 100001,
        trial: false,
      },
      skill_pa_fame: 0,
      party_no,
      name: format!("Party{}", party_no),
    }
  }

  pub fn to_basic_battle_party_form(&self) -> BasicBattlePartyForm {
    BasicBattlePartyForm {
      id: self.id,
      party_no: self.party_no,
      form_no: self.form_no,
      main: self.main,
      sub1: self.sub1,
      sub2: self.sub2,
      weapon: self.weapon as i32,
      acc: self.acc as i32,
      skill_pa_fame: self.skill_pa_fame,
    }
  }
}

// See [Wonder_Api_SpecialSkillInfoResponseDto_Fields]
/// ## Special skills
///
/// Client loads special skills from `skill_sp` master, keyed by `character_id`.
/// Skills are upgraded based on member's promotion level.
/// See `skill_sp_details.unlock_intimacy_lv`.
/// `skill_sp.pattern_number` can be 1 or 2.
#[derive(Debug, Serialize, Deserialize)]
pub struct SpecialSkillInfo {
  /// Must be non-zero if main member is set.
  ///
  /// CLIENT QUIRK: Client shows list of skills for the character *of the skill itself*,
  /// not the main character of the party form. For example, sending Kazuma's skill ID
  /// will show his skills in selection screen, even if the main character is Megumin.
  pub special_skill_id: i32,
  pub trial: bool,
}

// See [Wonder_Api_PartyPassiveSkillInfoResponseDto_Fields]
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PartyPassiveSkillInfo {
  pub skill_id: i64,
  // CLIENT QUIRK: Client displays trait of the member specified, ignoring [skill_id],
  //  even if member has Character Trait and not Party Trait.
  pub user_member_id: i64,
}

impl PartyPassiveSkillInfo {
  pub fn new(skill_id: i64, user_member_id: i64) -> Self {
    Self {
      skill_id,
      user_member_id,
    }
  }
}

impl Party {
  // See [Wonder.UI.Data.PartyData$$get_Force]
  pub fn power(&self, assist_level: u32) -> u32 {
    // See [Wonder.Data.MemberParameter$$GetStrength]
    let party_forms_strength: u32 = self.party_forms.iter().map(|form| form.strength as u32).sum();
    party_forms_strength + (4 * assist_level)
  }
}

pub async fn party_info(state: Arc<AppState>, session: Arc<Session>) -> impl IntoHandlerResponse {
  let client = state.get_database_client().await?;

  let mut members = FetchUserMembers::new(&client).await?.run(session.user_id).await?;
  FetchUserMemberSkillsIn::new(&client)
    .await?
    .run(session.user_id, &mut members.iter_mut().collect::<Vec<_>>())
    .await?;

  let mut parties = FetchUserParties::new(&client).await?.run(session.user_id).await?;
  // PWR shown in the team screen: main member strength plus its sub members
  let by_id = members.iter().map(|member| (member.id, member)).collect::<HashMap<_, _>>();
  for party in &mut parties {
    party.fix_special_skills();
    for form in party.party_forms.iter_mut() {
      let Some(main) = by_id.get(&form.main) else {
        form.strength = 0;
        continue;
      };
      let mut strength = main.main_strength_value();
      for sub_id in [form.sub1, form.sub2] {
        if let Some(sub) = by_id.get(&sub_id) {
          strength += sub.sub_strength_value(sub.prototype.character_id == main.prototype.character_id);
        }
      }
      form.strength = strength;
    }
  }

  Ok(Signed(
    PartyWire {
      party: parties,
      members: members.iter().map(|member| member.to_party_member()).collect(),
      weapons: vec![],
      accessories: vec![],
    },
    session,
  ))
}
