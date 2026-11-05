//! $JOY — the JOYGOTCHI soft currency.
//!
//! A plain NEP-141 token with a minter allow-list. Pets, items and mining
//! rewards are paid in $JOY; the hard resource (AI credit) lives in the
//! `pet-stake` contract and is deliberately not a token. See docs/ECONOMY.md.

use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::{FungibleToken, FungibleTokenCore};
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::LookupSet;
use near_sdk::json_types::U128;
use near_sdk::{
    env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault, PromiseOrValue,
};

const DECIMALS: u8 = 18;
const DATA_IMAGE_SVG_ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Ccircle cx='16' cy='16' r='15' fill='%23f48f59'/%3E%3Ctext x='16' y='22' font-size='16' text-anchor='middle' fill='%232d3c53' font-family='monospace'%3EJ%3C/text%3E%3C/svg%3E";

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Minters,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct JoyToken {
    token: FungibleToken,
    metadata: FungibleTokenMetadata,
    owner_id: AccountId,
    /// Accounts allowed to call `mint`: the game contract, the faucet, the
    /// mint router. Kept small and explicit.
    minters: LookupSet<AccountId>,
}

#[near]
impl JoyToken {
    #[init]
    pub fn new(owner_id: AccountId, total_supply: U128) -> Self {
        let metadata = FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name: "JOYGOTCHI".to_string(),
            symbol: "JOY".to_string(),
            icon: Some(DATA_IMAGE_SVG_ICON.to_string()),
            reference: None,
            reference_hash: None,
            decimals: DECIMALS,
        };
        metadata.assert_valid();

        let mut contract = Self {
            token: FungibleToken::new(b"a".to_vec()),
            metadata,
            owner_id: owner_id.clone(),
            minters: LookupSet::new(StorageKey::Minters),
        };

        contract.token.internal_register_account(&owner_id);
        contract.token.internal_deposit(&owner_id, total_supply.0);
        contract.minters.insert(&owner_id);

        near_contract_standards::fungible_token::events::FtMint {
            owner_id: &owner_id,
            amount: total_supply,
            memo: Some("initial supply"),
        }
        .emit();

        contract
    }

    // ── Owner ────────────────────────────────────────────────────────────────

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only the owner can call this"
        );
    }

    pub fn add_minter(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.minters.insert(&account_id);
    }

    pub fn remove_minter(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.minters.remove(&account_id);
    }

    pub fn is_minter(&self, account_id: AccountId) -> bool {
        self.minters.contains(&account_id)
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn set_owner(&mut self, owner_id: AccountId) {
        self.assert_owner();
        self.owner_id = owner_id;
    }

    // ── Supply ───────────────────────────────────────────────────────────────

    /// Mint $JOY. Only an allow-listed minter can call this, and the receiver
    /// must already be registered for storage.
    pub fn mint(&mut self, account_id: AccountId, amount: U128, memo: Option<String>) {
        let caller = env::predecessor_account_id();
        require!(
            caller == self.owner_id || self.minters.contains(&caller),
            "Not allowed to mint"
        );

        if !self.token.accounts.contains_key(&account_id) {
            self.token.internal_register_account(&account_id);
        }
        self.token.internal_deposit(&account_id, amount.0);

        near_contract_standards::fungible_token::events::FtMint {
            owner_id: &account_id,
            amount,
            memo: memo.as_deref(),
        }
        .emit();
    }

    /// Burn the caller's own $JOY. Item purchases and credit conversions use
    /// this so every sink is visible on-chain.
    pub fn burn(&mut self, amount: U128, memo: Option<String>) {
        let account_id = env::predecessor_account_id();
        self.token.internal_withdraw(&account_id, amount.0);

        near_contract_standards::fungible_token::events::FtBurn {
            owner_id: &account_id,
            amount,
            memo: memo.as_deref(),
        }
        .emit();
    }
}

// ── NEP-141 ──────────────────────────────────────────────────────────────────

#[near]
impl FungibleTokenCore for JoyToken {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.token.ft_transfer(receiver_id, amount, memo);
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near]
impl StorageManagement for JoyToken {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        self.token.storage_withdraw(amount)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        #[allow(unused_variables)]
        if let Some((account_id, balance)) = self.token.internal_storage_unregister(force) {
            near_contract_standards::fungible_token::events::FtBurn {
                owner_id: &account_id,
                amount: U128(balance),
                memo: Some("storage unregister"),
            }
            .emit();
            true
        } else {
            false
        }
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.token.storage_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenMetadataProvider for JoyToken {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.clone()
    }
}

/// Resolve a failed `ft_transfer_call`, as required by NEP-141.
#[near]
impl JoyToken {
    #[private]
    pub fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        let (used, burned) = self
            .token
            .internal_ft_resolve_transfer(&sender_id, receiver_id, amount);
        if burned > 0 {
            near_contract_standards::fungible_token::events::FtBurn {
                owner_id: &sender_id,
                amount: U128(burned),
                memo: Some("refund on failed ft_transfer_call"),
            }
            .emit();
        }
        U128(used)
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

    fn context(predecessor: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor);
        builder
    }

    #[test]
    fn mints_initial_supply_to_owner() {
        testing_env!(context(owner()).build());
        let contract = JoyToken::new(owner(), U128(1_000));
        assert_eq!(contract.ft_total_supply(), U128(1_000));
        assert_eq!(contract.ft_balance_of(owner()), U128(1_000));
    }

    #[test]
    #[should_panic(expected = "Not allowed to mint")]
    fn rejects_mint_from_stranger() {
        testing_env!(context(owner()).build());
        let mut contract = JoyToken::new(owner(), U128(1_000));

        testing_env!(context("stranger.near".parse().unwrap()).build());
        contract.mint(game(), U128(1), None);
    }

    #[test]
    fn allow_listed_minter_can_mint() {
        testing_env!(context(owner()).build());
        let mut contract = JoyToken::new(owner(), U128(1_000));
        contract.add_minter(game());

        testing_env!(context(game()).build());
        contract.mint(game(), U128(500), Some("mining".to_string()));

        assert_eq!(contract.ft_balance_of(game()), U128(500));
        assert_eq!(contract.ft_total_supply(), U128(1_500));
    }

    #[test]
    fn burn_reduces_supply() {
        testing_env!(context(owner()).build());
        let mut contract = JoyToken::new(owner(), U128(1_000));
        contract.burn(U128(400), None);

        assert_eq!(contract.ft_total_supply(), U128(600));
        assert_eq!(contract.ft_balance_of(owner()), U128(600));
    }
}
