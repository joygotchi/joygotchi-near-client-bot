use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{U128, U64};
use near_sdk::{near, AccountId};

/// Credits use 6 decimals, so one credit (US$1 of NEAR AI compute) is 1_000_000.
pub const CREDIT_DECIMALS: u8 = 6;
pub const ONE_CREDIT: u128 = 1_000_000;

/// 30 days, in seconds. One "epoch" of the credit budget.
pub const EPOCH_SECONDS: u64 = 30 * 24 * 60 * 60;

pub const NANOS_PER_SECOND: u64 = 1_000_000_000;

/// Fixed-point scale for the reward accumulator.
pub const ACC_PRECISION: u128 = 1_000_000_000_000;

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
    /// Base staking weight. A legendary pet earns 4x a common one.
    pub fn base_weight(self) -> u128 {
        match self {
            Rarity::Common => 100,
            Rarity::Rare => 160,
            Rarity::Epic => 260,
            Rarity::Legendary => 400,
        }
    }
}

/// The pet attributes that matter for staking weight.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct PetAttrs {
    pub rarity: Rarity,
    pub level: u32,
    pub star: u32,
    pub evolution_phase: u8,
}

impl PetAttrs {
    /// `weight = base(rarity) x (1 + level/20) x (1 + star x 0.15) x evolution`
    ///
    /// Computed in basis points to stay integral. Growth is sub-linear on
    /// purpose: one maxed pet must not be able to crowd out a hundred casual
    /// players, or the credit stream stops feeling worth playing for.
    pub fn weight(&self) -> u128 {
        let base = self.rarity.base_weight();

        // (1 + level/20), in basis points.
        let level_bps = 10_000u128 + (self.level as u128) * 500;
        // (1 + star * 0.15), in basis points.
        let star_bps = 10_000u128 + (self.star as u128) * 1_500;
        // Evolution multiplier, in basis points.
        let evo_bps: u128 = match self.evolution_phase {
            0 | 1 => 10_000,
            2 => 11_500,
            3 => 13_500,
            _ => 16_000,
        };

        base * level_bps / 10_000 * star_bps / 10_000 * evo_bps / 10_000
    }
}

/// Credit-item ids. The game contract's own $JOY shop uses 1-4, so these start
/// at 101 to keep the two namespaces apart in events and logs.
pub const ITEM_TONIC: u16 = 101;
pub const ITEM_WARD: u16 = 102;
pub const ITEM_LENS: u16 = 103;
pub const ITEM_CHARM: u16 = 104;

pub const WARD_SECONDS: u64 = 24 * 60 * 60;
pub const WOUND_SECONDS: u64 = 6 * 60 * 60;
pub const LENS_FEEDS: u8 = 10;

/// The most XP one AI feed can ever be worth.
///
/// `feedXp` in src/lib/economy/params.ts is `35 x (1 + match*0.8 + lens*0.25)`,
/// which peaks at 72 for a perfect match with a Focus Lens. The cap is that
/// peak and nothing more, so the number needs no separate justification: one
/// call to `award_feed_xp` can be worth at most one perfect feed. The game
/// contract's own clamp is 500 — wide enough for a compromised backend to take
/// a pet from level 0 to level 5 in five calls, which is the gap this closes.
pub const MAX_FEED_XP: u32 = 72;

/// How long between mining claims.
pub const MINING_COOLDOWN_SECONDS: u64 = 6 * 60 * 60;

/// Every item that can drop, in ascending order of value.
pub const DROPPABLE_ITEMS: [u16; 4] = [ITEM_TONIC, ITEM_CHARM, ITEM_LENS, ITEM_WARD];

/// Floor drop chance, in basis points, before pet weight is counted.
pub const DROP_BASE_BPS: u32 = 200;
/// Ceiling, so a maxed pet is lucky rather than a printing press.
pub const DROP_MAX_BPS: u32 = 2_500;
/// Weight needed for one extra basis point of drop chance.
///
/// One, so the curve actually spans the weights pets reach: a fresh common is
/// weight 100 and a maxed legendary 2,240, giving 3% and 24.4%. At `20` the
/// whole range fit between 2.05% and 3.1% and the ceiling was unreachable —
/// "stronger pets find more" was true only on paper.
pub const DROP_WEIGHT_PER_BPS: u128 = 1;

/// A stronger pet finds better things, not just more of them.
///
/// Returns the chance in basis points and the highest item tier the pet can
/// roll, as an index into `DROPPABLE_ITEMS`.
pub fn drop_odds(weight: u128) -> (u32, usize) {
    if weight == 0 {
        return (0, 0);
    }

    let bonus = (weight / DROP_WEIGHT_PER_BPS) as u32;
    let chance = (DROP_BASE_BPS + bonus).min(DROP_MAX_BPS);

    // Tiers unlock with weight: 400 is a fresh legendary, 2240 a maxed one.
    let tier = match weight {
        0..=199 => 0,
        200..=599 => 1,
        600..=1_399 => 2,
        _ => 3,
    };

    (chance, tier)
}

/// An item offered for sale between players.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Listing {
    pub id: U64,
    pub seller_id: AccountId,
    pub item_id: u16,
    pub quantity: u32,
    /// Price of one item, in micro-credits.
    pub price_each: U128,
    pub created_at: U64,
}

/// Per-pet state that credit items and battles write.
///
/// Deliberately kept out of the weight calculation. The credit accumulator is
/// exact, and a weight that changes on a deadline would need the pool updated
/// at a moment when nothing is calling it — so every effect here gates
/// *gameplay* instead of altering the stream.
#[derive(BorshSerialize, BorshDeserialize, Default, Clone)]
#[borsh(crate = "near_sdk::borsh")]
pub struct PetState {
    /// Cannot battle until this timestamp.
    pub wounded_until: u64,
    /// Cannot be attacked until this timestamp.
    pub shielded_until: u64,
    /// AI feeds still covered by a Focus Lens.
    pub lens_feeds_left: u8,
    /// Keeps the pet's star on its next loss.
    pub star_guard: bool,
}

#[near(serializers = [json])]
pub struct PetStateView {
    pub pet_id: u64,
    pub wounded_until: U64,
    pub shielded_until: U64,
    pub lens_feeds_left: u8,
    pub star_guard: bool,
    pub staked: bool,
    /// False while staked, warded or wounded.
    pub attackable: bool,
}

#[near(serializers = [json])]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BattleOutcome {
    /// Credits moved from the loser to the winner.
    pub transferred: U128,
    /// Credits taken as rake.
    pub rake: U128,
    /// True when a Star Charm absorbed the loss — the game contract should
    /// leave the pet's star alone.
    pub star_protected: bool,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct StakedPet {
    pub pet_id: u64,
    pub owner_id: AccountId,
    pub weight: U128,
    pub attrs: PetAttrs,
    pub staked_at: U64,
}

#[derive(BorshSerialize, BorshDeserialize, Default)]
#[borsh(crate = "near_sdk::borsh")]
pub struct Account {
    pub weight: u128,
    /// Credits claimed and ready to spend.
    pub credit_balance: u128,
    /// Credits accrued but not yet claimed.
    pub pending: u128,
    /// `acc_credit_per_weight * weight / ACC_PRECISION` at the last harvest.
    pub reward_debt: u128,
    /// Daily auto-feed allowance, in micro-credits. 0 disables it.
    pub auto_feed_budget: u128,
    pub last_claim_at: u64,
    pub last_faucet_at: u64,
    pub last_mining_at: u64,
    /// Items dropped or bought and not yet used, keyed by item id.
    pub items: Vec<(u16, u32)>,
    pub staked: Vec<u64>,
}

/// What `get_account` returns.
#[near(serializers = [json])]
pub struct AccountView {
    pub account_id: AccountId,
    pub staked_pets: Vec<u64>,
    pub weight: U128,
    pub credit_balance: U128,
    pub pending_credits: U128,
    pub auto_feed_budget: U128,
    pub last_claim_at: U64,
    /// Unused items, as (item_id, count).
    pub items: Vec<(u16, u32)>,
}

/// What `get_pool` returns.
#[near(serializers = [json])]
pub struct PoolView {
    pub total_weight: U128,
    /// Micro-credits streamed per 30-day epoch.
    pub epoch_budget: U128,
    /// Micro-credits per second, derived from `epoch_budget`.
    pub rate_per_second: U128,
    pub staked_pets: u64,
    pub total_distributed: U128,
    pub total_spent: U128,
    /// Credits the treasury has set aside for $JOY conversions and the faucet.
    pub sale_reserve: U128,
    pub updated_at: U64,
    pub paused: bool,
}
