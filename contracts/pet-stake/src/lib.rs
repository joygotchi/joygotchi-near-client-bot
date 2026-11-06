//! Pet staking → NEAR AI credits.
//!
//! A player stakes a pet into this contract and the pet starts "working at the
//! AI lab": it stops being able to battle, it still has to be fed, and it earns
//! a share of the credit stream for as long as it is staked.
//!
//! The stream is *not* inflationary. The treasury stakes NEAR with NEAR AI
//! (stake-to-compute), NEAR AI converts that position into a monthly allowance
//! of compute credits, and the treasury publishes that allowance here as
//! `epoch_budget`. This contract only decides *who* gets which slice of an
//! allowance that already exists off-chain. Spending a credit really does spend
//! NEAR AI compute, which is why credits are an account balance here and never
//! a transferable token.
//!
//! The distribution maths is the usual accumulator ("MasterChef") pattern, the
//! same shape the game contract already uses for its `reward_debt` field.

mod types;

use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::{LookupMap, UnorderedMap};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde_json::json;
use near_sdk::{
    env, ext_contract, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault,
    Promise, PromiseError,
};

pub use types::*;

const GAS_FOR_METADATA: Gas = Gas::from_tgas(10);
const GAS_FOR_LOCK: Gas = Gas::from_tgas(10);
const GAS_FOR_RESOLVE: Gas = Gas::from_tgas(15);
/// `award_xp` can level the pet, which makes the game contract call back into
/// this one to re-sync the pet's weight, so this leaves room for two hops.
const GAS_FOR_AWARD_XP: Gas = Gas::from_tgas(30);

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Accounts,
    Pets,
    Prices,
    PetStates,
    ItemPrices,
    Listings,
}

/// The view this contract needs from the game contract. A deployment whose game
/// contract does not expose it can use `admin_set_pet` instead.
#[ext_contract(ext_game)]
#[allow(dead_code)]
trait Game {
    fn get_pet_attrs(&self, pet_id: u64) -> (AccountId, PetAttrs);

    /// Mirror the staked flag on the pet contract, so a pet at work here
    /// cannot be transferred there. A transfer has to be atomic, and a
    /// cross-contract view cannot be, so the flag has to live on both sides.
    fn set_locked(&mut self, pet_id: u64, locked: bool);

    /// Add XP to a pet, levelling it if that crosses the threshold. The game
    /// contract accepts this from the staking contract or its own owner only,
    /// which is why awarding XP has to come through here.
    fn award_xp(&mut self, pet_id: u64, xp: u32);
}

#[ext_contract(ext_self)]
#[allow(dead_code)]
trait SelfCallbacks {
    fn on_pet_attrs(&mut self, pet_id: u64, account_id: AccountId) -> bool;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct PetStake {
    owner_id: AccountId,
    /// Contract that owns the pets.
    game_contract: AccountId,
    /// The only account allowed to grant credits — the mint router, where all
    /// payments land. This contract itself never receives NEAR or any token.
    granter: AccountId,

    /// Micro-credits streamed per 30-day epoch, set by the treasury to match
    /// what NEAR AI actually grants for its staked NEAR.
    epoch_budget: u128,
    total_weight: u128,
    /// Micro-credits per unit of weight, scaled by `ACC_PRECISION`.
    acc_credit_per_weight: u128,
    updated_at: u64,

    accounts: UnorderedMap<AccountId, Account>,
    pets: UnorderedMap<u64, StakedPet>,
    /// Price of each credit sink, in micro-credits.
    prices: LookupMap<String, u128>,
    /// Per-pet item and battle state. Exists for unstaked pets too.
    pet_states: UnorderedMap<u64, PetState>,
    /// Price of each credit item, in micro-credits.
    item_prices: LookupMap<u16, u128>,
    /// Credits a challenger puts up to attack, in micro-credits.
    battle_wager: u128,
    /// Share of the pot taken as rake, in basis points.
    battle_rake_bps: u16,

    total_distributed: u128,
    total_spent: u128,

    /// Credits set aside for sale, mining and the faucet. Separate from the
    /// staking stream so selling credits can never overdraw the real NEAR AI
    /// balance that backs them.
    sale_reserve: u128,
    faucet_amount: u128,
    /// Credits paid by one mining claim.
    mining_reward: u128,

    /// Items offered for sale between players.
    listings: UnorderedMap<u64, Listing>,
    next_listing_id: u64,
    /// Marketplace cut, in basis points.
    market_rake_bps: u16,

    paused: bool,

    /// The one account allowed to report feed XP — the AI backend's key.
    ///
    /// Last in the struct on purpose: the previous layout's bytes are then a
    /// prefix of this one, which is what makes `migrate` below a pure append
    /// rather than a re-read of every field. See docs/GAME_CONTRACT_V2.md §7.
    oracle: AccountId,
}

#[near]
impl PetStake {
    /// `oracle` is the AI backend's account, the only one allowed to report
    /// feed XP. Left out it falls back to the owner and `set_oracle` names it
    /// later, because the backend's key usually does not exist yet when the
    /// three contracts go up.
    #[init]
    pub fn new(
        owner_id: AccountId,
        game_contract: AccountId,
        granter: AccountId,
        epoch_budget: U128,
        oracle: Option<AccountId>,
    ) -> Self {
        let mut contract = Self {
            oracle: oracle.unwrap_or_else(|| owner_id.clone()),
            owner_id,
            game_contract,
            granter,
            epoch_budget: epoch_budget.0,
            total_weight: 0,
            acc_credit_per_weight: 0,
            updated_at: env::block_timestamp(),
            accounts: UnorderedMap::new(StorageKey::Accounts),
            pets: UnorderedMap::new(StorageKey::Pets),
            prices: LookupMap::new(StorageKey::Prices),
            pet_states: UnorderedMap::new(StorageKey::PetStates),
            item_prices: LookupMap::new(StorageKey::ItemPrices),
            battle_wager: 10_000, // 0.01 credit
            battle_rake_bps: 500, // 5%
            total_distributed: 0,
            total_spent: 0,
            sale_reserve: 0,
            // One-off grant, not a daily one: NEAR implicit accounts are nearly
            // free to create, and 0.05/day per account would let a script with
            // a thousand accounts drain a $500 monthly budget three times over.
            faucet_amount: ONE_CREDIT / 100, // 0.01 credit, once per account
            mining_reward: ONE_CREDIT / 200, // 0.005 credit per claim
            listings: UnorderedMap::new(StorageKey::Listings),
            next_listing_id: 1,
            market_rake_bps: 500, // 5%
            paused: false,
        };

        // Keep these in step with src/lib/economy/params.ts.
        for (action, credits) in [
            ("ai_feed", 2_000u128),
            ("train", 8_000),
            ("evolve", 500_000),
            ("craft_item", 20_000),
            ("chat", 1_000),
            ("auto_feed", 15_000),
        ] {
            contract.prices.insert(&action.to_string(), &credits);
        }

        for (item, credits) in [
            (ITEM_TONIC, 10_000u128),
            (ITEM_WARD, 50_000),
            (ITEM_LENS, 20_000),
            (ITEM_CHARM, 15_000),
        ] {
            contract.item_prices.insert(&item, &credits);
        }

        contract
    }

    // ── Accrual ──────────────────────────────────────────────────────────────

    /// Nominal stream rate, for display only. Never used in the maths: dividing
    /// the budget by the epoch length first would throw away up to half a
    /// percent of the treasury's allowance to integer truncation.
    fn rate_per_second(&self) -> u128 {
        self.epoch_budget / EPOCH_SECONDS as u128
    }

    /// Micro-credits emitted over `elapsed` seconds, scaled by `ACC_PRECISION`.
    /// Multiplying before dividing keeps the full budget in play.
    fn scaled_emission(&self, elapsed: u128) -> u128 {
        self.epoch_budget * elapsed * ACC_PRECISION / EPOCH_SECONDS as u128
    }

    /// Move the accumulator forward to the current block.
    fn update_pool(&mut self) {
        let now = env::block_timestamp();
        if now <= self.updated_at {
            return;
        }
        if self.total_weight == 0 {
            self.updated_at = now;
            return;
        }

        let elapsed = ((now - self.updated_at) / NANOS_PER_SECOND) as u128;
        if elapsed == 0 {
            return;
        }

        let scaled = self.scaled_emission(elapsed);
        self.acc_credit_per_weight += scaled / self.total_weight;
        self.total_distributed += scaled / ACC_PRECISION;
        // Advance only by whole seconds so no fraction of a second is lost.
        self.updated_at += elapsed as u64 * NANOS_PER_SECOND;
    }

    fn accrued(&self, account: &Account) -> u128 {
        self.acc_credit_per_weight * account.weight / ACC_PRECISION
    }

    /// Fold everything earned since the last touch into `pending`.
    fn harvest(&self, account: &mut Account) {
        let accrued = self.accrued(account);
        account.pending += accrued.saturating_sub(account.reward_debt);
        account.reward_debt = accrued;
    }

    fn account_or_default(&self, account_id: &AccountId) -> Account {
        self.accounts.get(account_id).unwrap_or_default()
    }

    fn assert_running(&self) {
        require!(!self.paused, "The AI lab is paused");
    }

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only the owner can call this"
        );
    }

    fn log_event(&self, event: &str, data: near_sdk::serde_json::Value) {
        env::log_str(&format!(
            "EVENT_JSON:{}",
            json!({
                "standard": "joygotchi",
                "version": "1.0.0",
                "event": event,
                "data": [data],
            })
        ));
    }

    // ── Staking ──────────────────────────────────────────────────────────────

    /// Stake a pet. Reads the pet's owner and attributes from the game contract,
    /// so a player can only stake a pet they actually hold.
    ///
    /// Attach 1 yoctoNEAR: this is a confirmation-worthy action, and requiring a
    /// full-access signature stops a limited gameplay key from staking pets.
    #[payable]
    pub fn stake_pet(&mut self, pet_id: u64) -> Promise {
        self.assert_running();
        require!(
            env::attached_deposit() >= NearToken::from_yoctonear(1),
            "Attach 1 yoctoNEAR"
        );
        require!(self.pets.get(&pet_id).is_none(), "Pet is already staked");

        let account_id = env::predecessor_account_id();

        ext_game::ext(self.game_contract.clone())
            .with_static_gas(GAS_FOR_METADATA)
            .get_pet_attrs(pet_id)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE)
                    .on_pet_attrs(pet_id, account_id),
            )
    }

    #[private]
    pub fn on_pet_attrs(
        &mut self,
        pet_id: u64,
        account_id: AccountId,
        #[callback_result] result: Result<(AccountId, PetAttrs), PromiseError>,
    ) -> bool {
        let Ok((owner_id, attrs)) = result else {
            env::log_str("Could not read the pet from the game contract");
            return false;
        };

        if owner_id != account_id {
            env::log_str("That pet belongs to someone else");
            return false;
        }

        self.internal_stake(pet_id, account_id, attrs);
        true
    }

    /// Forget a pet's current stake record, taking its weight back out of both
    /// the account and the pool.
    ///
    /// `internal_stake` is reached twice for the same pet: once when it is
    /// staked, and again by `sync_pet` every time it levels up or evolves. The
    /// attributes alone cannot tell those apart, so the weight has to be
    /// *replaced*, never added. Adding it is what broke the pool on testnet —
    /// one level-up left the account carrying its pet's weight twice, so
    /// unstaking gave back only the newer half and `total_weight` kept paying
    /// out for a pet that had already left.
    fn drop_stake_record(&mut self, pet_id: u64, account: &mut Account) {
        let Some(existing) = self.pets.get(&pet_id) else {
            return;
        };
        account.weight = account.weight.saturating_sub(existing.weight.0);
        self.total_weight = self.total_weight.saturating_sub(existing.weight.0);
        account.staked.retain(|id| *id != pet_id);
        self.pets.remove(&pet_id);
    }

    fn internal_stake(&mut self, pet_id: u64, account_id: AccountId, attrs: PetAttrs) {
        self.update_pool();

        let weight = attrs.weight();
        let mut account = self.account_or_default(&account_id);
        // Harvest first: everything earned so far was earned at the old weight.
        self.harvest(&mut account);
        let resync = self.pets.get(&pet_id).is_some();
        self.drop_stake_record(pet_id, &mut account);

        account.weight += weight;
        account.staked.push(pet_id);
        account.reward_debt = self.accrued(&account);

        self.total_weight += weight;
        self.accounts.insert(&account_id, &account);
        self.pets.insert(
            &pet_id,
            &StakedPet {
                pet_id,
                owner_id: account_id.clone(),
                weight: U128(weight),
                attrs,
                staked_at: U64(env::block_timestamp()),
            },
        );

        // A re-weigh is not a new stake. Emitting `pet_staked` for both made an
        // indexer count the same pet twice.
        self.log_event(
            if resync { "pet_resynced" } else { "pet_staked" },
            json!({ "account_id": account_id, "pet_id": pet_id, "weight": U128(weight) }),
        );

        // Always re-assert the lock, even on a re-weigh: `set_pet_lock` is
        // fire-and-forget, so a lock that failed to land the first time has no
        // other way back, and `set_locked` is idempotent.
        self.set_pet_lock(pet_id, true);
    }

    /// Fire-and-forget: a failed lock leaves the pet transferable for a while,
    /// which the game contract's own checks still catch on the next read, and
    /// `admin_set_pet` can repair.
    fn set_pet_lock(&self, pet_id: u64, locked: bool) {
        ext_game::ext(self.game_contract.clone())
            .with_static_gas(GAS_FOR_LOCK)
            .set_locked(pet_id, locked)
            .detach();
    }

    /// Take a pet out of the lab. Credits earned so far stay in `pending`.
    #[payable]
    pub fn unstake_pet(&mut self, pet_id: u64) {
        require!(
            env::attached_deposit() >= NearToken::from_yoctonear(1),
            "Attach 1 yoctoNEAR"
        );

        let account_id = env::predecessor_account_id();
        let pet = self.pets.get(&pet_id).expect("Pet is not staked");
        require!(pet.owner_id == account_id, "That pet belongs to someone else");

        self.update_pool();

        let mut account = self.account_or_default(&account_id);
        self.harvest(&mut account);

        account.weight = account.weight.saturating_sub(pet.weight.0);
        account.staked.retain(|id| *id != pet_id);
        account.reward_debt = self.accrued(&account);

        self.total_weight = self.total_weight.saturating_sub(pet.weight.0);
        self.accounts.insert(&account_id, &account);
        self.pets.remove(&pet_id);

        self.log_event(
            "pet_unstaked",
            json!({ "account_id": account_id, "pet_id": pet_id, "weight": pet.weight }),
        );

        self.set_pet_lock(pet_id, false);
    }

    /// Re-read a staked pet's attributes after it levels up or evolves, so its
    /// weight reflects what it has become.
    pub fn sync_pet(&mut self, pet_id: u64) -> Promise {
        let pet = self.pets.get(&pet_id).expect("Pet is not staked");
        ext_game::ext(self.game_contract.clone())
            .with_static_gas(GAS_FOR_METADATA)
            .get_pet_attrs(pet_id)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE)
                    .on_pet_attrs(pet_id, pet.owner_id),
            )
    }

    // ── Credits ──────────────────────────────────────────────────────────────

    /// Move accrued credits into the spendable balance.
    pub fn claim_credits(&mut self) -> U128 {
        self.assert_running();
        self.update_pool();

        let account_id = env::predecessor_account_id();
        let mut account = self.account_or_default(&account_id);
        self.harvest(&mut account);

        let claimed = account.pending;
        require!(claimed > 0, "Nothing to claim yet");

        account.pending = 0;
        account.credit_balance += claimed;
        account.last_claim_at = env::block_timestamp();
        self.accounts.insert(&account_id, &account);

        self.log_event(
            "credits_claimed",
            json!({ "account_id": account_id, "amount": U128(claimed) }),
        );

        U128(claimed)
    }

    /// Spend credits on one action.
    ///
    /// `amount` is passed in so the caller's intent is visible in the
    /// transaction, but the contract charges its own configured price — the
    /// backend reads this call to decide whether to run NEAR AI inference, so
    /// the price must not be something the client can talk down.
    pub fn spend_credits(&mut self, action: String, pet_id: Option<u64>, amount: U128) -> U128 {
        self.assert_running();
        self.update_pool();

        let price = self.prices.get(&action).expect("Unknown action");
        require!(
            amount.0 >= price,
            "Offered amount is below the price of this action"
        );

        let account_id = env::predecessor_account_id();
        let mut account = self.account_or_default(&account_id);
        self.harvest(&mut account);

        require!(
            account.credit_balance >= price,
            "Not enough credits — claim or convert some first"
        );

        account.credit_balance -= price;
        self.accounts.insert(&account_id, &account);
        self.total_spent += price;

        // A Focus Lens is consumed here, and whether it applied goes into the
        // event: the backend reads this transaction's logs to decide the XP
        // multiplier, so the bonus cannot be claimed by a client that merely
        // says it has one.
        let mut lens_applied = false;
        if action == "ai_feed" {
            if let Some(id) = pet_id {
                let mut state = self.pet_state_or_default(id);
                if state.lens_feeds_left > 0 {
                    state.lens_feeds_left -= 1;
                    lens_applied = true;
                    self.pet_states.insert(&id, &state);
                }
            }
        }

        // Doing things with your pet is what turns something up.
        let dropped = if matches!(action.as_str(), "ai_feed" | "train" | "craft_item") {
            let mut account = self.account_or_default(&account_id);
            let dropped = self.roll_drop(&account_id, &mut account, action.len() as u8);
            if dropped.is_some() {
                self.accounts.insert(&account_id, &account);
            }
            dropped
        } else {
            None
        };

        self.log_event(
            "credit_spent",
            json!({
                "account_id": account_id,
                "action": action,
                "pet_id": pet_id,
                "amount": U128(price),
                "lens_applied": lens_applied,
                "dropped_item": dropped,
            }),
        );

        U128(price)
    }

    // ── Credit items ─────────────────────────────────────────────────────────

    fn pet_state_or_default(&self, pet_id: u64) -> PetState {
        self.pet_states.get(&pet_id).unwrap_or_default()
    }

    // ── Inventory ────────────────────────────────────────────────────────────

    fn add_item(account: &mut Account, item_id: u16, quantity: u32) {
        match account.items.iter_mut().find(|(id, _)| *id == item_id) {
            Some(entry) => entry.1 = entry.1.saturating_add(quantity),
            None => account.items.push((item_id, quantity)),
        }
    }

    fn take_item(account: &mut Account, item_id: u16, quantity: u32) {
        let entry = account
            .items
            .iter_mut()
            .find(|(id, _)| *id == item_id)
            .unwrap_or_else(|| env::panic_str("You do not have that item"));

        require!(entry.1 >= quantity, "You do not have that many");
        entry.1 -= quantity;
        account.items.retain(|(_, count)| *count > 0);
    }

    /// Buy an item from the shop. It goes into your inventory; `use_item`
    /// applies it. The client sends both in one transaction when you buy to
    /// use straight away.
    pub fn buy_credit_item(&mut self, item_id: u16, quantity: u32) -> U128 {
        self.assert_running();
        self.update_pool();

        let quantity = quantity.max(1);
        let price = self.item_prices.get(&item_id).expect("Unknown item");
        let total = price * quantity as u128;
        let account_id = env::predecessor_account_id();

        let mut account = self.account_or_default(&account_id);
        self.harvest(&mut account);
        require!(
            account.credit_balance >= total,
            "Not enough credits — claim or earn some first"
        );
        account.credit_balance -= total;
        Self::add_item(&mut account, item_id, quantity);
        self.accounts.insert(&account_id, &account);
        self.total_spent += total;

        self.log_event(
            "item_bought",
            json!({
                "account_id": account_id,
                "item_id": item_id,
                "quantity": quantity,
                "credits": U128(total),
            }),
        );

        U128(total)
    }

    /// Spend one item from your inventory on a pet.
    pub fn use_item(&mut self, item_id: u16, pet_id: u64) {
        self.assert_running();
        let account_id = env::predecessor_account_id();

        if let Some(pet) = self.pets.get(&pet_id) {
            require!(pet.owner_id == account_id, "That pet belongs to someone else");
        }

        let mut account = self.account_or_default(&account_id);
        Self::take_item(&mut account, item_id, 1);
        self.accounts.insert(&account_id, &account);

        let now = env::block_timestamp();
        let mut state = self.pet_state_or_default(pet_id);

        match item_id {
            ITEM_TONIC => state.wounded_until = 0,
            ITEM_WARD => {
                // Extend an existing ward rather than cutting it short.
                let from = state.shielded_until.max(now);
                state.shielded_until = from + WARD_SECONDS * NANOS_PER_SECOND;
            }
            ITEM_LENS => {
                state.lens_feeds_left = state.lens_feeds_left.saturating_add(LENS_FEEDS);
            }
            ITEM_CHARM => state.star_guard = true,
            _ => env::panic_str("Unknown item"),
        }

        self.pet_states.insert(&pet_id, &state);

        self.log_event(
            "item_used",
            json!({ "account_id": account_id, "pet_id": pet_id, "item_id": item_id }),
        );
    }

    // ── Drops ────────────────────────────────────────────────────────────────

    /// Roll for a drop. A stronger pet finds things more often *and* finds
    /// better things, which is what makes levelling pay for itself.
    ///
    /// Randomness comes from the block's random seed. Good enough for loot;
    /// a validator could in principle influence it, so nothing of real value
    /// should ever hang on this roll alone.
    fn roll_drop(&mut self, account_id: &AccountId, account: &mut Account, salt: u8) -> Option<u16> {
        let (chance_bps, max_tier) = drop_odds(account.weight);
        if chance_bps == 0 {
            return None;
        }

        let seed = env::random_seed();
        let a = *seed.first().unwrap_or(&0) as u32;
        let b = *seed.get(1).unwrap_or(&0) as u32;
        let roll = ((a << 8 | b) ^ salt as u32) % 10_000;

        if roll >= chance_bps {
            return None;
        }

        let pick = *seed.get(2).unwrap_or(&0) as usize % (max_tier + 1);
        let item_id = DROPPABLE_ITEMS[pick];
        Self::add_item(account, item_id, 1);

        self.log_event(
            "item_dropped",
            json!({
                "account_id": account_id,
                "item_id": item_id,
                "chance_bps": chance_bps,
                "weight": U128(account.weight),
            }),
        );

        Some(item_id)
    }

    // ── Marketplace ──────────────────────────────────────────────────────────

    /// Offer items to other players, priced in credits.
    ///
    /// Items sell to players, never back to the contract. Selling to the
    /// contract would turn free drops into minted credits, and the drop rate
    /// rises with pet strength — that is an inflation pump with a difficulty
    /// slider on it. Between players the credits only change hands.
    pub fn list_item(&mut self, item_id: u16, quantity: u32, price_each: U128) -> U64 {
        self.assert_running();
        require!(quantity > 0, "Nothing to list");
        require!(price_each.0 > 0, "Set a price");
        require!(
            self.item_prices.get(&item_id).is_some(),
            "Unknown item"
        );

        let seller_id = env::predecessor_account_id();
        let mut account = self.account_or_default(&seller_id);
        Self::take_item(&mut account, item_id, quantity);
        self.accounts.insert(&seller_id, &account);

        let id = self.next_listing_id;
        self.next_listing_id += 1;

        self.listings.insert(
            &id,
            &Listing {
                id: U64(id),
                seller_id: seller_id.clone(),
                item_id,
                quantity,
                price_each,
                created_at: U64(env::block_timestamp()),
            },
        );

        self.log_event(
            "item_listed",
            json!({
                "listing_id": U64(id),
                "seller_id": seller_id,
                "item_id": item_id,
                "quantity": quantity,
                "price_each": price_each,
            }),
        );

        U64(id)
    }

    pub fn cancel_listing(&mut self, listing_id: U64) {
        let listing = self.listings.get(&listing_id.0).expect("No such listing");
        let seller_id = env::predecessor_account_id();
        require!(listing.seller_id == seller_id, "That listing is not yours");

        let mut account = self.account_or_default(&seller_id);
        Self::add_item(&mut account, listing.item_id, listing.quantity);
        self.accounts.insert(&seller_id, &account);
        self.listings.remove(&listing_id.0);

        self.log_event("listing_cancelled", json!({ "listing_id": listing_id }));
    }

    pub fn buy_listing(&mut self, listing_id: U64, quantity: u32) -> U128 {
        self.assert_running();
        self.update_pool();

        let mut listing = self.listings.get(&listing_id.0).expect("No such listing");
        let buyer_id = env::predecessor_account_id();
        require!(listing.seller_id != buyer_id, "That listing is yours");
        require!(quantity > 0 && quantity <= listing.quantity, "Not that many for sale");

        let total = listing.price_each.0 * quantity as u128;
        let rake = total * self.market_rake_bps as u128 / 10_000;
        let to_seller = total - rake;

        let mut buyer = self.account_or_default(&buyer_id);
        self.harvest(&mut buyer);
        require!(buyer.credit_balance >= total, "Not enough credits");
        buyer.credit_balance -= total;
        Self::add_item(&mut buyer, listing.item_id, quantity);
        self.accounts.insert(&buyer_id, &buyer);

        let mut seller = self.account_or_default(&listing.seller_id);
        self.harvest(&mut seller);
        seller.credit_balance += to_seller;
        self.accounts.insert(&listing.seller_id, &seller);

        // The rake leaves circulation into the reserve, so the market is a
        // small credit sink rather than a source.
        self.sale_reserve += rake;

        listing.quantity -= quantity;
        if listing.quantity == 0 {
            self.listings.remove(&listing_id.0);
        } else {
            self.listings.insert(&listing_id.0, &listing);
        }

        self.log_event(
            "listing_bought",
            json!({
                "listing_id": listing_id,
                "buyer_id": buyer_id,
                "seller_id": listing.seller_id,
                "item_id": listing.item_id,
                "quantity": quantity,
                "paid": U128(total),
                "rake": U128(rake),
            }),
        );

        U128(total)
    }

    pub fn set_market_rake_bps(&mut self, rake_bps: u16) {
        self.assert_owner();
        require!(rake_bps <= 2_000, "Rake cannot exceed 20%");
        self.market_rake_bps = rake_bps;
    }

    // ── Battles ──────────────────────────────────────────────────────────────

    /// Settle a battle. Called by the game contract, which decides who won.
    ///
    /// The loser's wager moves to the winner and the loser is wounded for a
    /// while. No pet dies: credits are streamed by weight share, so destroying
    /// another staker's pet would raise the attacker's own income, which makes
    /// griefing directly profitable. Battle therefore moves credits between
    /// players instead of destroying what they paid for.
    ///
    /// Owner accounts are passed in rather than looked up here: a pet that can
    /// be attacked is by definition not staked, so this contract holds no
    /// record of who owns it. The game contract is the authority on ownership
    /// and is the only account allowed to call this.
    pub fn report_battle(
        &mut self,
        winner_pet_id: u64,
        winner_id: AccountId,
        loser_pet_id: u64,
        loser_id: AccountId,
    ) -> BattleOutcome {
        self.assert_running();
        let caller = env::predecessor_account_id();
        require!(
            caller == self.game_contract || caller == self.owner_id,
            "Only the game contract can settle battles"
        );
        require!(winner_pet_id != loser_pet_id, "A pet cannot fight itself");
        require!(winner_id != loser_id, "Both pets have the same owner");
        require!(
            self.is_attackable(loser_pet_id),
            "That pet is protected right now"
        );
        require!(
            !self.is_staked(winner_pet_id),
            "A pet in the AI Lab cannot fight"
        );

        self.update_pool();

        let mut loser = self.account_or_default(&loser_id);
        self.harvest(&mut loser);

        // Take what the loser can cover, so a battle never fails just because
        // they are short — it simply pays out less.
        let pot = self.battle_wager.min(loser.credit_balance);
        let rake = pot * self.battle_rake_bps as u128 / 10_000;
        let transferred = pot - rake;
        loser.credit_balance -= pot;

        let now = env::block_timestamp();
        let mut loser_state = self.pet_state_or_default(loser_pet_id);
        loser_state.wounded_until = now + WOUND_SECONDS * NANOS_PER_SECOND;

        let star_protected = loser_state.star_guard;
        if star_protected {
            loser_state.star_guard = false;
        }

        self.pet_states.insert(&loser_pet_id, &loser_state);
        self.accounts.insert(&loser_id, &loser);

        let mut winner = self.account_or_default(&winner_id);
        self.harvest(&mut winner);
        winner.credit_balance += transferred;
        self.roll_drop(&winner_id, &mut winner, 7);
        self.accounts.insert(&winner_id, &winner);

        // Rake funds the reserve behind credit sales and the faucet, so PvP
        // never dilutes the staking stream.
        self.sale_reserve += rake;

        self.log_event(
            "battle_settled",
            json!({
                "winner_id": winner_id,
                "loser_id": loser_id,
                "winner_pet_id": winner_pet_id,
                "loser_pet_id": loser_pet_id,
                "transferred": U128(transferred),
                "rake": U128(rake),
                "star_protected": star_protected,
            }),
        );

        BattleOutcome {
            transferred: U128(transferred),
            rake: U128(rake),
            star_protected,
        }
    }


    /// A one-off grant, so a new player can try AI Feed before owning a pet.
    ///
    /// Once per account, not once per day: NEAR implicit accounts are nearly
    /// free to create, so a daily faucet is a sybil tap on the treasury rather
    /// than an onboarding gift.
    pub fn claim_faucet(&mut self) -> U128 {
        self.assert_running();

        let account_id = env::predecessor_account_id();
        let mut account = self.account_or_default(&account_id);
        let now = env::block_timestamp();

        require!(account.last_faucet_at == 0, "You have already used the faucet");
        require!(
            self.sale_reserve >= self.faucet_amount,
            "The faucet is dry — try again later"
        );

        self.sale_reserve -= self.faucet_amount;
        account.credit_balance += self.faucet_amount;
        account.last_faucet_at = now;
        self.accounts.insert(&account_id, &account);

        self.log_event(
            "faucet_claimed",
            json!({ "account_id": account_id, "amount": U128(self.faucet_amount) }),
        );

        U128(self.faucet_amount)
    }

    pub fn set_auto_feed_budget(&mut self, budget: U128) {
        let account_id = env::predecessor_account_id();
        let mut account = self.account_or_default(&account_id);
        account.auto_feed_budget = budget.0;
        self.accounts.insert(&account_id, &account);

        self.log_event(
            "auto_feed_set",
            json!({ "account_id": account_id, "budget": budget }),
        );
    }

    // ── Feed XP ──────────────────────────────────────────────────────────────

    /// Report the XP one AI feed earned, and forward it to the game contract.
    ///
    /// The XP cannot be awarded inside `spend_credits`: the taste multiplier is
    /// only known once NEAR AI has judged the meal, which happens off-chain
    /// after the credits are already debited. So the backend verifies the
    /// `spend_credits` receipt, runs the inference, and reports the result here.
    ///
    /// `game::award_xp` accepts only this contract or the game owner, which is
    /// why the hop exists at all — the alternative is handing the backend a key
    /// on the game owner's account, which can mint, pause and transfer pets.
    ///
    /// `xp` is clamped to `MAX_FEED_XP` rather than trusted: the caller is a
    /// server, and a server can be compromised. One feed is worth at most one
    /// perfect feed.
    pub fn award_feed_xp(&mut self, pet_id: u64, xp: u32) -> Promise {
        self.assert_running();
        let caller = env::predecessor_account_id();
        require!(
            caller == self.oracle || caller == self.owner_id,
            "Only the feed oracle can award XP"
        );
        require!(xp > 0, "Nothing to award");

        let awarded = xp.min(MAX_FEED_XP);
        self.log_event(
            "feed_xp_awarded",
            json!({ "pet_id": pet_id, "xp": awarded, "requested": xp }),
        );

        ext_game::ext(self.game_contract.clone())
            .with_static_gas(GAS_FOR_AWARD_XP)
            .award_xp(pet_id, awarded)
    }

    /// Name the account whose key the AI backend signs `award_feed_xp` with.
    pub fn set_oracle(&mut self, oracle: AccountId) {
        self.assert_owner();
        self.oracle = oracle;
        self.log_event("oracle_set", json!({ "oracle": self.oracle }));
    }

    // ── Owner ────────────────────────────────────────────────────────────────

    /// Publish the credit allowance NEAR AI grants for the treasury's staked
    /// NEAR. This is the one number that sets total emission.
    pub fn set_epoch_budget(&mut self, epoch_budget: U128) {
        self.assert_owner();
        self.update_pool();
        self.epoch_budget = epoch_budget.0;
        self.log_event("epoch_budget_set", json!({ "epoch_budget": epoch_budget }));
    }

    /// Record credits the treasury has actually bought on NEAR AI and is
    /// willing to sell for NEAR or give away through the faucet and mining.
    pub fn fund_reserve(&mut self, credits: U128) {
        self.assert_owner();
        self.sale_reserve += credits.0;
        self.log_event(
            "reserve_funded",
            json!({ "credits": credits, "reserve": U128(self.sale_reserve) }),
        );
    }

    pub fn set_action_price(&mut self, action: String, credits: U128) {
        self.assert_owner();
        self.prices.insert(&action, &credits.0);
    }

    /// Retire a credit sink, so `spend_credits` refuses it outright.
    ///
    /// `set_action_price(action, 0)` is not the same thing and not a
    /// substitute: a price of zero still passes every check in
    /// `spend_credits`, so the action stays spendable and merely becomes free.
    /// Deleting the key is what makes it "Unknown action" again.
    ///
    /// This exists because `revive` was priced at 1 credit and advertised in
    /// the client, and no contract ever implemented it — a sink that debited a
    /// balance and did nothing. Removing it from `new`'s seed fixes fresh
    /// deployments only; the price lives in contract *state*, so a deployment
    /// already running needs this call to forget it.
    pub fn remove_action_price(&mut self, action: String) -> bool {
        self.assert_owner();
        let existed = self.prices.remove(&action).is_some();
        self.log_event(
            "action_price_removed",
            json!({ "action": action, "existed": existed }),
        );
        existed
    }

    pub fn set_item_price(&mut self, item_id: u16, credits: U128) {
        self.assert_owner();
        self.item_prices.insert(&item_id, &credits.0);
    }

    pub fn set_battle_params(&mut self, wager: U128, rake_bps: u16) {
        self.assert_owner();
        require!(rake_bps <= 2_000, "Rake cannot exceed 20%");
        self.battle_wager = wager.0;
        self.battle_rake_bps = rake_bps;
        self.log_event(
            "battle_params_set",
            json!({ "wager": wager, "rake_bps": rake_bps }),
        );
    }

    pub fn set_faucet_amount(&mut self, amount: U128) {
        self.assert_owner();
        self.faucet_amount = amount.0;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.assert_owner();
        self.update_pool();
        self.paused = paused;
    }

    pub fn set_game_contract(&mut self, game_contract: AccountId) {
        self.assert_owner();
        self.game_contract = game_contract;
    }

    /// Recompute an account's weight from the pets actually on record.
    ///
    /// A safety valve, added because the pool had no way back. The weight
    /// double-count that `drop_stake_record` now prevents left accounts holding
    /// weight for pets that had already been unstaked, and no method on this
    /// contract could take it off again — the only repair was a redeploy with
    /// fresh state, which costs every player their stake. `self.pets` is the
    /// authority here: it holds one record per genuinely staked pet.
    pub fn admin_rebuild_account(&mut self, account_id: AccountId) -> U128 {
        self.assert_owner();
        self.update_pool();

        let mut account = self.account_or_default(&account_id);
        // Credits earned up to now were earned at the weight on record, wrong
        // or not. Repairing the books is not a reason to claw those back.
        self.harvest(&mut account);

        let mut staked: Vec<u64> = Vec::new();
        let mut weight: u128 = 0;
        for (pet_id, pet) in self.pets.iter() {
            if pet.owner_id == account_id {
                staked.push(pet_id);
                weight += pet.weight.0;
            }
        }

        self.total_weight = self.total_weight.saturating_sub(account.weight) + weight;
        account.weight = weight;
        account.staked = staked;
        account.reward_debt = self.accrued(&account);
        self.accounts.insert(&account_id, &account);

        self.log_event(
            "account_rebuilt",
            json!({ "account_id": account_id, "weight": U128(weight) }),
        );

        U128(weight)
    }

    /// Escape hatch for a game contract that predates `get_pet_attrs`: an
    /// operator pushes the pet's owner and attributes instead.
    pub fn admin_set_pet(&mut self, pet_id: u64, owner_id: AccountId, attrs: PetAttrs) {
        self.assert_owner();
        if let Some(existing) = self.pets.get(&pet_id) {
            require!(
                existing.owner_id == owner_id,
                "Unstake the pet before moving it to another owner"
            );
        }
        // `internal_stake` replaces the old weight rather than adding to it.
        self.internal_stake(pet_id, owner_id, attrs);
    }

    // ── Views ────────────────────────────────────────────────────────────────

    pub fn get_pool(&self) -> PoolView {
        PoolView {
            total_weight: U128(self.total_weight),
            epoch_budget: U128(self.epoch_budget),
            rate_per_second: U128(self.rate_per_second()),
            staked_pets: self.pets.len(),
            total_distributed: U128(self.total_distributed),
            total_spent: U128(self.total_spent),
            sale_reserve: U128(self.sale_reserve),
            updated_at: U64(self.updated_at),
            paused: self.paused,
        }
    }

    pub fn get_account(&self, account_id: AccountId) -> Option<AccountView> {
        let account = self.accounts.get(&account_id)?;

        // Project the accumulator forward so the view matches what a claim
        // right now would actually pay.
        let now = env::block_timestamp();
        let mut acc = self.acc_credit_per_weight;
        if self.total_weight > 0 && now > self.updated_at {
            let elapsed = ((now - self.updated_at) / NANOS_PER_SECOND) as u128;
            acc += self.scaled_emission(elapsed) / self.total_weight;
        }

        let accrued = acc * account.weight / ACC_PRECISION;
        let pending = account.pending + accrued.saturating_sub(account.reward_debt);

        Some(AccountView {
            account_id,
            staked_pets: account.staked.clone(),
            weight: U128(account.weight),
            credit_balance: U128(account.credit_balance),
            pending_credits: U128(pending),
            auto_feed_budget: U128(account.auto_feed_budget),
            last_claim_at: U64(account.last_claim_at),
            items: account.items.clone(),
        })
    }

    pub fn get_staked_pets(&self, account_id: AccountId) -> Vec<StakedPet> {
        self.accounts
            .get(&account_id)
            .map(|account| {
                account
                    .staked
                    .iter()
                    .filter_map(|pet_id| self.pets.get(pet_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A pet at work in the lab is not available to fight.
    pub fn is_staked(&self, pet_id: u64) -> bool {
        self.pets.get(&pet_id).is_some()
    }

    /// The game contract must call this before allowing an attack.
    ///
    /// A staked pet is unavailable in *both* directions: it cannot attack, and
    /// it cannot be attacked. Without the second half the AI Lab would be a
    /// honeypot — staking would mark you as a target, and cutting another
    /// staker's weight raises the attacker's own share of the stream.
    pub fn is_attackable(&self, pet_id: u64) -> bool {
        if self.pets.get(&pet_id).is_some() {
            return false;
        }
        let now = env::block_timestamp();
        let state = self.pet_state_or_default(pet_id);
        state.shielded_until <= now && state.wounded_until <= now
    }

    pub fn get_pet_state(&self, pet_id: u64) -> PetStateView {
        let state = self.pet_state_or_default(pet_id);
        let staked = self.pets.get(&pet_id).is_some();
        let now = env::block_timestamp();

        PetStateView {
            pet_id,
            wounded_until: U64(state.wounded_until),
            shielded_until: U64(state.shielded_until),
            lens_feeds_left: state.lens_feeds_left,
            star_guard: state.star_guard,
            staked,
            attackable: !staked && state.shielded_until <= now && state.wounded_until <= now,
        }
    }

    pub fn get_pet_states(&self, pet_ids: Vec<u64>) -> Vec<PetStateView> {
        pet_ids
            .into_iter()
            .map(|pet_id| self.get_pet_state(pet_id))
            .collect()
    }

    pub fn get_action_price(&self, action: String) -> Option<U128> {
        self.prices.get(&action).map(U128)
    }

    pub fn get_item_price(&self, item_id: u16) -> Option<U128> {
        self.item_prices.get(&item_id).map(U128)
    }

    /// What a pet's owner can expect to find, for the UI to show honestly.
    pub fn get_drop_odds(&self, account_id: AccountId) -> near_sdk::serde_json::Value {
        let account = self.account_or_default(&account_id);
        let (chance_bps, tier) = drop_odds(account.weight);
        json!({
            "weight": U128(account.weight),
            "chance_bps": chance_bps,
            "best_item": DROPPABLE_ITEMS[tier],
            "can_drop": DROPPABLE_ITEMS[..=tier].to_vec(),
        })
    }

    pub fn get_inventory(&self, account_id: AccountId) -> Vec<(u16, u32)> {
        self.account_or_default(&account_id).items
    }

    pub fn get_listings(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<Listing> {
        let from = from_index.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(50) as usize;
        self.listings
            .values()
            .skip(from)
            .take(limit)
            .collect()
    }

    pub fn get_listings_by(&self, account_id: AccountId) -> Vec<Listing> {
        self.listings
            .values()
            .filter(|listing| listing.seller_id == account_id)
            .collect()
    }

    /// Every credit item and its price, for the shop UI.
    pub fn get_items(&self) -> Vec<(u16, U128)> {
        [ITEM_TONIC, ITEM_WARD, ITEM_LENS, ITEM_CHARM]
            .into_iter()
            .filter_map(|id| self.item_prices.get(&id).map(|price| (id, U128(price))))
            .collect()
    }

    pub fn get_config(&self) -> near_sdk::serde_json::Value {
        json!({
            "owner_id": self.owner_id,
            "game_contract": self.game_contract,
            "granter": self.granter,
            "oracle": self.oracle,
            "max_feed_xp": MAX_FEED_XP,
            "faucet_amount": U128(self.faucet_amount),
            "mining_reward": U128(self.mining_reward),
            "mining_cooldown_seconds": MINING_COOLDOWN_SECONDS,
            "credit_decimals": CREDIT_DECIMALS,
            "battle_wager": U128(self.battle_wager),
            "battle_rake_bps": self.battle_rake_bps,
            "ward_seconds": WARD_SECONDS,
            "wound_seconds": WOUND_SECONDS,
            "lens_feeds": LENS_FEEDS,
        })
    }
}

// ── Credit grants and mining ─────────────────────────────────────────────────

#[near]
impl PetStake {
    /// Hand credits to a player out of the reserve.
    ///
    /// This contract never touches NEAR. Payments land on the mint router,
    /// which is the only account allowed to call this — so there is exactly one
    /// place in the system that custodies funds, and this one stays a pure
    /// ledger of a resource that cannot be transferred out anyway.
    pub fn grant_credits(&mut self, account_id: AccountId, credits: U128) -> U128 {
        self.assert_running();
        let caller = env::predecessor_account_id();
        require!(
            caller == self.granter || caller == self.owner_id,
            "Only the mint router can grant credits"
        );
        require!(credits.0 > 0, "Nothing to grant");

        // Pay what the reserve can cover and report it, so the router can
        // refund the difference rather than silently keeping the money.
        let granted = credits.0.min(self.sale_reserve);
        if granted == 0 {
            env::log_str("Credit reserve is empty");
            return U128(0);
        }

        self.sale_reserve -= granted;

        let mut account = self.account_or_default(&account_id);
        account.credit_balance += granted;
        self.accounts.insert(&account_id, &account);

        self.log_event(
            "credits_granted",
            json!({ "account_id": account_id, "credits": U128(granted) }),
        );

        U128(granted)
    }

    pub fn set_granter(&mut self, granter: AccountId) {
        self.assert_owner();
        self.granter = granter;
    }

    /// Mining: a small credit payout for anyone with a pet at work.
    ///
    /// Gated on staked weight rather than on an account existing, because a
    /// NEAR account is nearly free while a pet costs real money — that is what
    /// keeps this from being the sybil tap a plain daily faucet was.
    pub fn claim_mining(&mut self) -> U128 {
        self.assert_running();
        self.update_pool();

        let account_id = env::predecessor_account_id();
        let mut account = self.account_or_default(&account_id);
        require!(account.weight > 0, "Stake a pet in the AI Lab first");

        // As with the faucet, 0 means "never mined" rather than "mined at the
        // dawn of the chain".
        let now = env::block_timestamp();
        require!(
            account.last_mining_at == 0
                || now.saturating_sub(account.last_mining_at)
                    >= MINING_COOLDOWN_SECONDS * NANOS_PER_SECOND,
            "Mining is still on cooldown"
        );
        require!(
            self.sale_reserve >= self.mining_reward,
            "The mining reserve is empty — try again later"
        );

        self.sale_reserve -= self.mining_reward;
        account.credit_balance += self.mining_reward;
        account.last_mining_at = now;
        self.roll_drop(&account_id, &mut account, 13);
        self.accounts.insert(&account_id, &account);

        self.log_event(
            "mining_claimed",
            json!({ "account_id": account_id, "amount": U128(self.mining_reward) }),
        );

        U128(self.mining_reward)
    }

    pub fn set_mining_reward(&mut self, amount: U128) {
        self.assert_owner();
        self.mining_reward = amount.0;
    }

    /// When a player can mine again. 0 means now.
    pub fn mining_available_at(&self, account_id: AccountId) -> U64 {
        let account = self.account_or_default(&account_id);
        if account.last_mining_at == 0 {
            return U64(0);
        }
        U64(account.last_mining_at + MINING_COOLDOWN_SECONDS * NANOS_PER_SECOND)
    }
}

// ── Migration ────────────────────────────────────────────────────────────────

/// The contract state as it was before `oracle` was added.
///
/// Adding a field to the contract struct is a Borsh layout change, and this
/// repository has already lost two deployments to one (see
/// docs/GAME_CONTRACT_V2.md §9). `oracle` is appended last, so these bytes are
/// exactly the new layout minus its final `AccountId` — which is the whole
/// reason a migration is possible here instead of fresh accounts: a redeploy
/// with new state costs every player their staked pet and their credit
/// balance, and on mainnet that is not a thing that can be done at all.
#[derive(near_sdk::borsh::BorshDeserialize)]
#[borsh(crate = "near_sdk::borsh")]
struct PetStakeV1 {
    owner_id: AccountId,
    game_contract: AccountId,
    granter: AccountId,
    epoch_budget: u128,
    total_weight: u128,
    acc_credit_per_weight: u128,
    updated_at: u64,
    accounts: UnorderedMap<AccountId, Account>,
    pets: UnorderedMap<u64, StakedPet>,
    prices: LookupMap<String, u128>,
    pet_states: UnorderedMap<u64, PetState>,
    item_prices: LookupMap<u16, u128>,
    battle_wager: u128,
    battle_rake_bps: u16,
    total_distributed: u128,
    total_spent: u128,
    sale_reserve: u128,
    faucet_amount: u128,
    mining_reward: u128,
    listings: UnorderedMap<u64, Listing>,
    next_listing_id: u64,
    market_rake_bps: u16,
    paused: bool,
}

#[near]
impl PetStake {
    /// Deploy over a contract that predates `oracle`, keeping every stake,
    /// balance, item and listing.
    ///
    /// Owner-gated by reading the *old* state's `owner_id` — `assert_owner`
    /// cannot be used, because at this point `self` does not exist yet.
    #[init(ignore_state)]
    pub fn migrate(oracle: AccountId) -> Self {
        let old: PetStakeV1 = env::state_read().expect("No state to migrate");
        require!(
            env::predecessor_account_id() == old.owner_id,
            "Only the owner can migrate"
        );

        Self {
            owner_id: old.owner_id,
            game_contract: old.game_contract,
            granter: old.granter,
            epoch_budget: old.epoch_budget,
            total_weight: old.total_weight,
            acc_credit_per_weight: old.acc_credit_per_weight,
            updated_at: old.updated_at,
            accounts: old.accounts,
            pets: old.pets,
            prices: old.prices,
            pet_states: old.pet_states,
            item_prices: old.item_prices,
            battle_wager: old.battle_wager,
            battle_rake_bps: old.battle_rake_bps,
            total_distributed: old.total_distributed,
            total_spent: old.total_spent,
            sale_reserve: old.sale_reserve,
            faucet_amount: old.faucet_amount,
            mining_reward: old.mining_reward,
            listings: old.listings,
            next_listing_id: old.next_listing_id,
            market_rake_bps: old.market_rake_bps,
            paused: old.paused,
            oracle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn owner() -> AccountId {
        "owner.near".parse().unwrap()
    }
    fn game() -> AccountId {
        "game.near".parse().unwrap()
    }
    fn router() -> AccountId {
        "mint.near".parse().unwrap()
    }
    fn alice() -> AccountId {
        "alice.near".parse().unwrap()
    }
    fn bob() -> AccountId {
        "bob.near".parse().unwrap()
    }
    fn oracle() -> AccountId {
        "oracle.near".parse().unwrap()
    }

    fn ctx(predecessor: AccountId, seconds: u64) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .predecessor_account_id(predecessor)
            .block_timestamp(seconds * NANOS_PER_SECOND)
            .attached_deposit(NearToken::from_yoctonear(1));
        builder
    }

    fn attrs(level: u32, star: u32, phase: u8, rarity: Rarity) -> PetAttrs {
        PetAttrs {
            rarity,
            level,
            star,
            evolution_phase: phase,
        }
    }

    /// 500 credits per 30-day epoch.
    fn deploy() -> PetStake {
        testing_env!(ctx(owner(), 0).build());
        PetStake::new(owner(), game(), router(), U128(500 * ONE_CREDIT), Some(oracle()))
    }

    #[test]
    fn weight_grows_sub_linearly() {
        let common = attrs(0, 0, 1, Rarity::Common);
        let maxed = attrs(20, 5, 4, Rarity::Legendary);

        assert_eq!(common.weight(), 100);
        // 400 * 2.0 * 1.75 * 1.6 = 2240
        assert_eq!(maxed.weight(), 2240);
        // A fully maxed legendary is worth ~22 commons, not thousands.
        assert!(maxed.weight() < common.weight() * 30);
    }

    #[test]
    fn single_staker_earns_the_whole_budget() {
        let mut contract = deploy();

        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common));

        // One epoch later.
        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        let view = contract.get_account(alice()).unwrap();

        // The whole epoch budget, to the micro-credit.
        assert_eq!(view.pending_credits.0, 500 * ONE_CREDIT);
    }

    #[test]
    fn two_stakers_split_by_weight() {
        let mut contract = deploy();

        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common)); // weight 100
        contract.admin_set_pet(2, bob(), attrs(0, 0, 1, Rarity::Rare)); // weight 160

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        let a = contract.get_account(alice()).unwrap().pending_credits.0;
        let b = contract.get_account(bob()).unwrap().pending_credits.0;

        // 100 : 160 == 5 : 8
        let ratio = b * 1_000 / a;
        assert!(ratio > 1_590 && ratio < 1_610, "ratio was {ratio}");
    }

    #[test]
    fn claim_then_spend() {
        let mut contract = deploy();

        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(10, 2, 2, Rarity::Rare));

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        let claimed = contract.claim_credits().0;
        assert!(claimed > 0);

        let spent = contract.spend_credits("ai_feed".to_string(), Some(1), U128(2_000)).0;
        assert_eq!(spent, 2_000);

        let view = contract.get_account(alice()).unwrap();
        assert_eq!(view.credit_balance.0, claimed - 2_000);
        assert_eq!(contract.get_pool().total_spent.0, 2_000);
    }

    #[test]
    #[should_panic(expected = "Not enough credits")]
    fn cannot_spend_what_you_do_not_have() {
        let mut contract = deploy();
        testing_env!(ctx(alice(), 10).build());
        contract.spend_credits("ai_feed".to_string(), None, U128(2_000));
    }

    #[test]
    #[should_panic(expected = "Unknown action")]
    fn rejects_unknown_actions() {
        let mut contract = deploy();
        testing_env!(ctx(alice(), 10).build());
        contract.spend_credits("free_lunch".to_string(), None, U128(1));
    }

    /// The bug this pins cost a whole testnet round.
    ///
    /// `sync_pet` re-reads a pet after it levels up and lands back in
    /// `on_pet_attrs`, the same callback a first stake uses. That callback added
    /// the new weight on top of the old one, so one level-up left alice holding
    /// 100 + 105 for a single pet, the pool holding the same 205, and `staked`
    /// listing pet 1 twice. Unstaking then returned only the newer 105 and left
    /// 100 units of weight earning credits for a pet that was no longer there.
    ///
    /// Every existing test went through `admin_set_pet`, which unwound the old
    /// record by hand — which is exactly why none of them caught this.
    fn resync(contract: &mut PetStake, pet_id: u64, owner: AccountId, attrs: PetAttrs) {
        contract.on_pet_attrs(pet_id, owner.clone(), Ok((owner, attrs)));
    }

    #[test]
    fn a_level_up_replaces_the_pets_weight_instead_of_adding_to_it() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());

        resync(&mut contract, 1, alice(), attrs(0, 0, 1, Rarity::Common));
        assert_eq!(contract.get_pool().total_weight.0, 100);

        // Level 1: 100 * 1.05.
        resync(&mut contract, 1, alice(), attrs(1, 0, 1, Rarity::Common));

        let view = contract.get_account(alice()).unwrap();
        assert_eq!(view.weight.0, 105, "the pet is worth 105, not 100 + 105");
        assert_eq!(view.staked_pets, vec![1], "the pet is staked once, not twice");
        assert_eq!(contract.get_pool().total_weight.0, 105);
        assert_eq!(contract.get_pool().staked_pets, 1);
    }

    #[test]
    fn unstaking_a_levelled_pet_empties_the_pool() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());

        resync(&mut contract, 1, alice(), attrs(0, 0, 1, Rarity::Common));
        resync(&mut contract, 1, alice(), attrs(1, 0, 1, Rarity::Common));
        resync(&mut contract, 1, alice(), attrs(2, 0, 1, Rarity::Common));

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.unstake_pet(1);

        // Nothing is staked, so nothing may still be carrying weight. Phantom
        // weight here is not cosmetic: it keeps paying out a share of every
        // epoch to an account that has no pet at work.
        assert_eq!(contract.get_account(alice()).unwrap().weight.0, 0);
        assert_eq!(contract.get_pool().total_weight.0, 0);
        assert_eq!(contract.get_pool().staked_pets, 0);
    }

    #[test]
    fn a_corrupted_account_can_be_rebuilt_from_the_pets_on_record() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());

        resync(&mut contract, 1, alice(), attrs(0, 0, 1, Rarity::Common));
        resync(&mut contract, 2, alice(), attrs(0, 0, 1, Rarity::Rare));

        // Forge the damage the old double-count left behind: weight for a pet
        // that is no longer staked, and a stale id in `staked`.
        let mut account = contract.accounts.get(&alice()).unwrap();
        account.weight += 500;
        account.staked.push(99);
        contract.accounts.insert(&alice(), &account);
        contract.total_weight += 500;

        testing_env!(ctx(owner(), 10).build());
        let rebuilt = contract.admin_rebuild_account(alice()).0;

        // 100 + 160, from the two records that really exist.
        assert_eq!(rebuilt, 260);
        let view = contract.get_account(alice()).unwrap();
        assert_eq!(view.weight.0, 260);
        assert_eq!(view.staked_pets, vec![1, 2]);
        assert_eq!(contract.get_pool().total_weight.0, 260);
    }

    #[test]
    fn rebuilding_keeps_what_was_already_earned() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        resync(&mut contract, 1, alice(), attrs(0, 0, 1, Rarity::Common));

        testing_env!(ctx(owner(), EPOCH_SECONDS).build());
        let before = contract.get_account(alice()).unwrap().pending_credits.0;
        contract.admin_rebuild_account(alice());
        let after = contract.get_account(alice()).unwrap().pending_credits.0;

        assert!(before > 0);
        assert_eq!(before, after);
    }

    #[test]
    #[should_panic(expected = "Only the owner can call this")]
    fn players_cannot_rebuild_their_own_books() {
        let mut contract = deploy();
        testing_env!(ctx(alice(), 10).build());
        contract.admin_rebuild_account(alice());
    }

    // ── Credit sinks ─────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Unknown action")]
    fn revive_is_not_a_sink() {
        // It was priced at 1 credit and listed in the client, and no contract
        // ever implemented it: `spend_credits("revive")` debited a balance and
        // did nothing. Reviving a starved pet is the game contract's Holy
        // Water, bought with NEAR, and always was.
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common));

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.claim_credits();
        contract.spend_credits("revive".to_string(), Some(1), U128(ONE_CREDIT));
    }

    #[test]
    fn the_owner_can_retire_a_sink_from_a_running_deployment() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common));

        // A deployment that predates this still has the price in state, so
        // stand one back up the only way state can be written.
        contract.set_action_price("revive".to_string(), U128(ONE_CREDIT));

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.claim_credits();
        // Spendable again, which is the problem this call is for.
        contract.spend_credits("revive".to_string(), Some(1), U128(ONE_CREDIT));

        testing_env!(ctx(owner(), EPOCH_SECONDS).build());
        assert!(contract.remove_action_price("revive".to_string()));
        // Idempotent: nothing left to remove the second time.
        assert!(!contract.remove_action_price("revive".to_string()));
        assert!(contract.get_action_price("revive".to_string()).is_none());
    }

    #[test]
    #[should_panic(expected = "Unknown action")]
    fn a_retired_sink_cannot_be_spent() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common));
        contract.set_action_price("revive".to_string(), U128(ONE_CREDIT));
        contract.remove_action_price("revive".to_string());

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.claim_credits();
        contract.spend_credits("revive".to_string(), Some(1), U128(ONE_CREDIT));
    }

    #[test]
    fn a_price_of_zero_is_not_how_you_retire_a_sink() {
        // The trap this method exists to avoid: zero passes `amount >= price`
        // and `balance >= price`, so the action stays live and merely becomes
        // free. Pinned so nobody "simplifies" `remove_action_price` away.
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common));
        contract.set_action_price("revive".to_string(), U128(0));

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        assert_eq!(
            contract
                .spend_credits("revive".to_string(), Some(1), U128(0))
                .0,
            0
        );
    }

    #[test]
    #[should_panic(expected = "Only the owner can call this")]
    fn players_cannot_retire_a_sink() {
        let mut contract = deploy();
        testing_env!(ctx(alice(), 10).build());
        contract.remove_action_price("ai_feed".to_string());
    }

    // ── Feed XP ──────────────────────────────────────────────────────────────
    //
    // `award_feed_xp` returns a Promise that a unit test cannot resolve, hence
    // the `let _ =`. What is under test is everything that runs before it: the
    // caller check, the clamp and the event. That the forwarded call reaches
    // `game::award_xp` is proved on testnet — see docs/GAME_CONTRACT_V2.md §10.

    #[test]
    #[should_panic(expected = "Only the feed oracle can award XP")]
    fn players_cannot_award_their_own_pet_xp() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common));

        // Owning the pet is not the point: XP comes from an inference this
        // contract cannot see, so only the backend that ran it may report it.
        testing_env!(ctx(alice(), 10).build());
        let _ = contract.award_feed_xp(1, 60);
    }

    #[test]
    fn the_oracle_can_award_xp() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common));

        testing_env!(ctx(oracle(), 10).build());
        let _ = contract.award_feed_xp(1, 63);

        let logs = near_sdk::test_utils::get_logs();
        let awarded = logs
            .iter()
            .find(|l| l.contains("\"feed_xp_awarded\""))
            .unwrap_or_else(|| panic!("logs were {logs:?}"));
        assert!(awarded.contains("\"xp\":63"), "log was {awarded}");
    }

    #[test]
    fn the_owner_can_award_xp_too() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common));
        // Same escape hatch every other admin method has, so a broken oracle
        // key does not freeze levelling.
        let _ = contract.award_feed_xp(1, 10);
    }

    #[test]
    fn one_feed_cannot_be_worth_more_than_a_perfect_feed() {
        let mut contract = deploy();
        testing_env!(ctx(oracle(), 10).build());

        // The game contract's own clamp is 500, which a compromised backend
        // could use to take a pet five levels in five calls.
        let _ = contract.award_feed_xp(1, 500);

        let logs = near_sdk::test_utils::get_logs();
        let awarded = logs
            .iter()
            .find(|l| l.contains("\"feed_xp_awarded\""))
            .unwrap_or_else(|| panic!("logs were {logs:?}"));
        assert!(
            awarded.contains(&format!("\"xp\":{MAX_FEED_XP}")),
            "log was {awarded}"
        );
        // What was asked for is kept, so an oracle gone wrong is visible.
        assert!(awarded.contains("\"requested\":500"), "log was {awarded}");
    }

    #[test]
    #[should_panic(expected = "Nothing to award")]
    fn a_zero_xp_award_is_refused() {
        let mut contract = deploy();
        testing_env!(ctx(oracle(), 10).build());
        let _ = contract.award_feed_xp(1, 0);
    }

    #[test]
    fn the_owner_can_move_the_oracle() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.set_oracle(bob());

        testing_env!(ctx(bob(), 10).build());
        let _ = contract.award_feed_xp(1, 20);
    }

    #[test]
    #[should_panic(expected = "Only the owner can call this")]
    fn a_player_cannot_make_themselves_the_oracle() {
        let mut contract = deploy();
        testing_env!(ctx(alice(), 10).build());
        contract.set_oracle(alice());
    }

    #[test]
    #[should_panic(expected = "Only the feed oracle can award XP")]
    fn the_old_oracle_loses_the_right_when_it_moves() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.set_oracle(bob());

        testing_env!(ctx(oracle(), 10).build());
        let _ = contract.award_feed_xp(1, 20);
    }

    #[test]
    fn the_cap_is_exactly_one_perfect_feed() {
        // Mirrors feedXp(1, true) in src/lib/economy/params.ts: 35 * 2.05.
        assert_eq!(MAX_FEED_XP, 72);
    }

    #[test]
    fn a_resync_does_not_report_a_new_stake() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());

        resync(&mut contract, 1, alice(), attrs(0, 0, 1, Rarity::Common));
        resync(&mut contract, 1, alice(), attrs(1, 0, 1, Rarity::Common));

        // Both emitted `pet_staked` before, so an indexer counted one pet as
        // two stakes every time it levelled.
        let logs = near_sdk::test_utils::get_logs();
        let staked = logs.iter().filter(|l| l.contains("\"pet_staked\"")).count();
        let resynced = logs.iter().filter(|l| l.contains("\"pet_resynced\"")).count();
        assert_eq!(staked, 1, "logs were {logs:?}");
        assert_eq!(resynced, 1, "logs were {logs:?}");
    }

    #[test]
    fn a_resync_cannot_hand_the_pet_to_someone_else() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());

        resync(&mut contract, 1, alice(), attrs(0, 0, 1, Rarity::Common));
        // The game contract says bob owns it; alice asked. Nothing moves.
        contract.on_pet_attrs(1, alice(), Ok((bob(), attrs(5, 0, 1, Rarity::Common))));

        assert_eq!(contract.get_account(alice()).unwrap().weight.0, 100);
        assert!(contract.get_account(bob()).is_none());
        assert_eq!(contract.get_pool().total_weight.0, 100);
    }

    #[test]
    fn unstaking_keeps_what_was_earned_and_stops_the_stream() {
        let mut contract = deploy();

        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(1, alice(), attrs(0, 0, 1, Rarity::Common));

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.unstake_pet(1);
        let after_unstake = contract.get_account(alice()).unwrap().pending_credits.0;

        // Another epoch passes with nothing staked.
        testing_env!(ctx(alice(), EPOCH_SECONDS * 2).build());
        let later = contract.get_account(alice()).unwrap().pending_credits.0;

        assert!(after_unstake > 0);
        assert_eq!(after_unstake, later, "an unstaked pet must stop earning");
        assert_eq!(contract.get_pool().total_weight.0, 0);
        assert!(!contract.is_staked(1));
    }

    #[test]
    fn only_the_router_can_grant_credits() {
        let mut contract = deploy();

        testing_env!(ctx(owner(), 0).build());
        contract.fund_reserve(U128(ONE_CREDIT));

        testing_env!(ctx(router(), 10).build());
        assert_eq!(contract.grant_credits(alice(), U128(ONE_CREDIT)).0, ONE_CREDIT);
        assert_eq!(
            contract.get_account(alice()).unwrap().credit_balance.0,
            ONE_CREDIT
        );
    }

    #[test]
    #[should_panic(expected = "Only the mint router")]
    fn players_cannot_grant_themselves_credits() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.fund_reserve(U128(ONE_CREDIT));

        testing_env!(ctx(alice(), 10).build());
        contract.grant_credits(alice(), U128(ONE_CREDIT));
    }

    #[test]
    fn a_grant_beyond_the_reserve_is_capped_and_reported() {
        let mut contract = deploy();

        testing_env!(ctx(owner(), 0).build());
        contract.fund_reserve(U128(ONE_CREDIT / 2));

        testing_env!(ctx(router(), 10).build());
        // The router refunds the difference based on what came back.
        assert_eq!(
            contract.grant_credits(alice(), U128(ONE_CREDIT)).0,
            ONE_CREDIT / 2
        );
        assert_eq!(contract.get_pool().sale_reserve.0, 0);
    }

    #[test]
    fn an_empty_reserve_grants_nothing() {
        let mut contract = deploy();
        testing_env!(ctx(router(), 10).build());
        assert_eq!(contract.grant_credits(alice(), U128(ONE_CREDIT)).0, 0);
    }

    // ── Drops ────────────────────────────────────────────────────────────────

    #[test]
    fn stronger_pets_find_more_and_better() {
        // A pet with no weight finds nothing at all.
        assert_eq!(drop_odds(0), (0, 0));

        let (weak_chance, weak_tier) = drop_odds(100); // fresh common
        let (mid_chance, mid_tier) = drop_odds(800); // levelled epic
        let (max_chance, max_tier) = drop_odds(2_240); // maxed legendary

        assert!(weak_chance < mid_chance && mid_chance < max_chance);
        assert!(weak_tier < mid_tier && mid_tier < max_tier);

        // The curve has to span the weights pets actually reach, or "stronger
        // pets find more" is true only on paper.
        assert_eq!(weak_chance, 300); // 3%
        assert_eq!(max_chance, 2_440); // 24.4%
        assert_eq!(max_tier, DROPPABLE_ITEMS.len() - 1);

        // A stable of pets is capped rather than guaranteed a drop every time.
        assert_eq!(drop_odds(100_000).0, DROP_MAX_BPS);
    }

    #[test]
    fn the_odds_view_tells_the_truth() {
        let mut contract = deploy();
        stake_at_zero(&mut contract, alice(), 1);

        testing_env!(ctx(alice(), 10).build());
        let odds = contract.get_drop_odds(alice());
        assert!(odds["chance_bps"].as_u64().unwrap() > DROP_BASE_BPS as u64);
        assert!(!odds["can_drop"].as_array().unwrap().is_empty());

        // Someone with no pets is told so rather than shown a teaser.
        let none = contract.get_drop_odds(bob());
        assert_eq!(none["chance_bps"].as_u64().unwrap(), 0);
    }

    // ── Mining ───────────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Stake a pet")]
    fn mining_needs_a_working_pet() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0).build());
        contract.fund_reserve(U128(ONE_CREDIT));

        testing_env!(ctx(alice(), 10).build());
        contract.claim_mining();
    }

    #[test]
    fn mining_pays_then_waits() {
        let mut contract = deploy();
        stake_at_zero(&mut contract, alice(), 1);

        testing_env!(ctx(owner(), 0).build());
        contract.fund_reserve(U128(ONE_CREDIT));

        testing_env!(ctx(alice(), 10).build());
        let paid = contract.claim_mining().0;
        assert_eq!(paid, ONE_CREDIT / 200);
        assert!(contract.mining_available_at(alice()).0 > 0);
    }

    #[test]
    #[should_panic(expected = "cooldown")]
    fn mining_cannot_be_spammed() {
        let mut contract = deploy();
        stake_at_zero(&mut contract, alice(), 1);

        testing_env!(ctx(owner(), 0).build());
        contract.fund_reserve(U128(ONE_CREDIT));

        testing_env!(ctx(alice(), 10).build());
        contract.claim_mining();
        contract.claim_mining();
    }

    // ── Marketplace ──────────────────────────────────────────────────────────

    /// Give an account items without relying on a random drop.
    fn give_items(contract: &mut PetStake, who: AccountId, item_id: u16, quantity: u32) {
        testing_env!(ctx(who.clone(), EPOCH_SECONDS).build());
        contract.buy_credit_item(item_id, quantity);
    }

    #[test]
    fn items_change_hands_for_credits() {
        let mut contract = deploy();
        stake_at_zero(&mut contract, alice(), 1);
        stake_at_zero(&mut contract, bob(), 2);

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.claim_credits();
        testing_env!(ctx(bob(), EPOCH_SECONDS).build());
        contract.claim_credits();

        give_items(&mut contract, alice(), ITEM_WARD, 2);
        assert_eq!(contract.get_inventory(alice()), vec![(ITEM_WARD, 2)]);

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        let listing_id = contract.list_item(ITEM_WARD, 2, U128(100_000));
        // Listed items leave the seller's inventory straight away.
        assert!(contract.get_inventory(alice()).is_empty());
        assert_eq!(contract.get_listings(None, None).len(), 1);

        let seller_before = contract.get_account(alice()).unwrap().credit_balance.0;
        let buyer_before = contract.get_account(bob()).unwrap().credit_balance.0;
        let reserve_before = contract.get_pool().sale_reserve.0;

        testing_env!(ctx(bob(), EPOCH_SECONDS).build());
        let paid = contract.buy_listing(listing_id, 1).0;

        assert_eq!(paid, 100_000);
        assert_eq!(contract.get_inventory(bob()), vec![(ITEM_WARD, 1)]);
        // 5% rake leaves circulation; the rest reaches the seller.
        assert_eq!(
            contract.get_account(alice()).unwrap().credit_balance.0,
            seller_before + 95_000
        );
        assert_eq!(
            contract.get_account(bob()).unwrap().credit_balance.0,
            buyer_before - 100_000
        );
        assert_eq!(contract.get_pool().sale_reserve.0, reserve_before + 5_000);

        // One of the two is still for sale.
        assert_eq!(contract.get_listings(None, None)[0].quantity, 1);
    }

    #[test]
    fn cancelling_returns_the_items() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);
        give_items(&mut contract, alice(), ITEM_LENS, 3);

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        let id = contract.list_item(ITEM_LENS, 3, U128(50_000));
        contract.cancel_listing(id);

        assert_eq!(contract.get_inventory(alice()), vec![(ITEM_LENS, 3)]);
        assert!(contract.get_listings(None, None).is_empty());
    }

    #[test]
    #[should_panic(expected = "listing is yours")]
    fn cannot_buy_your_own_listing() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);
        give_items(&mut contract, alice(), ITEM_TONIC, 1);

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        let id = contract.list_item(ITEM_TONIC, 1, U128(10_000));
        contract.buy_listing(id, 1);
    }

    #[test]
    #[should_panic(expected = "do not have that many")]
    fn cannot_list_items_you_do_not_hold() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);
        give_items(&mut contract, alice(), ITEM_TONIC, 1);

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.list_item(ITEM_TONIC, 5, U128(10_000));
    }

    #[test]
    #[should_panic(expected = "Not that many for sale")]
    fn cannot_buy_more_than_is_listed() {
        let mut contract = deploy();
        stake_at_zero(&mut contract, alice(), 1);
        stake_at_zero(&mut contract, bob(), 2);
        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.claim_credits();
        testing_env!(ctx(bob(), EPOCH_SECONDS).build());
        contract.claim_credits();

        give_items(&mut contract, alice(), ITEM_TONIC, 1);
        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        let id = contract.list_item(ITEM_TONIC, 1, U128(10_000));

        testing_env!(ctx(bob(), EPOCH_SECONDS).build());
        contract.buy_listing(id, 2);
    }

    #[test]
    fn faucet_grants_once_per_account() {
        let mut contract = deploy();

        testing_env!(ctx(owner(), 0).build());
        contract.fund_reserve(U128(ONE_CREDIT));

        testing_env!(ctx(alice(), 100).build());
        assert_eq!(contract.claim_faucet().0, ONE_CREDIT / 100);
        assert_eq!(
            contract.get_account(alice()).unwrap().credit_balance.0,
            ONE_CREDIT / 100
        );
    }

    #[test]
    #[should_panic(expected = "already used the faucet")]
    fn faucet_cannot_be_farmed_by_waiting() {
        let mut contract = deploy();

        testing_env!(ctx(owner(), 0).build());
        contract.fund_reserve(U128(ONE_CREDIT));

        testing_env!(ctx(alice(), 100).build());
        contract.claim_faucet();

        // A week later it is still one grant per account, not one per day.
        testing_env!(ctx(alice(), 100 + 7 * 24 * 60 * 60).build());
        contract.claim_faucet();
    }

    // ── Credit items ─────────────────────────────────────────────────────────

    /// Stake a pet at t=0. Every pet must be staked before the clock moves, or
    /// the ones added later accrue nothing and have nothing to claim.
    fn stake_at_zero(contract: &mut PetStake, who: AccountId, pet_id: u64) {
        testing_env!(ctx(owner(), 0).build());
        contract.admin_set_pet(pet_id, who, attrs(10, 2, 2, Rarity::Epic));
    }

    /// Stake one pet, run an epoch, claim — the account now has credits.
    fn funded(contract: &mut PetStake, who: AccountId, pet_id: u64) {
        stake_at_zero(contract, who.clone(), pet_id);
        testing_env!(ctx(who, EPOCH_SECONDS).build());
        contract.claim_credits();
    }

    /// Two players, each with a staked pet and a claimed balance, both out of
    /// the lab so they are free to fight.
    fn two_fighters(contract: &mut PetStake) {
        stake_at_zero(contract, alice(), 1);
        stake_at_zero(contract, bob(), 2);

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.claim_credits();
        contract.unstake_pet(1);

        testing_env!(ctx(bob(), EPOCH_SECONDS).build());
        contract.claim_credits();
        contract.unstake_pet(2);
    }

    #[test]
    fn ward_makes_a_pet_unattackable_and_extends() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);

        // Unstake so the pet is attackable to begin with.
        contract.unstake_pet(1);
        assert!(contract.is_attackable(1));

        contract.buy_credit_item(ITEM_WARD, 1);
        contract.use_item(ITEM_WARD, 1);
        let state = contract.get_pet_state(1);
        assert!(!state.attackable);
        assert!(!contract.is_attackable(1));

        let first_until = state.shielded_until.0;

        // A second ward stacks on top instead of restarting from now.
        contract.buy_credit_item(ITEM_WARD, 1);
        contract.use_item(ITEM_WARD, 1);
        let second_until = contract.get_pet_state(1).shielded_until.0;
        assert_eq!(second_until, first_until + WARD_SECONDS * NANOS_PER_SECOND);
    }

    #[test]
    fn ward_expires_on_its_own() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);
        contract.unstake_pet(1);
        contract.buy_credit_item(ITEM_WARD, 1);
        contract.use_item(ITEM_WARD, 1);

        assert!(!contract.is_attackable(1));

        // A day and a second later.
        testing_env!(ctx(alice(), EPOCH_SECONDS + WARD_SECONDS + 1).build());
        assert!(contract.is_attackable(1));
    }

    #[test]
    fn lens_applies_to_feeds_then_runs_out() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);

        contract.buy_credit_item(ITEM_LENS, 1);
        contract.use_item(ITEM_LENS, 1);
        assert_eq!(contract.get_pet_state(1).lens_feeds_left, LENS_FEEDS);

        for _ in 0..LENS_FEEDS {
            contract.spend_credits("ai_feed".to_string(), Some(1), U128(2_000));
        }
        assert_eq!(contract.get_pet_state(1).lens_feeds_left, 0);

        // Feeding past the lens must not underflow the counter.
        contract.spend_credits("ai_feed".to_string(), Some(1), U128(2_000));
        assert_eq!(contract.get_pet_state(1).lens_feeds_left, 0);
    }

    #[test]
    #[should_panic(expected = "Unknown item")]
    fn rejects_unknown_items() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);
        contract.buy_credit_item(999, 1);
    }

    #[test]
    #[should_panic(expected = "do not have that item")]
    fn cannot_use_an_item_you_never_bought() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);
        contract.use_item(ITEM_WARD, 1);
    }

    #[test]
    #[should_panic(expected = "belongs to someone else")]
    fn cannot_use_items_on_someone_elses_staked_pet() {
        let mut contract = deploy();
        stake_at_zero(&mut contract, alice(), 1);
        stake_at_zero(&mut contract, bob(), 2);

        testing_env!(ctx(bob(), EPOCH_SECONDS).build());
        contract.claim_credits();
        contract.buy_credit_item(ITEM_WARD, 1);
        contract.use_item(ITEM_WARD, 1);
    }

    #[test]
    fn staked_pet_cannot_be_attacked() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);
        assert!(contract.get_pet_state(1).staked);
        assert!(!contract.is_attackable(1));
    }

    // ── Battles ──────────────────────────────────────────────────────────────

    #[test]
    fn battle_moves_credits_and_wounds_the_loser() {
        let mut contract = deploy();
        two_fighters(&mut contract);

        let before_winner = contract.get_account(alice()).unwrap().credit_balance.0;
        let before_loser = contract.get_account(bob()).unwrap().credit_balance.0;

        testing_env!(ctx(game(), EPOCH_SECONDS).build());
        let outcome = contract.report_battle(1, alice(), 2, bob());

        // 0.01 credit pot, 5% rake.
        assert_eq!(outcome.rake.0, 500);
        assert_eq!(outcome.transferred.0, 9_500);
        assert!(!outcome.star_protected);

        assert_eq!(
            contract.get_account(alice()).unwrap().credit_balance.0,
            before_winner + 9_500
        );
        assert_eq!(
            contract.get_account(bob()).unwrap().credit_balance.0,
            before_loser - 10_000
        );

        // Rake tops up the reserve rather than diluting the staking stream.
        assert_eq!(contract.get_pool().sale_reserve.0, 500);

        // The loser is wounded, not dead — and cannot be attacked again at once.
        let state = contract.get_pet_state(2);
        assert!(state.wounded_until.0 > 0);
        assert!(!contract.is_attackable(2));
    }

    #[test]
    fn charm_absorbs_the_star_loss_once() {
        let mut contract = deploy();
        two_fighters(&mut contract);

        contract.buy_credit_item(ITEM_CHARM, 1);
        contract.use_item(ITEM_CHARM, 2);
        assert!(contract.get_pet_state(2).star_guard);

        testing_env!(ctx(game(), EPOCH_SECONDS).build());
        assert!(contract.report_battle(1, alice(), 2, bob()).star_protected);
        assert!(!contract.get_pet_state(2).star_guard, "the charm is consumed");
    }

    #[test]
    #[should_panic(expected = "protected right now")]
    fn battle_rejects_a_warded_pet() {
        let mut contract = deploy();
        two_fighters(&mut contract);

        contract.buy_credit_item(ITEM_WARD, 1);
        contract.use_item(ITEM_WARD, 2);

        testing_env!(ctx(game(), EPOCH_SECONDS).build());
        contract.report_battle(1, alice(), 2, bob());
    }

    #[test]
    fn tonic_clears_a_wound() {
        let mut contract = deploy();
        two_fighters(&mut contract);

        testing_env!(ctx(game(), EPOCH_SECONDS).build());
        contract.report_battle(1, alice(), 2, bob());
        assert!(!contract.is_attackable(2));

        testing_env!(ctx(bob(), EPOCH_SECONDS).build());
        contract.buy_credit_item(ITEM_TONIC, 1);
        contract.use_item(ITEM_TONIC, 2);
        assert_eq!(contract.get_pet_state(2).wounded_until.0, 0);
        assert!(contract.is_attackable(2));
    }

    #[test]
    #[should_panic(expected = "Only the game contract")]
    fn players_cannot_settle_their_own_battles() {
        let mut contract = deploy();
        two_fighters(&mut contract);

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.report_battle(1, alice(), 2, bob());
    }

    #[test]
    #[should_panic(expected = "same owner")]
    fn cannot_farm_credits_by_fighting_yourself() {
        let mut contract = deploy();
        stake_at_zero(&mut contract, alice(), 1);
        stake_at_zero(&mut contract, alice(), 2);

        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.claim_credits();
        contract.unstake_pet(1);
        contract.unstake_pet(2);

        testing_env!(ctx(game(), EPOCH_SECONDS).build());
        contract.report_battle(1, alice(), 2, alice());
    }

    #[test]
    fn a_broke_loser_still_loses_the_fight() {
        let mut contract = deploy();

        // Bob never staked, so he has no credits at all.
        funded(&mut contract, alice(), 1);
        testing_env!(ctx(alice(), EPOCH_SECONDS).build());
        contract.unstake_pet(1);

        testing_env!(ctx(game(), EPOCH_SECONDS).build());
        let outcome = contract.report_battle(1, alice(), 2, bob());

        assert_eq!(outcome.transferred.0, 0, "nothing to take");
        assert_eq!(outcome.rake.0, 0);
        assert!(contract.get_pet_state(2).wounded_until.0 > 0, "still wounded");
    }
}
