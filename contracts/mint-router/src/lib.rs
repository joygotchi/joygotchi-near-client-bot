//! Multichain mint receiver.
//!
//! A player pays in ZEC, SOL, BTC or anything else NEAR Intents supports. The
//! 1Click service swaps it and withdraws wNEAR to this contract with
//! `ft_transfer_call`, carrying the `customRecipientMsg` the app set on the
//! quote. This contract turns that payment into a pet (or into $JOY).
//!
//! The payment arrives from a bridge, not from the player's own wallet, so the
//! player never signs anything on NEAR for the mint. That means this contract
//! has to be careful: the only thing it trusts is the wNEAR transfer itself,
//! and every mint it cannot complete immediately is kept as a pending claim
//! instead of being dropped.

use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::Deserialize;
use near_sdk::serde_json::json;
use near_sdk::{
    env, ext_contract, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault,
    Promise, PromiseError, PromiseOrValue,
};

// Kept modest on purpose: `ft_on_transfer` runs on gas the bridge attached, not
// gas we chose. A mint that runs out of gas stays claimable through
// `redeem_pending`, so being frugal here costs nothing.
const GAS_FOR_MINT: Gas = Gas::from_tgas(30);
const GAS_FOR_RESOLVE: Gas = Gas::from_tgas(10);
const GAS_FOR_JOY_MINT: Gas = Gas::from_tgas(20);
const GAS_FOR_JOY_RESOLVE: Gas = Gas::from_tgas(5);
const GAS_FOR_FT_TRANSFER: Gas = Gas::from_tgas(15);

const ALLOWED_CATEGORIES: [&str; 2] = ["blackdragon", "greendragon"];
const MAX_QUANTITY: u32 = 10;

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Pending,
}

/// What the app put in `customRecipientMsg` when it asked 1Click for a quote.
#[derive(Deserialize)]
#[serde(crate = "near_sdk::serde", tag = "action", rename_all = "snake_case")]
enum PaymentMsg {
    MintPet {
        account_id: AccountId,
        category: String,
        name: String,
        #[serde(default = "one")]
        quantity: u32,
    },
    BuyJoy {
        account_id: AccountId,
    },
}

fn one() -> u32 {
    1
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct PendingMint {
    pub category: String,
    pub name: String,
    pub paid: U128,
    pub created_at: U64,
}

/// What this contract needs from the game contract: mint a pet and assign it to
/// somebody other than the caller, because the caller here is a bridge.
#[ext_contract(ext_game)]
#[allow(dead_code)]
trait Game {
    fn create_pet_for(&mut self, owner_id: AccountId, name: String, category: String) -> u64;
}

#[ext_contract(ext_ft)]
#[allow(dead_code)]
trait Ft {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    fn mint(&mut self, account_id: AccountId, amount: U128, memo: Option<String>);
}

#[ext_contract(ext_self)]
#[allow(dead_code)]
trait SelfCallbacks {
    fn on_minted(&mut self, account_id: AccountId, index: u64) -> bool;
    fn on_joy_minted(&mut self, account_id: AccountId, refund: U128) -> U128;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct MintRouter {
    owner_id: AccountId,
    game_contract: AccountId,
    joy_token: AccountId,
    /// wNEAR — the only token 1Click settles into for us.
    wnear: AccountId,
    treasury: AccountId,

    /// Price of one pet, in yoctoNEAR of wNEAR.
    mint_price: u128,
    /// $JOY (in token units) handed out per 1 wNEAR for `buy_joy`.
    joy_per_wnear: u128,

    /// Mints that have been paid for but not yet created on the game contract.
    pending: UnorderedMap<AccountId, Vec<PendingMint>>,

    total_minted: u64,
    total_received: u128,
}

#[near]
impl MintRouter {
    #[init]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        owner_id: AccountId,
        game_contract: AccountId,
        joy_token: AccountId,
        wnear: AccountId,
        treasury: AccountId,
        mint_price: U128,
        joy_per_wnear: U128,
    ) -> Self {
        Self {
            owner_id,
            game_contract,
            joy_token,
            wnear,
            treasury,
            mint_price: mint_price.0,
            joy_per_wnear: joy_per_wnear.0,
            pending: UnorderedMap::new(StorageKey::Pending),
            total_minted: 0,
            total_received: 0,
        }
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

    // ── Minting ──────────────────────────────────────────────────────────────

    fn queue_mint(&mut self, account_id: &AccountId, mint: PendingMint) -> u64 {
        let mut queue = self.pending.get(account_id).unwrap_or_default();
        queue.push(mint);
        let index = (queue.len() - 1) as u64;
        self.pending.insert(account_id, &queue);
        index
    }

    fn start_mint(&mut self, account_id: AccountId, index: u64) -> Promise {
        let queue = self.pending.get(&account_id).unwrap_or_default();
        let mint = queue
            .get(index as usize)
            .expect("No pending mint at that index")
            .clone();

        ext_game::ext(self.game_contract.clone())
            .with_static_gas(GAS_FOR_MINT)
            .create_pet_for(account_id.clone(), mint.name, mint.category)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE)
                    .on_minted(account_id, index),
            )
    }

    #[private]
    pub fn on_minted(
        &mut self,
        account_id: AccountId,
        index: u64,
        #[callback_result] result: Result<u64, PromiseError>,
    ) -> bool {
        match result {
            Ok(pet_id) => {
                let mut queue = self.pending.get(&account_id).unwrap_or_default();
                if (index as usize) < queue.len() {
                    queue.remove(index as usize);
                    if queue.is_empty() {
                        self.pending.remove(&account_id);
                    } else {
                        self.pending.insert(&account_id, &queue);
                    }
                }
                self.total_minted += 1;
                self.log_event(
                    "pet_minted",
                    json!({ "account_id": account_id, "pet_id": pet_id }),
                );
                true
            }
            Err(_) => {
                // The payment is already recorded, so keep the claim alive
                // rather than losing it.
                self.log_event(
                    "mint_deferred",
                    json!({ "account_id": account_id, "index": U64(index) }),
                );
                false
            }
        }
    }

    /// Retry a mint that the game contract rejected earlier — for example
    /// because it was paused when the payment landed. Anyone may call this; it
    /// can only ever mint a pet that has already been paid for.
    pub fn redeem_pending(&mut self, account_id: AccountId, index: U64) -> Promise {
        self.start_mint(account_id, index.0)
    }

    // ── Views ────────────────────────────────────────────────────────────────

    pub fn get_pending(&self, account_id: AccountId) -> Vec<PendingMint> {
        self.pending.get(&account_id).unwrap_or_default()
    }

    pub fn get_stats(&self) -> near_sdk::serde_json::Value {
        json!({
            "mint_price": U128(self.mint_price),
            "joy_per_wnear": U128(self.joy_per_wnear),
            "total_minted": U64(self.total_minted),
            "total_received": U128(self.total_received),
            "pending_accounts": self.pending.len(),
            "game_contract": self.game_contract,
            "wnear": self.wnear,
            "treasury": self.treasury,
        })
    }

    // ── Owner ────────────────────────────────────────────────────────────────

    pub fn set_mint_price(&mut self, mint_price: U128) {
        self.assert_owner();
        require!(mint_price.0 > 0, "Price must be positive");
        self.mint_price = mint_price.0;
    }

    pub fn set_joy_per_wnear(&mut self, joy_per_wnear: U128) {
        self.assert_owner();
        self.joy_per_wnear = joy_per_wnear.0;
    }

    pub fn set_game_contract(&mut self, game_contract: AccountId) {
        self.assert_owner();
        self.game_contract = game_contract;
    }

    /// Forward collected wNEAR to the treasury, which is what gets staked with
    /// NEAR AI to produce the credit stream the AI Lab distributes.
    pub fn sweep(&mut self, amount: U128) -> Promise {
        self.assert_owner();
        ext_ft::ext(self.wnear.clone())
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .ft_transfer(self.treasury.clone(), amount, Some("mint revenue".to_string()))
    }
}

#[near]
impl FungibleTokenReceiver for MintRouter {
    /// Entry point for every multichain payment. 1Click calls this with the
    /// swapped wNEAR and the message the app attached to the quote.
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        require!(
            env::predecessor_account_id() == self.wnear,
            "This contract only accepts wNEAR"
        );

        let Ok(payload) = near_sdk::serde_json::from_str::<PaymentMsg>(&msg) else {
            env::log_str("Unrecognised payment message — refunding");
            return PromiseOrValue::Value(amount);
        };

        match payload {
            PaymentMsg::MintPet {
                account_id,
                category,
                name,
                quantity,
            } => {
                let quantity = quantity.clamp(1, MAX_QUANTITY);

                if !ALLOWED_CATEGORIES.contains(&category.as_str()) {
                    env::log_str("Unknown pet category — refunding");
                    return PromiseOrValue::Value(amount);
                }

                let required = self.mint_price * quantity as u128;
                if amount.0 < required {
                    env::log_str("Payment is below the mint price — refunding");
                    return PromiseOrValue::Value(amount);
                }

                self.total_received += required;

                let name = name.chars().take(32).collect::<String>();
                let mut first_index = None;
                for _ in 0..quantity {
                    let index = self.queue_mint(
                        &account_id,
                        PendingMint {
                            category: category.clone(),
                            name: name.clone(),
                            paid: U128(self.mint_price),
                            created_at: U64(env::block_timestamp()),
                        },
                    );
                    if first_index.is_none() {
                        first_index = Some(index);
                    }
                }

                self.log_event(
                    "mint_paid",
                    json!({
                        "account_id": account_id,
                        "sender_id": sender_id,
                        "category": category,
                        "quantity": quantity,
                        "paid": U128(required),
                    }),
                );

                // Kick off the first mint; the rest stay claimable through
                // `redeem_pending`, which keeps this call's gas predictable.
                //
                // The promise is scheduled as soon as it is built, so dropping
                // the handle here still runs the mint. We deliberately return a
                // value rather than the promise: the value is the refund amount
                // NEP-141 expects, and any overpayment has to go back to 1Click
                // so it reaches the player's refund address on the origin chain.
                drop(self.start_mint(account_id, first_index.unwrap_or(0)));
                PromiseOrValue::Value(U128(amount.0 - required))
            }

            PaymentMsg::BuyJoy { account_id } => {
                let one_near = 10u128.pow(24);
                let joy = amount.0 * self.joy_per_wnear / one_near;
                if joy == 0 {
                    env::log_str("Payment is too small for a single $JOY — refunding");
                    return PromiseOrValue::Value(amount);
                }

                self.log_event(
                    "joy_paid",
                    json!({ "account_id": account_id, "wnear": amount, "joy": U128(joy) }),
                );

                PromiseOrValue::Promise(
                    ext_ft::ext(self.joy_token.clone())
                        .with_static_gas(GAS_FOR_JOY_MINT)
                        .mint(
                            account_id.clone(),
                            U128(joy),
                            Some("multichain top up".to_string()),
                        )
                        .then(
                            ext_self::ext(env::current_account_id())
                                .with_static_gas(GAS_FOR_JOY_RESOLVE)
                                .on_joy_minted(account_id, amount),
                        ),
                )
            }
        }
    }
}

#[near]
impl MintRouter {
    /// Terminates the `buy_joy` chain with the amount of wNEAR to refund: none
    /// when the $JOY was minted, all of it when it was not. Without this the
    /// player could pay and receive nothing.
    #[private]
    pub fn on_joy_minted(
        &mut self,
        account_id: AccountId,
        refund: U128,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> U128 {
        match result {
            Ok(()) => {
                self.total_received += refund.0;
                U128(0)
            }
            Err(_) => {
                self.log_event(
                    "joy_mint_failed",
                    json!({ "account_id": account_id, "refunded": refund }),
                );
                refund
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
    fn wnear() -> AccountId {
        "wrap.near".parse().unwrap()
    }
    fn treasury() -> AccountId {
        "treasury.near".parse().unwrap()
    }
    fn alice() -> AccountId {
        "alice.near".parse().unwrap()
    }
    /// 1Click's withdrawal account.
    fn bridge() -> AccountId {
        "intents.near".parse().unwrap()
    }

    const ONE_NEAR: u128 = 10u128.pow(24);

    fn ctx(predecessor: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id("mint.near".parse().unwrap())
            .predecessor_account_id(predecessor)
            .block_timestamp(1_000);
        builder
    }

    fn deploy() -> MintRouter {
        testing_env!(ctx(owner()).build());
        MintRouter::new(
            owner(),
            game(),
            joy(),
            wnear(),
            treasury(),
            U128(2 * ONE_NEAR),
            U128(1_000 * 10u128.pow(18)),
        )
    }

    fn mint_msg(quantity: u32) -> String {
        json!({
            "action": "mint_pet",
            "account_id": "alice.near",
            "category": "blackdragon",
            "name": "SMAUG",
            "quantity": quantity,
        })
        .to_string()
    }

    fn value_of(result: PromiseOrValue<U128>) -> u128 {
        match result {
            PromiseOrValue::Value(value) => value.0,
            PromiseOrValue::Promise(_) => panic!("expected an immediate value"),
        }
    }

    #[test]
    fn exact_payment_queues_one_mint_and_refunds_nothing() {
        let mut contract = deploy();
        testing_env!(ctx(wnear()).build());

        let change = value_of(contract.ft_on_transfer(bridge(), U128(2 * ONE_NEAR), mint_msg(1)));

        assert_eq!(change, 0);
        let pending = contract.get_pending(alice());
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].category, "blackdragon");
        assert_eq!(pending[0].name, "SMAUG");
    }

    #[test]
    fn overpayment_is_returned_as_change() {
        let mut contract = deploy();
        testing_env!(ctx(wnear()).build());

        let change = value_of(contract.ft_on_transfer(bridge(), U128(3 * ONE_NEAR), mint_msg(1)));

        assert_eq!(change, ONE_NEAR);
    }

    #[test]
    fn underpayment_is_refunded_in_full_and_mints_nothing() {
        let mut contract = deploy();
        testing_env!(ctx(wnear()).build());

        let refund = value_of(contract.ft_on_transfer(bridge(), U128(ONE_NEAR), mint_msg(1)));

        assert_eq!(refund, ONE_NEAR);
        assert!(contract.get_pending(alice()).is_empty());
    }

    #[test]
    fn bulk_payment_queues_every_pet() {
        let mut contract = deploy();
        testing_env!(ctx(wnear()).build());

        let change = value_of(contract.ft_on_transfer(bridge(), U128(6 * ONE_NEAR), mint_msg(3)));

        assert_eq!(change, 0);
        assert_eq!(contract.get_pending(alice()).len(), 3);
    }

    #[test]
    fn unknown_category_is_refunded() {
        let mut contract = deploy();
        testing_env!(ctx(wnear()).build());

        let msg = json!({
            "action": "mint_pet",
            "account_id": "alice.near",
            "category": "unicorn",
            "name": "NOPE",
        })
        .to_string();

        let refund = value_of(contract.ft_on_transfer(bridge(), U128(2 * ONE_NEAR), msg));
        assert_eq!(refund, 2 * ONE_NEAR);
        assert!(contract.get_pending(alice()).is_empty());
    }

    #[test]
    fn garbled_message_is_refunded() {
        let mut contract = deploy();
        testing_env!(ctx(wnear()).build());

        let refund = value_of(contract.ft_on_transfer(bridge(), U128(2 * ONE_NEAR), "{}".to_string()));
        assert_eq!(refund, 2 * ONE_NEAR);
    }

    #[test]
    #[should_panic(expected = "only accepts wNEAR")]
    fn rejects_other_tokens() {
        let mut contract = deploy();
        testing_env!(ctx(joy()).build());
        let _ = contract.ft_on_transfer(bridge(), U128(2 * ONE_NEAR), mint_msg(1));
    }

    #[test]
    fn successful_callback_clears_the_pending_entry() {
        let mut contract = deploy();
        testing_env!(ctx(wnear()).build());
        let _ = contract.ft_on_transfer(bridge(), U128(2 * ONE_NEAR), mint_msg(1));

        testing_env!(ctx("mint.near".parse().unwrap()).build());
        assert!(contract.on_minted(alice(), 0, Ok(42)));
        assert!(contract.get_pending(alice()).is_empty());
    }

    #[test]
    fn failed_callback_keeps_the_claim() {
        let mut contract = deploy();
        testing_env!(ctx(wnear()).build());
        let _ = contract.ft_on_transfer(bridge(), U128(2 * ONE_NEAR), mint_msg(1));

        testing_env!(ctx("mint.near".parse().unwrap()).build());
        assert!(!contract.on_minted(alice(), 0, Err(PromiseError::Failed)));
        assert_eq!(contract.get_pending(alice()).len(), 1, "the paid mint must survive");
    }
}
