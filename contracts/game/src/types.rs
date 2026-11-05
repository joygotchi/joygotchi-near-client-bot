use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{U128, U64};
use near_sdk::{near, AccountId};

pub const NANOS_PER_SECOND: u64 = 1_000_000_000;

/// How long a fresh pet lives before it starves.
pub const STARVE_SECONDS: u64 = 24 * 60 * 60;
/// Ceiling on banked food, so a whale cannot make a pet immortal in one go.
pub const MAX_STARVE_SECONDS: u64 = 7 * 24 * 60 * 60;

/// A pet cannot attack again until this has passed.
pub const ATTACK_COOLDOWN_SECONDS: u64 = 60 * 60;
/// A pet cannot be attacked again until this has passed.
pub const ATTACKED_COOLDOWN_SECONDS: u64 = 60 * 60;

/// XP needed to reach level 1 from 0. Each level costs 18% more.
pub const BASE_LEVEL_XP: u32 = 100;
pub const MAX_LEVEL: u32 = 20;
pub const MAX_STAR: u32 = 5;

/// Consecutive wins that earn a star.
///
/// Stars are the PvP ladder: they are the one reward battling gives that
/// staking cannot, and each one is +15% staking weight, so winning fights
/// feeds the credit stream. They are also the reason a Star Charm is worth
/// buying — before this there was no way to gain a star at all, so losing one
/// was impossible and the charm protected nothing.
pub const WINS_PER_STAR: u32 = 3;
pub const MAX_EVOLUTION_PHASE: u8 = 4;

/// Mirrors `pet-stake::Rarity`. The lowercase serde names are part of the
/// cross-contract interface, not a style choice.
#[near(serializers = [json, borsh])]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Common,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    /// Base ATK and DEF. The old contract had no combat stats at all — the
    /// client hard-coded 100/100 for every pet — so these are new.
    pub fn base_stats(self) -> (u32, u32) {
        match self {
            Rarity::Common => (100, 100),
            Rarity::Rare => (130, 125),
            Rarity::Epic => (170, 160),
            Rarity::Legendary => (220, 200),
        }
    }

    /// Mint odds in basis points, in order. Published so players can see them.
    pub const ODDS_BPS: [(Rarity, u32); 4] = [
        (Rarity::Common, 7_000),
        (Rarity::Rare, 2_000),
        (Rarity::Epic, 800),
        (Rarity::Legendary, 200),
    ];

    /// Roll a rarity from a uniform value in 0..10_000.
    pub fn from_roll(roll: u32) -> Rarity {
        let mut floor = 0;
        for (rarity, weight) in Self::ODDS_BPS {
            floor += weight;
            if roll < floor {
                return rarity;
            }
        }
        Rarity::Common
    }
}

/// Exactly what `pet-stake` deserialises from `get_pet_attrs`. Do not reorder
/// or rename these fields.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct PetAttrs {
    pub rarity: Rarity,
    pub level: u32,
    pub star: u32,
    pub evolution_phase: u8,
}

/// What `pet-stake::report_battle` returns.
#[near(serializers = [json])]
#[derive(Clone, Copy, Debug)]
pub struct BattleOutcome {
    pub transferred: U128,
    pub rake: U128,
    pub star_protected: bool,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub enum PetStatus {
    Happy,
    Hungry,
    Dying,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
#[borsh(crate = "near_sdk::borsh")]
pub struct Pet {
    pub pet_id: u64,
    pub owner_id: AccountId,
    pub name: String,
    pub category: String,
    pub rarity: Rarity,
    pub level: u32,
    pub star: u32,
    pub xp: u32,
    pub evolution_phase: u8,
    pub atk: u32,
    pub def: u32,
    /// Absolute timestamp, in nanoseconds, at which the pet starves.
    ///
    /// Named for what it is. The v1 field was `time_until_starving` and held
    /// the same absolute value, which read as a duration and had the client
    /// rendering a 5,460-year countdown.
    pub starves_at: u64,
    pub born_at: u64,
    pub last_attack_used: u64,
    pub last_attacked: u64,
    /// Blocks exactly one incoming attack, then is spent.
    pub shield: u32,
    /// Wins since the last defeat. At `WINS_PER_STAR` it converts to a star.
    pub win_streak: u32,
    /// True while `pet-stake` has it at work. Set only by that contract.
    pub locked: bool,
}

/// Default marketplace fee, in basis points. Goes to the treasury, which is
/// what gets staked with NEAR AI — so trading volume feeds the credit stream.
pub const DEFAULT_MARKET_FEE_BPS: u16 = 250;

impl Pet {
    pub fn attrs(&self) -> PetAttrs {
        PetAttrs {
            rarity: self.rarity,
            level: self.level,
            star: self.star,
            evolution_phase: self.evolution_phase,
        }
    }

    pub fn status(&self, now: u64) -> PetStatus {
        if now >= self.starves_at {
            return PetStatus::Dying;
        }
        let left = (self.starves_at - now) / NANOS_PER_SECOND;
        if left < 3 * 3_600 {
            PetStatus::Hungry
        } else {
            PetStatus::Happy
        }
    }

    pub fn is_alive(&self, now: u64) -> bool {
        now < self.starves_at
    }

    /// XP to go from `level` to `level + 1`: 100 × 1.18^level, in integers.
    pub fn xp_for_next_level(&self) -> u32 {
        let mut need = BASE_LEVEL_XP as u64;
        for _ in 0..self.level {
            need = need * 118 / 100;
        }
        need as u32
    }

    /// Effective attack. Level and star matter, but sub-linearly, so a high
    /// level cannot make a common pet unbeatable.
    pub fn effective_atk(&self) -> u64 {
        let base = self.atk as u64;
        base * (100 + self.level as u64 * 5) / 100 * (100 + self.star as u64 * 10) / 100
    }

    pub fn effective_def(&self) -> u64 {
        let base = self.def as u64;
        base * (100 + self.level as u64 * 4) / 100 * (100 + self.star as u64 * 10) / 100
    }
}

/// The JSON shape clients read. `starves_at` is absolute nanoseconds and
/// `status` is computed at read time rather than stored, which is what the v1
/// contract got wrong — 14 of its 20 pets reported HAPPY while long starved.
#[near(serializers = [json])]
pub struct PetView {
    pub pet_id: u64,
    pub owner_id: AccountId,
    pub name: String,
    pub category: String,
    pub rarity: Rarity,
    pub level: u32,
    pub star: u32,
    pub xp: u32,
    pub xp_for_next_level: u32,
    pub evolution_phase: u8,
    pub atk: u32,
    pub def: u32,
    pub effective_atk: U64,
    pub effective_def: U64,
    pub starves_at: U64,
    pub seconds_until_starving: U64,
    pub status: PetStatus,
    pub shield: u32,
    pub win_streak: u32,
    /// Wins still needed for the next star, or `null` at `MAX_STAR`.
    pub wins_to_next_star: Option<u32>,
    pub locked: bool,
    /// Asking price in yoctoNEAR while the pet is for sale.
    pub listed_price: Option<U128>,
    pub born_at: U64,
    pub last_attack_used: U64,
    pub last_attacked: U64,
}

/// A pet offered for sale.
///
/// Pets are priced in NEAR, not credits. Credits are what you earn by playing
/// and they are deliberately not transferable; a pet is an asset bought with
/// money, and giving it a resale price is what turns minting from a one-way
/// purchase into something a player can get out of.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct PetListing {
    pub pet_id: u64,
    pub seller_id: AccountId,
    /// Price in yoctoNEAR.
    pub price: U128,
    pub listed_at: U64,
}

/// An item in the game contract's own shop, priced in yoctoNEAR.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct ShopItem {
    pub item_id: u16,
    pub name: String,
    /// Price in yoctoNEAR. The v1 shop was priced in a token that never
    /// existed on-chain; NEAR is the only money now.
    pub price: U128,
    /// Seconds added to the starving deadline.
    pub time_extension: u64,
    /// XP granted.
    pub xp: u32,
    /// Incoming attacks blocked.
    pub shield: u32,
    /// Whether it can revive a starved pet.
    pub is_revival: bool,
}

/// One pet in a v1 migration payload.
#[near(serializers = [json])]
pub struct LegacyPet {
    pub pet_id: u64,
    pub owner_id: AccountId,
    pub name: String,
    pub category: String,
    pub level: u32,
    pub star: u32,
    pub pet_evolution_phase: u8,
    pub time_pet_born: U64,
    /// The v1 field: absolute nanoseconds despite the name.
    pub time_until_starving: U64,
}
