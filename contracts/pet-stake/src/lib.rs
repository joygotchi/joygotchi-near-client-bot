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

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::{LookupMap, UnorderedMap};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde_json::json;
use near_sdk::{
    env, ext_contract, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault,
    Promise, PromiseError, PromiseOrValue,
};

pub use types::*;

const GAS_FOR_METADATA: Gas = Gas::from_tgas(10);
const GAS_FOR_RESOLVE: Gas = Gas::from_tgas(15);

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Accounts,
    Pets,
    Prices,
    PetStates,
    ItemPrices,
}

/// The view this contract needs from the game contract. A deployment whose game
/// contract does not expose it can use `admin_set_pet` instead.
#[ext_contract(ext_game)]
#[allow(dead_code)]
trait Game {
    fn get_pet_attrs(&self, pet_id: u64) -> (AccountId, PetAttrs);
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
    /// $JOY token, the only FT this contract accepts.
    joy_token: AccountId,

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

    /// Credits set aside for $JOY conversions and the faucet. Separate from the
    /// staking stream so selling credits can never overdraw the real NEAR AI
    /// balance that backs them.
    sale_reserve: u128,
    /// Whole $JOY (in token units) needed for one credit.
    joy_per_credit: u128,
    faucet_amount: u128,

    paused: bool,
}

#[near]
impl PetStake {
    #[init]
    pub fn new(
        owner_id: AccountId,
        game_contract: AccountId,
        joy_token: AccountId,
        epoch_budget: U128,
    ) -> Self {
        let mut contract = Self {
            owner_id,
            game_contract,
            joy_token,
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
            // 400 JOY (18 decimals) for one credit — deliberately a bad deal.
            joy_per_credit: 400 * 10u128.pow(18),
            // One-off grant, not a daily one: NEAR implicit accounts are nearly
            // free to create, and 0.05/day per account would let a script with
            // a thousand accounts drain a $500 monthly budget three times over.
            faucet_amount: ONE_CREDIT / 100, // 0.01 credit, once per account
            paused: false,
        };

        // Keep these in step with src/lib/economy/params.ts.
        for (action, credits) in [
            ("ai_feed", 2_000u128),
            ("train", 8_000),
            ("evolve", 500_000),
            ("craft_item", 20_000),
            ("chat", 1_000),
            ("revive", 1_000_000),
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

    fn internal_stake(&mut self, pet_id: u64, account_id: AccountId, attrs: PetAttrs) {
        self.update_pool();

        let weight = attrs.weight();
        let mut account = self.account_or_default(&account_id);
        self.harvest(&mut account);

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

        self.log_event(
            "pet_staked",
            json!({ "account_id": account_id, "pet_id": pet_id, "weight": U128(weight) }),
        );
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

        self.log_event(
            "credit_spent",
            json!({
                "account_id": account_id,
                "action": action,
                "pet_id": pet_id,
                "amount": U128(price),
                "lens_applied": lens_applied,
            }),
        );

        U128(price)
    }

    // ── Credit items ─────────────────────────────────────────────────────────

    fn pet_state_or_default(&self, pet_id: u64) -> PetState {
        self.pet_states.get(&pet_id).unwrap_or_default()
    }

    /// Buy one credit item for a pet.
    ///
    /// When the pet is staked here we can check ownership and do. For an
    /// unstaked pet we cannot without a cross-contract call, so we allow it:
    /// the only thing at stake is the buyer's own credits, and every effect is
    /// beneficial to the pet.
    pub fn buy_credit_item(&mut self, item_id: u16, pet_id: u64) -> U128 {
        self.assert_running();
        self.update_pool();

        let price = self.item_prices.get(&item_id).expect("Unknown item");
        let account_id = env::predecessor_account_id();

        if let Some(pet) = self.pets.get(&pet_id) {
            require!(pet.owner_id == account_id, "That pet belongs to someone else");
        }

        let mut account = self.account_or_default(&account_id);
        self.harvest(&mut account);
        require!(
            account.credit_balance >= price,
            "Not enough credits — claim or convert some first"
        );
        account.credit_balance -= price;
        self.accounts.insert(&account_id, &account);
        self.total_spent += price;

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
            "item_bought",
            json!({
                "account_id": account_id,
                "pet_id": pet_id,
                "item_id": item_id,
                "credits": U128(price),
            }),
        );

        U128(price)
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
        self.accounts.insert(&winner_id, &winner);

        // Rake funds the reserve behind $JOY conversions and the faucet, so PvP
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
    /// willing to sell for $JOY or give away through the faucet.
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

    pub fn set_joy_per_credit(&mut self, joy_per_credit: U128) {
        self.assert_owner();
        require!(joy_per_credit.0 > 0, "Rate must be positive");
        self.joy_per_credit = joy_per_credit.0;
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

    /// Escape hatch for a game contract that predates `get_pet_attrs`: an
    /// operator pushes the pet's owner and attributes instead.
    pub fn admin_set_pet(&mut self, pet_id: u64, owner_id: AccountId, attrs: PetAttrs) {
        self.assert_owner();
        if let Some(existing) = self.pets.get(&pet_id) {
            require!(
                existing.owner_id == owner_id,
                "Unstake the pet before moving it to another owner"
            );
            // Re-stake with the new weight.
            self.update_pool();
            let mut account = self.account_or_default(&owner_id);
            self.harvest(&mut account);
            account.weight = account.weight.saturating_sub(existing.weight.0);
            self.total_weight = self.total_weight.saturating_sub(existing.weight.0);
            account.staked.retain(|id| *id != pet_id);
            self.accounts.insert(&owner_id, &account);
            self.pets.remove(&pet_id);
        }
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
            "joy_token": self.joy_token,
            "joy_per_credit": U128(self.joy_per_credit),
            "faucet_amount": U128(self.faucet_amount),
            "credit_decimals": CREDIT_DECIMALS,
            "battle_wager": U128(self.battle_wager),
            "battle_rake_bps": self.battle_rake_bps,
            "ward_seconds": WARD_SECONDS,
            "wound_seconds": WOUND_SECONDS,
            "lens_feeds": LENS_FEEDS,
        })
    }
}

// ── $JOY -> credits ──────────────────────────────────────────────────────────

#[near]
impl FungibleTokenReceiver for PetStake {
    /// One-way conversion: $JOY in, credits out, at a deliberately poor rate.
    /// Credits are real compute, so they must never be convertible back into a
    /// tradeable token — that is what keeps them from becoming a second
    /// speculative asset with a redemption promise behind it.
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        require!(
            env::predecessor_account_id() == self.joy_token,
            "This contract only accepts $JOY"
        );
        self.assert_running();

        let action: FtAction = near_sdk::serde_json::from_str(&msg)
            .unwrap_or_else(|_| env::panic_str("msg must be {\"action\":\"buy_credits\"}"));

        match action {
            FtAction::BuyCredits => {
                let credits = amount.0 * ONE_CREDIT / self.joy_per_credit;
                require!(credits > 0, "Too little $JOY for a single credit unit");

                if credits > self.sale_reserve {
                    env::log_str("Credit reserve is empty — refunding the $JOY");
                    return PromiseOrValue::Value(amount);
                }

                self.sale_reserve -= credits;

                let mut account = self.account_or_default(&sender_id);
                account.credit_balance += credits;
                self.accounts.insert(&sender_id, &account);

                self.log_event(
                    "credits_bought",
                    json!({ "account_id": sender_id, "joy": amount, "credits": U128(credits) }),
                );

                // The $JOY stays here as a sink; the treasury burns it later.
                PromiseOrValue::Value(U128(0))
            }
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
    fn joy() -> AccountId {
        "joy.near".parse().unwrap()
    }
    fn alice() -> AccountId {
        "alice.near".parse().unwrap()
    }
    fn bob() -> AccountId {
        "bob.near".parse().unwrap()
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
        PetStake::new(owner(), game(), joy(), U128(500 * ONE_CREDIT))
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
    fn joy_converts_to_credits_only_while_the_reserve_holds() {
        let mut contract = deploy();

        testing_env!(ctx(owner(), 0).build());
        contract.fund_reserve(U128(ONE_CREDIT)); // one credit in reserve

        // 400 JOY buys exactly one credit.
        let four_hundred_joy = U128(400 * 10u128.pow(18));
        testing_env!(ctx(joy(), 10).build());
        let refund = contract.ft_on_transfer(
            alice(),
            four_hundred_joy,
            "{\"action\":\"buy_credits\"}".to_string(),
        );
        match refund {
            PromiseOrValue::Value(value) => assert_eq!(value.0, 0, "nothing should be refunded"),
            _ => panic!("expected an immediate value"),
        }
        assert_eq!(
            contract.get_account(alice()).unwrap().credit_balance.0,
            ONE_CREDIT
        );

        // Reserve is empty now, so the next conversion refunds the $JOY.
        testing_env!(ctx(joy(), 20).build());
        let refund = contract.ft_on_transfer(
            bob(),
            four_hundred_joy,
            "{\"action\":\"buy_credits\"}".to_string(),
        );
        match refund {
            PromiseOrValue::Value(value) => {
                assert_eq!(value.0, four_hundred_joy.0, "the $JOY must come back")
            }
            _ => panic!("expected an immediate value"),
        }
    }

    #[test]
    #[should_panic(expected = "only accepts $JOY")]
    fn rejects_other_tokens() {
        let mut contract = deploy();
        testing_env!(ctx("usdc.near".parse().unwrap(), 10).build());
        let _ = contract.ft_on_transfer(alice(), U128(1), "{\"action\":\"buy_credits\"}".to_string());
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
        let state = contract.get_pet_state(1);
        assert!(!state.attackable);
        assert!(!contract.is_attackable(1));

        let first_until = state.shielded_until.0;

        // A second ward stacks on top instead of restarting from now.
        contract.buy_credit_item(ITEM_WARD, 1);
        let second_until = contract.get_pet_state(1).shielded_until.0;
        assert_eq!(second_until, first_until + WARD_SECONDS * NANOS_PER_SECOND);
    }

    #[test]
    fn ward_expires_on_its_own() {
        let mut contract = deploy();
        funded(&mut contract, alice(), 1);
        contract.unstake_pet(1);
        contract.buy_credit_item(ITEM_WARD, 1);

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
    #[should_panic(expected = "belongs to someone else")]
    fn cannot_buy_items_for_someone_elses_staked_pet() {
        let mut contract = deploy();
        stake_at_zero(&mut contract, alice(), 1);
        stake_at_zero(&mut contract, bob(), 2);

        testing_env!(ctx(bob(), EPOCH_SECONDS).build());
        contract.claim_credits();
        contract.buy_credit_item(ITEM_WARD, 1);
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

        contract.buy_credit_item(ITEM_CHARM, 2);
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

        contract.buy_credit_item(ITEM_WARD, 2);

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
        contract.buy_credit_item(ITEM_TONIC, 2);
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
