//! JOYGOTCHI pets, v2.
//!
//! A NEP-171 collection with the game on top. Written to replace
//! `game.joyv1.near`, which is not NEP-171, has no combat stats, and stores a
//! status field it only refreshes when something touches the pet.
//!
//! The interface this implements is specified in docs/GAME_CONTRACT_V2.md.
//! Four things in it exist solely because another contract calls them:
//! `get_pet_attrs` and `create_pet_for` (called by `pet-stake` and
//! `mint-router`), `set_locked` (called by `pet-stake` when a pet starts or
//! stops work), and `attack`, which settles through `pet-stake::report_battle`.
//!
//! Five open decisions from the spec are resolved here. Each is marked
//! `DECISION:` where it is implemented, so they are easy to find and change.

mod types;

use near_contract_standards::non_fungible_token::metadata::{
    NFTContractMetadata, NonFungibleTokenMetadataProvider, TokenMetadata, NFT_METADATA_SPEC,
};
use near_contract_standards::non_fungible_token::{NonFungibleToken, Token, TokenId};
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::{LookupMap, UnorderedMap};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde_json::json;
use near_sdk::{
    env, ext_contract, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault,
    Promise, PromiseError, PromiseOrValue,
};

pub use types::*;

const GAS_FOR_BATTLE: Gas = Gas::from_tgas(40);
const GAS_FOR_BATTLE_RESOLVE: Gas = Gas::from_tgas(20);
const GAS_FOR_SYNC: Gas = Gas::from_tgas(30);

/// Species a pet can be minted as. The art lives at
/// `/assets/animation/<category>/<evolution_phase>.gif`.
const CATEGORIES: [&str; 2] = ["blackdragon", "greendragon"];

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    NonFungibleToken,
    TokenMetadata,
    Enumeration,
    Approval,
    Pets,
    Shop,
    Listings,
}

/// What this contract needs from the staking contract.
#[ext_contract(ext_stake)]
#[allow(dead_code)]
trait Stake {
    fn report_battle(
        &mut self,
        winner_pet_id: u64,
        winner_id: AccountId,
        loser_pet_id: u64,
        loser_id: AccountId,
    ) -> BattleOutcome;

    /// Re-read a staked pet's attributes after it levels up or evolves.
    fn sync_pet(&mut self, pet_id: u64);
}

#[ext_contract(ext_self)]
#[allow(dead_code)]
trait SelfCallbacks {
    fn on_battle_settled(&mut self, winner_pet_id: u64, loser_pet_id: u64) -> bool;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Game {
    tokens: NonFungibleToken,
    metadata: NFTContractMetadata,

    owner_id: AccountId,
    /// Allowed to call `set_locked`, and the contract battles settle through.
    stake_contract: AccountId,
    /// Allowed to call `create_pet_for`.
    mint_router: AccountId,

    pets: UnorderedMap<u64, Pet>,
    next_pet_id: u64,

    /// The shop, priced in yoctoNEAR.
    shop: LookupMap<u16, ShopItem>,
    shop_ids: Vec<u16>,

    /// What `create_pet` costs, in yoctoNEAR.
    mint_price: u128,

    /// Pets for sale, priced in NEAR.
    listings: UnorderedMap<u64, PetListing>,
    /// Marketplace cut, in basis points, paid to the treasury.
    market_fee_bps: u16,
    /// Where mint revenue and marketplace fees go.
    treasury: AccountId,

    paused: bool,
}

#[near]
impl Game {
    #[init]
    pub fn new(
        owner_id: AccountId,
        stake_contract: AccountId,
        mint_router: AccountId,
        treasury: AccountId,
        mint_price: U128,
    ) -> Self {
        let metadata = NFTContractMetadata {
            spec: NFT_METADATA_SPEC.to_string(),
            name: "JOYGOTCHI".to_string(),
            symbol: "JOY".to_string(),
            icon: None,
            base_uri: None,
            reference: None,
            reference_hash: None,
        };
        metadata.assert_valid();

        let mut contract = Self {
            tokens: NonFungibleToken::new(
                StorageKey::NonFungibleToken,
                owner_id.clone(),
                Some(StorageKey::TokenMetadata),
                Some(StorageKey::Enumeration),
                Some(StorageKey::Approval),
            ),
            metadata,
            owner_id,
            stake_contract,
            mint_router,
            pets: UnorderedMap::new(StorageKey::Pets),
            next_pet_id: 1,
            shop: LookupMap::new(StorageKey::Shop),
            shop_ids: Vec::new(),
            mint_price: mint_price.0,
            listings: UnorderedMap::new(StorageKey::Listings),
            market_fee_bps: DEFAULT_MARKET_FEE_BPS,
            treasury,
            paused: false,
        };

        // DECISION 4: the shop is repriced in NEAR rather than retired. The v1
        // ids and names are kept so existing clients and explorers still make
        // sense; only the currency changes.
        for (item_id, name, price_near, time_extension, xp, shield, is_revival) in [
            (1u16, "Beef", 10u128, 3 * 3_600u64, 30u32, 0u32, false),
            (2, "Water", 5, 3 * 3_600, 10, 0, false),
            (3, "Shield", 20, 0, 0, 1, false),
            (4, "Holy Water", 50, 3 * 3_600, 0, 0, true),
        ] {
            contract.shop.insert(
                &item_id,
                &ShopItem {
                    item_id,
                    name: name.to_string(),
                    // Prices are in hundredths of a NEAR: Beef is 0.10 NEAR.
                    price: U128(price_near * 10u128.pow(22)),
                    time_extension,
                    xp,
                    shield,
                    is_revival,
                },
            );
            contract.shop_ids.push(item_id);
        }

        contract
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn assert_owner(&self) {
        require!(
            env::predecessor_account_id() == self.owner_id,
            "Only the owner can call this"
        );
    }

    fn assert_running(&self) {
        require!(!self.paused, "The game is paused");
    }

    fn expect_pet(&self, pet_id: u64) -> Pet {
        self.pets.get(&pet_id).expect("No such pet")
    }

    fn assert_pet_owner(&self, pet: &Pet) {
        require!(
            pet.owner_id == env::predecessor_account_id(),
            "That pet belongs to someone else"
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

    /// A uniform value in `0..range`, from the block's random seed.
    ///
    /// Good enough for loot and a combat jitter. A validator can in principle
    /// influence it, so nothing of real value may hang on a single roll — the
    /// battle formula is therefore mostly deterministic, with randomness only
    /// as a tie-breaker band.
    fn roll(range: u32, salt: u8) -> u32 {
        let seed = env::random_seed();
        let a = *seed.first().unwrap_or(&0) as u32;
        let b = *seed.get(1).unwrap_or(&0) as u32;
        let c = *seed.get(2).unwrap_or(&0) as u32;
        (((a << 16) | (b << 8) | c) ^ salt as u32) % range.max(1)
    }

    fn token_metadata(pet: &Pet) -> TokenMetadata {
        TokenMetadata {
            title: Some(pet.name.clone()),
            description: Some(format!(
                "A {:?} {} · level {} · {} stars",
                pet.rarity, pet.category, pet.level, pet.star
            )),
            media: Some(format!(
                "/assets/animation/{}/{}.gif",
                pet.category, pet.evolution_phase
            )),
            media_hash: None,
            copies: Some(1),
            issued_at: Some(pet.born_at.to_string()),
            expires_at: None,
            starts_at: None,
            updated_at: None,
            extra: None,
            reference: None,
            reference_hash: None,
        }
    }

    /// Tell the staking contract to re-weight a pet whose attributes changed.
    ///
    /// Only worth doing while the pet is at work; `sync_pet` panics otherwise.
    /// The promise is fire-and-forget: a failed sync leaves the old weight in
    /// place, which pays slightly wrong until the next change rather than
    /// breaking anything.
    fn sync_weight(&self, pet: &Pet) {
        if !pet.locked {
            return;
        }
        ext_stake::ext(self.stake_contract.clone())
            .with_static_gas(GAS_FOR_SYNC)
            .sync_pet(pet.pet_id)
            .detach();
    }

    // ── Minting ──────────────────────────────────────────────────────────────

    fn internal_mint(&mut self, owner_id: AccountId, name: String, category: String, salt: u8) -> u64 {
        require!(
            CATEGORIES.contains(&category.as_str()),
            "Unknown pet category"
        );

        let pet_id = self.next_pet_id;
        self.next_pet_id += 1;

        // DECISION 5: rarity is rolled at mint with published odds, so minting
        // another pet is a real decision rather than a formality. Odds live in
        // `Rarity::ODDS_BPS` and are readable through `get_mint_odds`.
        let rarity = Rarity::from_roll(Self::roll(10_000, salt));
        let (atk, def) = rarity.base_stats();
        let now = env::block_timestamp();

        let pet = Pet {
            pet_id,
            owner_id: owner_id.clone(),
            name: name.chars().take(32).collect(),
            category,
            rarity,
            level: 0,
            star: 0,
            xp: 0,
            evolution_phase: 1,
            atk,
            def,
            starves_at: now + STARVE_SECONDS * NANOS_PER_SECOND,
            born_at: now,
            last_attack_used: 0,
            last_attacked: 0,
            shield: 0,
            win_streak: 0,
            locked: false,
        };

        self.tokens.internal_mint_with_refund(
            pet_id.to_string(),
            owner_id.clone(),
            Some(Self::token_metadata(&pet)),
            None,
        );
        self.pets.insert(&pet_id, &pet);

        self.log_event(
            "pet_minted",
            json!({
                "pet_id": pet_id,
                "owner_id": owner_id,
                "rarity": pet.rarity,
                "category": pet.category,
            }),
        );

        pet_id
    }

    /// Mint for yourself. The deposit covers the mint price plus NEP-171
    /// storage; anything over is refunded by the standard.
    #[payable]
    pub fn create_pet(&mut self, name: String, category: String) -> u64 {
        self.assert_running();
        require!(
            env::attached_deposit().as_yoctonear() >= self.mint_price,
            "Attach at least the mint price"
        );
        self.internal_mint(env::predecessor_account_id(), name, category, 1)
    }

    /// Mint for somebody else.
    ///
    /// Only the mint router may call this. A multichain payment arrives from a
    /// bridge rather than from the player's wallet, so the caller is never the
    /// owner and ownership has to be passed in.
    pub fn create_pet_for(
        &mut self,
        owner_id: AccountId,
        name: String,
        category: String,
    ) -> u64 {
        self.assert_running();
        let caller = env::predecessor_account_id();
        require!(
            caller == self.mint_router || caller == self.owner_id,
            "Only the mint router can mint for another account"
        );
        self.internal_mint(owner_id, name, category, 2)
    }

    // ── Care ─────────────────────────────────────────────────────────────────

    pub fn change_name_pet(&mut self, pet_id: u64, name: String) {
        let mut pet = self.expect_pet(pet_id);
        self.assert_pet_owner(&pet);
        pet.name = name.chars().take(32).collect();
        self.pets.insert(&pet_id, &pet);
    }

    /// Buy a shop item and apply it immediately.
    ///
    /// The attached deposit is the price. Feeding moves the starving deadline
    /// and may grant XP, which can level the pet — so this is also one of the
    /// places that has to re-sync staking weight.
    #[payable]
    pub fn buy_item(&mut self, pet_id: u64, item_id: u16) {
        self.assert_running();
        let item = self.shop.get(&item_id).expect("Unknown item");
        require!(
            env::attached_deposit().as_yoctonear() >= item.price.0,
            "Attach at least the item price"
        );

        let mut pet = self.expect_pet(pet_id);
        self.assert_pet_owner(&pet);

        let now = env::block_timestamp();
        if !pet.is_alive(now) {
            require!(item.is_revival, "This pet has starved — use a revival item");
        }

        // Banked food is capped, so a pet cannot be made immortal in one go.
        let from = pet.starves_at.max(now);
        let extended = from + item.time_extension * NANOS_PER_SECOND;
        let ceiling = now + MAX_STARVE_SECONDS * NANOS_PER_SECOND;
        pet.starves_at = extended.min(ceiling);
        pet.shield = pet.shield.saturating_add(item.shield);

        let levelled = self.grant_xp(&mut pet, item.xp);
        self.pets.insert(&pet_id, &pet);

        self.log_event(
            "item_used",
            json!({
                "pet_id": pet_id,
                "item_id": item_id,
                "paid": item.price,
                "levelled": levelled,
            }),
        );

        if levelled {
            self.sync_weight(&pet);
        }
    }

    /// Add XP and level up as far as it goes. Returns whether the level moved.
    fn grant_xp(&self, pet: &mut Pet, xp: u32) -> bool {
        if xp == 0 {
            return false;
        }
        pet.xp = pet.xp.saturating_add(xp);

        let before = pet.level;
        while pet.level < MAX_LEVEL {
            let need = pet.xp_for_next_level();
            if pet.xp < need {
                break;
            }
            pet.xp -= need;
            pet.level += 1;
            // Stats grow with level so combat keeps pace with weight.
            pet.atk += 4;
            pet.def += 3;
        }

        if pet.level != before {
            self.log_event(
                "pet_levelled",
                json!({ "pet_id": pet.pet_id, "level": pet.level }),
            );
        }
        pet.level != before
    }

    /// Grant XP from an off-chain action the staking contract charged for,
    /// such as an AI feed. Only that contract may call it.
    pub fn award_xp(&mut self, pet_id: u64, xp: u32) {
        require!(
            env::predecessor_account_id() == self.stake_contract
                || env::predecessor_account_id() == self.owner_id,
            "Only the staking contract can award XP"
        );
        let mut pet = self.expect_pet(pet_id);
        let levelled = self.grant_xp(&mut pet, xp.min(500));
        self.pets.insert(&pet_id, &pet);
        if levelled {
            self.sync_weight(&pet);
        }
    }

    /// Advance a pet to its next evolution phase. Raises staking weight, and
    /// therefore both income and drop rate, permanently.
    pub fn evolve(&mut self, pet_id: u64) {
        self.assert_running();
        let mut pet = self.expect_pet(pet_id);
        self.assert_pet_owner(&pet);
        require!(
            pet.evolution_phase < MAX_EVOLUTION_PHASE,
            "This pet is fully evolved"
        );
        require!(
            pet.level >= pet.evolution_phase as u32 * 5,
            "Not enough levels to evolve yet"
        );

        pet.evolution_phase += 1;
        self.pets.insert(&pet_id, &pet);
        self.tokens.token_metadata_by_id.as_mut().map(|by_id| {
            by_id.insert(&pet_id.to_string(), &Self::token_metadata(&pet))
        });

        self.log_event(
            "pet_evolved",
            json!({ "pet_id": pet_id, "phase": pet.evolution_phase }),
        );
        self.sync_weight(&pet);
    }

    // ── Marketplace ──────────────────────────────────────────────────────────

    /// Offer a pet for sale, priced in NEAR.
    ///
    /// Listings live here rather than in a separate marketplace contract on
    /// purpose. This contract already owns transfers and the staking lock, so a
    /// sale is one atomic call — there is no window in which a pet gets staked
    /// between being listed and being bought, and no escrow hop to go wrong.
    #[payable]
    pub fn list_pet(&mut self, pet_id: u64, price: U128) {
        self.assert_running();
        require!(
            env::attached_deposit() >= NearToken::from_yoctonear(1),
            "Attach 1 yoctoNEAR"
        );
        require!(price.0 > 0, "Set a price");

        let pet = self.expect_pet(pet_id);
        self.assert_pet_owner(&pet);
        require!(
            !pet.locked,
            "This pet is at work in the AI Lab — unstake it before selling"
        );

        self.listings.insert(
            &pet_id,
            &PetListing {
                pet_id,
                seller_id: pet.owner_id.clone(),
                price,
                listed_at: U64(env::block_timestamp()),
            },
        );

        self.log_event(
            "pet_listed",
            json!({ "pet_id": pet_id, "seller_id": pet.owner_id, "price": price }),
        );
    }

    #[payable]
    pub fn unlist_pet(&mut self, pet_id: u64) {
        require!(
            env::attached_deposit() >= NearToken::from_yoctonear(1),
            "Attach 1 yoctoNEAR"
        );
        let listing = self.listings.get(&pet_id).expect("That pet is not for sale");
        require!(
            listing.seller_id == env::predecessor_account_id(),
            "That listing is not yours"
        );
        self.internal_unlist(pet_id, "unlisted");
    }

    fn internal_unlist(&mut self, pet_id: u64, reason: &str) {
        if self.listings.remove(&pet_id).is_some() {
            self.log_event("pet_unlisted", json!({ "pet_id": pet_id, "reason": reason }));
        }
    }

    /// Buy a listed pet. The deposit must cover the price; change is returned.
    #[payable]
    pub fn buy_pet(&mut self, pet_id: u64) -> Promise {
        self.assert_running();

        let listing = self.listings.get(&pet_id).expect("That pet is not for sale");
        let buyer_id = env::predecessor_account_id();
        require!(listing.seller_id != buyer_id, "That listing is yours");

        let paid = env::attached_deposit().as_yoctonear();
        require!(paid >= listing.price.0, "Attach at least the asking price");

        let mut pet = self.expect_pet(pet_id);
        // Nothing should be able to lock a pet between listing and sale, but a
        // sale is irreversible, so check rather than assume.
        require!(!pet.locked, "This pet is at work in the AI Lab");

        let fee = listing.price.0 * self.market_fee_bps as u128 / 10_000;
        let to_seller = listing.price.0 - fee;
        let change = paid - listing.price.0;

        // Move the token and the game's own owner field together.
        self.tokens.internal_transfer_unguarded(
            &pet_id.to_string(),
            &listing.seller_id,
            &buyer_id,
        );
        pet.owner_id = buyer_id.clone();
        self.pets.insert(&pet_id, &pet);
        self.listings.remove(&pet_id);

        self.log_event(
            "pet_sold",
            json!({
                "pet_id": pet_id,
                "seller_id": listing.seller_id,
                "buyer_id": buyer_id,
                "price": listing.price,
                "fee": U128(fee),
            }),
        );

        // The fee funds the treasury, which is what gets staked with NEAR AI —
        // so trading volume feeds the credit stream too.
        let mut payout = Promise::new(listing.seller_id.clone())
            .transfer(NearToken::from_yoctonear(to_seller));
        if fee > 0 {
            payout = payout
                .then(Promise::new(self.treasury.clone()).transfer(NearToken::from_yoctonear(fee)));
        }
        if change > 0 {
            payout =
                payout.then(Promise::new(buyer_id).transfer(NearToken::from_yoctonear(change)));
        }
        payout
    }

    pub fn get_pet_listing(&self, pet_id: u64) -> Option<PetListing> {
        self.listings.get(&pet_id)
    }

    pub fn get_pet_listings(&self, from_index: Option<u64>, limit: Option<u64>) -> Vec<PetListing> {
        self.listings
            .values()
            .skip(from_index.unwrap_or(0) as usize)
            .take(limit.unwrap_or(50) as usize)
            .collect()
    }

    pub fn set_market_fee_bps(&mut self, fee_bps: u16) {
        self.assert_owner();
        require!(fee_bps <= 1_000, "Fee cannot exceed 10%");
        self.market_fee_bps = fee_bps;
    }

    pub fn set_treasury(&mut self, treasury: AccountId) {
        self.assert_owner();
        self.treasury = treasury;
    }

    // ── Battle ───────────────────────────────────────────────────────────────

    /// Attack another pet.
    ///
    /// DECISION 2: the winner is decided here, mostly deterministically. Each
    /// side's score is its effective attack against the other's effective
    /// defence, plus a ±15% band rolled from the block seed. A player can see
    /// why they lost — the scores go into the event — while the band stops the
    /// matchup being perfectly solvable in advance.
    ///
    /// The settlement call comes first and the damage is applied in the
    /// callback. The other order looks equivalent and is not: if
    /// `report_battle` reverts — which it does for any pet that is staked,
    /// warded or wounded — a protected pet would already have been hurt.
    pub fn attack(&mut self, pet_id: u64, target_pet_id: u64) -> Promise {
        self.assert_running();
        require!(pet_id != target_pet_id, "A pet cannot fight itself");

        let mine = self.expect_pet(pet_id);
        let target = self.expect_pet(target_pet_id);
        self.assert_pet_owner(&mine);
        require!(
            mine.owner_id != target.owner_id,
            "Both pets have the same owner"
        );

        let now = env::block_timestamp();
        require!(mine.is_alive(now), "Your pet has starved — feed it first");
        require!(target.is_alive(now), "That pet has already starved");
        require!(!mine.locked, "This pet is at work in the AI Lab");
        require!(!target.locked, "That pet is at work in the AI Lab");

        require!(
            now.saturating_sub(mine.last_attack_used)
                >= ATTACK_COOLDOWN_SECONDS * NANOS_PER_SECOND
                || mine.last_attack_used == 0,
            "Your pet is still recovering from its last attack"
        );
        require!(
            now.saturating_sub(target.last_attacked)
                >= ATTACKED_COOLDOWN_SECONDS * NANOS_PER_SECOND
                || target.last_attacked == 0,
            "That pet was attacked too recently"
        );

        let (attacker_score, defender_score) = Self::battle_scores(&mine, &target);
        let attacker_wins = attacker_score >= defender_score;

        let (winner_pet_id, winner_id, loser_pet_id, loser_id) = if attacker_wins {
            (pet_id, mine.owner_id.clone(), target_pet_id, target.owner_id.clone())
        } else {
            (target_pet_id, target.owner_id.clone(), pet_id, mine.owner_id.clone())
        };

        self.log_event(
            "battle_rolled",
            json!({
                "pet_id": pet_id,
                "target_pet_id": target_pet_id,
                "attacker_score": U64(attacker_score),
                "defender_score": U64(defender_score),
                "winner_pet_id": winner_pet_id,
            }),
        );

        ext_stake::ext(self.stake_contract.clone())
            .with_static_gas(GAS_FOR_BATTLE)
            .report_battle(winner_pet_id, winner_id, loser_pet_id, loser_id)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_BATTLE_RESOLVE)
                    .on_battle_settled(winner_pet_id, loser_pet_id),
            )
    }

    /// Both sides' scores, with the random band applied.
    fn battle_scores(attacker: &Pet, defender: &Pet) -> (u64, u64) {
        let band = 30; // ±15%
        let attacker_roll = 85 + Self::roll(band + 1, 3) as u64;
        let defender_roll = 85 + Self::roll(band + 1, 5) as u64;

        let attacker_score = attacker.effective_atk() * attacker_roll / 100;
        let defender_score = defender.effective_def() * defender_roll / 100;
        (attacker_score, defender_score)
    }

    /// Apply the consequences once the credits have actually moved.
    #[private]
    pub fn on_battle_settled(
        &mut self,
        winner_pet_id: u64,
        loser_pet_id: u64,
        #[callback_result] result: Result<BattleOutcome, PromiseError>,
    ) -> bool {
        let Ok(outcome) = result else {
            // The settlement reverted — most often because the target turned
            // out to be protected. Nothing has been damaged, which is the
            // whole reason this runs after the call rather than before it.
            self.log_event(
                "battle_rejected",
                json!({ "winner_pet_id": winner_pet_id, "loser_pet_id": loser_pet_id }),
            );
            return false;
        };

        let now = env::block_timestamp();

        if let Some(mut winner) = self.pets.get(&winner_pet_id) {
            winner.last_attack_used = now;
            let levelled = self.grant_xp(&mut winner, 25);

            // Stars are earned here and nowhere else. Three wins in a row is
            // one star, and a single defeat takes it back — so the ladder is
            // worth climbing and worth defending.
            let mut starred = false;
            if winner.star < MAX_STAR {
                winner.win_streak += 1;
                if winner.win_streak >= WINS_PER_STAR {
                    winner.win_streak = 0;
                    winner.star += 1;
                    starred = true;
                    self.log_event(
                        "star_earned",
                        json!({ "pet_id": winner_pet_id, "star": winner.star }),
                    );
                }
            }

            self.pets.insert(&winner_pet_id, &winner);
            // A star is +15% staking weight, so it has to be re-read there.
            if levelled || starred {
                self.sync_weight(&winner);
            }
        }

        if let Some(mut loser) = self.pets.get(&loser_pet_id) {
            loser.last_attacked = now;
            // Any progress towards the next star is lost with the fight.
            loser.win_streak = 0;

            if loser.shield > 0 {
                // A Shield absorbs the hit itself, before anything else.
                loser.shield -= 1;
            } else if !outcome.star_protected && loser.star > 0 {
                loser.star -= 1;
            }

            // Losing costs time on the clock, not the pet's life. Pets never
            // die from a fight — see docs/ECONOMY.md.
            let penalty = 3 * 3_600 * NANOS_PER_SECOND;
            loser.starves_at = loser.starves_at.saturating_sub(penalty).max(now);

            self.pets.insert(&loser_pet_id, &loser);
            self.sync_weight(&loser);
        }

        self.log_event(
            "battle_settled",
            json!({
                "winner_pet_id": winner_pet_id,
                "loser_pet_id": loser_pet_id,
                "transferred": outcome.transferred,
                "star_protected": outcome.star_protected,
            }),
        );

        true
    }

    // ── Staking lock ─────────────────────────────────────────────────────────

    /// Mark a pet as at work, or back home.
    ///
    /// DECISION 3: a locked pet cannot be transferred or approved. The flag is
    /// mirrored here rather than read from `pet-stake`, because a transfer has
    /// to be atomic and a cross-contract view cannot be. `pet-stake` is the
    /// only account allowed to set it.
    pub fn set_locked(&mut self, pet_id: u64, locked: bool) {
        require!(
            env::predecessor_account_id() == self.stake_contract
                || env::predecessor_account_id() == self.owner_id,
            "Only the staking contract can lock a pet"
        );
        let mut pet = self.expect_pet(pet_id);
        pet.locked = locked;
        self.pets.insert(&pet_id, &pet);

        // A pet that goes to work stops being for sale. Leaving the listing up
        // would sell a buyer something they cannot use or move.
        if locked {
            self.internal_unlist(pet_id, "staked");
        }

        self.log_event("pet_lock_changed", json!({ "pet_id": pet_id, "locked": locked }));
    }

    fn assert_transferable(&self, token_id: &TokenId) {
        let pet_id: u64 = token_id.parse().expect("Bad token id");
        let pet = self.expect_pet(pet_id);
        require!(
            !pet.locked,
            "This pet is at work in the AI Lab — unstake it before transferring"
        );
    }

    // ── Views ────────────────────────────────────────────────────────────────

    /// Owner and the attributes staking weight is built from.
    ///
    /// `pet-stake` calls this with 10 TGas from `stake_pet` and `sync_pet`,
    /// and refuses the stake when the owner does not match the caller.
    pub fn get_pet_attrs(&self, pet_id: u64) -> (AccountId, PetAttrs) {
        let pet = self.expect_pet(pet_id);
        let attrs = pet.attrs();
        (pet.owner_id, attrs)
    }

    pub fn get_pet(&self, pet_id: u64) -> Option<PetView> {
        self.pets.get(&pet_id).map(|pet| self.to_view(&pet))
    }

    fn to_view(&self, pet: &Pet) -> PetView {
        let now = env::block_timestamp();
        PetView {
            pet_id: pet.pet_id,
            owner_id: pet.owner_id.clone(),
            name: pet.name.clone(),
            category: pet.category.clone(),
            rarity: pet.rarity,
            level: pet.level,
            star: pet.star,
            xp: pet.xp,
            xp_for_next_level: pet.xp_for_next_level(),
            evolution_phase: pet.evolution_phase,
            atk: pet.atk,
            def: pet.def,
            effective_atk: U64(pet.effective_atk()),
            effective_def: U64(pet.effective_def()),
            starves_at: U64(pet.starves_at),
            seconds_until_starving: U64(pet.starves_at.saturating_sub(now) / NANOS_PER_SECOND),
            status: pet.status(now),
            shield: pet.shield,
            listed_price: self.listings.get(&pet.pet_id).map(|l| l.price),
            win_streak: pet.win_streak,
            wins_to_next_star: if pet.star >= MAX_STAR {
                None
            } else {
                Some(WINS_PER_STAR - pet.win_streak)
            },
            locked: pet.locked,
            born_at: U64(pet.born_at),
            last_attack_used: U64(pet.last_attack_used),
            last_attacked: U64(pet.last_attacked),
        }
    }

    /// Every pet. Kept for compatibility with the v1 client, which reads the
    /// whole set and filters locally.
    pub fn get_all_pet_metadata(&self) -> Vec<PetView> {
        self.pets.values().map(|pet| self.to_view(&pet)).collect()
    }

    pub fn get_pets_for_owner(&self, account_id: AccountId) -> Vec<PetView> {
        self.pets
            .values()
            .filter(|pet| pet.owner_id == account_id)
            .map(|pet| self.to_view(&pet))
            .collect()
    }

    pub fn is_pet_alive(&self, pet_id: u64) -> bool {
        self.pets
            .get(&pet_id)
            .map(|pet| pet.is_alive(env::block_timestamp()))
            .unwrap_or(false)
    }

    pub fn get_shop(&self) -> Vec<ShopItem> {
        self.shop_ids
            .iter()
            .filter_map(|id| self.shop.get(id))
            .collect()
    }

    /// Published mint odds, in basis points.
    pub fn get_mint_odds(&self) -> Vec<(Rarity, u32)> {
        Rarity::ODDS_BPS.to_vec()
    }

    pub fn get_config(&self) -> near_sdk::serde_json::Value {
        json!({
            "owner_id": self.owner_id,
            "stake_contract": self.stake_contract,
            "mint_router": self.mint_router,
            "mint_price": U128(self.mint_price),
            "market_fee_bps": self.market_fee_bps,
            "treasury": self.treasury,
            "categories": CATEGORIES,
            "total_pets": self.pets.len(),
            "paused": self.paused,
        })
    }

    // ── Owner ────────────────────────────────────────────────────────────────

    pub fn set_mint_price(&mut self, mint_price: U128) {
        self.assert_owner();
        self.mint_price = mint_price.0;
    }

    pub fn set_stake_contract(&mut self, stake_contract: AccountId) {
        self.assert_owner();
        self.stake_contract = stake_contract;
    }

    pub fn set_mint_router(&mut self, mint_router: AccountId) {
        self.assert_owner();
        self.mint_router = mint_router;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.assert_owner();
        self.paused = paused;
    }

    pub fn set_shop_item(&mut self, item: ShopItem) {
        self.assert_owner();
        if !self.shop_ids.contains(&item.item_id) {
            self.shop_ids.push(item.item_id);
        }
        self.shop.insert(&item.item_id, &item);
    }

    /// Sweep mint and shop revenue to the treasury, which is what gets staked
    /// with NEAR AI to produce the credit stream.
    pub fn sweep(&mut self, receiver_id: AccountId, amount: U128) -> Promise {
        self.assert_owner();
        Promise::new(receiver_id).transfer(NearToken::from_yoctonear(amount.0))
    }

    /// Import pets from the v1 contract. All 20 of them fit in one call.
    ///
    /// `pet_id` is preserved so existing links and any `pet-stake` records stay
    /// valid, and `time_until_starving` is read for what it is — an absolute
    /// timestamp — rather than for what its name suggests.
    pub fn import_pets(&mut self, pets: Vec<LegacyPet>) -> u64 {
        self.assert_owner();
        let mut imported = 0;

        for legacy in pets {
            if self.pets.get(&legacy.pet_id).is_some() {
                continue;
            }

            // Every v1 pet is level 1 with no stars, so they are all common.
            let rarity = Rarity::Common;
            let (atk, def) = rarity.base_stats();

            let pet = Pet {
                pet_id: legacy.pet_id,
                owner_id: legacy.owner_id.clone(),
                name: legacy.name.chars().take(32).collect(),
                category: legacy.category.clone(),
                rarity,
                level: legacy.level.saturating_sub(1).min(MAX_LEVEL),
                star: legacy.star.min(MAX_STAR),
                xp: 0,
                evolution_phase: legacy.pet_evolution_phase.clamp(1, MAX_EVOLUTION_PHASE),
                atk,
                def,
                starves_at: legacy.time_until_starving.0,
                born_at: legacy.time_pet_born.0,
                last_attack_used: 0,
                last_attacked: 0,
                shield: 0,
                win_streak: 0,
                locked: false,
            };

            self.tokens.internal_mint_with_refund(
                pet.pet_id.to_string(),
                pet.owner_id.clone(),
                Some(Self::token_metadata(&pet)),
                None,
            );
            self.pets.insert(&pet.pet_id, &pet);
            self.next_pet_id = self.next_pet_id.max(pet.pet_id + 1);
            imported += 1;
        }

        self.log_event("pets_imported", json!({ "count": imported }));
        imported
    }
}

// ── NEP-171 ──────────────────────────────────────────────────────────────────

#[near]
impl Game {
    /// Transfers go through the standard, behind the lock check.
    #[payable]
    pub fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) {
        self.assert_transferable(&token_id);
        let pet_id: u64 = token_id.parse().expect("Bad token id");

        near_contract_standards::non_fungible_token::core::NonFungibleTokenCore::nft_transfer(
            &mut self.tokens,
            receiver_id.clone(),
            token_id,
            approval_id,
            memo,
        );

        // Keep the game's own owner field in step with the standard's.
        if let Some(mut pet) = self.pets.get(&pet_id) {
            pet.owner_id = receiver_id;
            self.pets.insert(&pet_id, &pet);
        }
        // Moving a pet out of band ends any sale of it.
        self.internal_unlist(pet_id, "transferred");
    }

    #[payable]
    pub fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool> {
        self.assert_transferable(&token_id);
        near_contract_standards::non_fungible_token::core::NonFungibleTokenCore::nft_transfer_call(
            &mut self.tokens,
            receiver_id,
            token_id,
            approval_id,
            memo,
            msg,
        )
    }

    pub fn nft_token(&self, token_id: TokenId) -> Option<Token> {
        near_contract_standards::non_fungible_token::core::NonFungibleTokenCore::nft_token(
            &self.tokens,
            token_id,
        )
    }

    #[private]
    #[allow(clippy::too_many_arguments)]
    pub fn nft_resolve_transfer(
        &mut self,
        previous_owner_id: AccountId,
        receiver_id: AccountId,
        token_id: TokenId,
        approved_account_ids: Option<std::collections::HashMap<AccountId, u64>>,
    ) -> bool {
        use near_contract_standards::non_fungible_token::core::NonFungibleTokenResolver;
        let kept = self.tokens.nft_resolve_transfer(
            previous_owner_id.clone(),
            receiver_id.clone(),
            token_id.clone(),
            approved_account_ids,
        );

        // `kept == true` means the receiver kept it; otherwise it came back.
        if let Ok(pet_id) = token_id.parse::<u64>() {
            if let Some(mut pet) = self.pets.get(&pet_id) {
                pet.owner_id = if kept { receiver_id } else { previous_owner_id };
                self.pets.insert(&pet_id, &pet);
            }
        }
        kept
    }

    #[payable]
    pub fn nft_approve(
        &mut self,
        token_id: TokenId,
        account_id: AccountId,
        msg: Option<String>,
    ) -> Option<Promise> {
        use near_contract_standards::non_fungible_token::approval::NonFungibleTokenApproval;
        self.assert_transferable(&token_id);
        self.tokens.nft_approve(token_id, account_id, msg)
    }

    #[payable]
    pub fn nft_revoke(&mut self, token_id: TokenId, account_id: AccountId) {
        use near_contract_standards::non_fungible_token::approval::NonFungibleTokenApproval;
        self.tokens.nft_revoke(token_id, account_id);
    }

    #[payable]
    pub fn nft_revoke_all(&mut self, token_id: TokenId) {
        use near_contract_standards::non_fungible_token::approval::NonFungibleTokenApproval;
        self.tokens.nft_revoke_all(token_id);
    }

    pub fn nft_is_approved(
        &self,
        token_id: TokenId,
        approved_account_id: AccountId,
        approval_id: Option<u64>,
    ) -> bool {
        use near_contract_standards::non_fungible_token::approval::NonFungibleTokenApproval;
        self.tokens
            .nft_is_approved(token_id, approved_account_id, approval_id)
    }

    pub fn nft_total_supply(&self) -> U128 {
        use near_contract_standards::non_fungible_token::enumeration::NonFungibleTokenEnumeration;
        self.tokens.nft_total_supply()
    }

    pub fn nft_tokens(&self, from_index: Option<U128>, limit: Option<u64>) -> Vec<Token> {
        use near_contract_standards::non_fungible_token::enumeration::NonFungibleTokenEnumeration;
        self.tokens.nft_tokens(from_index, limit)
    }

    pub fn nft_supply_for_owner(&self, account_id: AccountId) -> U128 {
        use near_contract_standards::non_fungible_token::enumeration::NonFungibleTokenEnumeration;
        self.tokens.nft_supply_for_owner(account_id)
    }

    pub fn nft_tokens_for_owner(
        &self,
        account_id: AccountId,
        from_index: Option<U128>,
        limit: Option<u64>,
    ) -> Vec<Token> {
        use near_contract_standards::non_fungible_token::enumeration::NonFungibleTokenEnumeration;
        self.tokens
            .nft_tokens_for_owner(account_id, from_index, limit)
    }
}

#[near]
impl NonFungibleTokenMetadataProvider for Game {
    fn nft_metadata(&self) -> NFTContractMetadata {
        self.metadata.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    const ONE_NEAR: u128 = 10u128.pow(24);

    fn owner() -> AccountId {
        "owner.near".parse().unwrap()
    }
    fn stake() -> AccountId {
        "stake.near".parse().unwrap()
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

    fn ctx(predecessor: AccountId, seconds: u64, deposit: u128) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id("game.near".parse().unwrap())
            .predecessor_account_id(predecessor)
            .block_timestamp(seconds * NANOS_PER_SECOND)
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .random_seed([7u8; 32]);
        builder
    }

    fn treasury() -> AccountId {
        "treasury.near".parse().unwrap()
    }

    fn deploy() -> Game {
        testing_env!(ctx(owner(), 0, 0).build());
        Game::new(owner(), stake(), router(), treasury(), U128(2 * ONE_NEAR))
    }

    /// Mint a pet for `who` without going through the payable entry point.
    fn mint(contract: &mut Game, who: AccountId, at: u64) -> u64 {
        testing_env!(ctx(router(), at, 0).build());
        contract.create_pet_for(who, "SMAUG".to_string(), "blackdragon".to_string())
    }

    // ── Rarity ───────────────────────────────────────────────────────────────

    #[test]
    fn rarity_odds_cover_the_whole_range_and_sum_to_one() {
        let total: u32 = Rarity::ODDS_BPS.iter().map(|(_, bps)| bps).sum();
        assert_eq!(total, 10_000, "odds must be exhaustive");

        assert_eq!(Rarity::from_roll(0), Rarity::Common);
        assert_eq!(Rarity::from_roll(6_999), Rarity::Common);
        assert_eq!(Rarity::from_roll(7_000), Rarity::Rare);
        assert_eq!(Rarity::from_roll(8_999), Rarity::Rare);
        assert_eq!(Rarity::from_roll(9_000), Rarity::Epic);
        assert_eq!(Rarity::from_roll(9_799), Rarity::Epic);
        assert_eq!(Rarity::from_roll(9_800), Rarity::Legendary);
        assert_eq!(Rarity::from_roll(9_999), Rarity::Legendary);
    }

    #[test]
    fn rarer_pets_start_stronger() {
        let (common_atk, _) = Rarity::Common.base_stats();
        let (legendary_atk, _) = Rarity::Legendary.base_stats();
        assert!(legendary_atk > common_atk);
        // Not so much stronger that rarity alone decides every fight.
        assert!(legendary_atk < common_atk * 3);
    }

    // ── Minting ──────────────────────────────────────────────────────────────

    #[test]
    fn minting_creates_a_token_and_a_pet() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 10);

        assert_eq!(pet_id, 1);
        assert_eq!(contract.nft_total_supply(), U128(1));
        assert_eq!(contract.nft_supply_for_owner(alice()), U128(1));

        let pet = contract.get_pet(pet_id).unwrap();
        assert_eq!(pet.owner_id, alice());
        assert_eq!(pet.level, 0);
        assert_eq!(pet.evolution_phase, 1);
        assert!(!pet.locked);

        // The attributes pet-stake reads must line up with the pet.
        let (stake_owner, attrs) = contract.get_pet_attrs(pet_id);
        assert_eq!(stake_owner, alice());
        assert_eq!(attrs.level, 0);
        assert_eq!(attrs.evolution_phase, 1);
    }

    #[test]
    #[should_panic(expected = "Only the mint router")]
    fn players_cannot_mint_for_other_accounts() {
        let mut contract = deploy();
        testing_env!(ctx(alice(), 10, 0).build());
        contract.create_pet_for(bob(), "FREE".to_string(), "blackdragon".to_string());
    }

    #[test]
    #[should_panic(expected = "Unknown pet category")]
    fn rejects_unknown_categories() {
        let mut contract = deploy();
        testing_env!(ctx(router(), 10, 0).build());
        contract.create_pet_for(alice(), "NOPE".to_string(), "unicorn".to_string());
    }

    #[test]
    #[should_panic(expected = "at least the mint price")]
    fn minting_for_yourself_costs_the_mint_price() {
        let mut contract = deploy();
        testing_env!(ctx(alice(), 10, ONE_NEAR).build());
        contract.create_pet("CHEAP".to_string(), "blackdragon".to_string());
    }

    // ── Status, derived rather than stored ───────────────────────────────────

    #[test]
    fn status_is_computed_from_the_deadline() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        // Fresh: a full day of food.
        assert_eq!(contract.get_pet(pet_id).unwrap().status, PetStatus::Happy);

        // Under three hours left.
        testing_env!(ctx(alice(), 100 + STARVE_SECONDS - 3_000, 0).build());
        assert_eq!(contract.get_pet(pet_id).unwrap().status, PetStatus::Hungry);

        // Past the deadline. v1 would still have said HAPPY here.
        testing_env!(ctx(alice(), 100 + STARVE_SECONDS + 1, 0).build());
        assert_eq!(contract.get_pet(pet_id).unwrap().status, PetStatus::Dying);
        assert!(!contract.is_pet_alive(pet_id));
    }

    // ── Feeding ──────────────────────────────────────────────────────────────

    #[test]
    fn feeding_extends_the_clock_and_grants_xp() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);
        let before = contract.get_pet(pet_id).unwrap().starves_at.0;

        // Beef: 0.10 NEAR, +3h, 30 XP.
        testing_env!(ctx(alice(), 100, ONE_NEAR).build());
        contract.buy_item(pet_id, 1);

        let after = contract.get_pet(pet_id).unwrap();
        assert_eq!(after.starves_at.0, before + 3 * 3_600 * NANOS_PER_SECOND);
        assert_eq!(after.xp, 30);
    }

    #[test]
    fn banked_food_is_capped() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        // Feed far more than the ceiling allows.
        for _ in 0..100 {
            testing_env!(ctx(alice(), 100, ONE_NEAR).build());
            contract.buy_item(pet_id, 1);
        }

        let pet = contract.get_pet(pet_id).unwrap();
        let ceiling = (100 + MAX_STARVE_SECONDS) * NANOS_PER_SECOND;
        assert!(
            pet.starves_at.0 <= ceiling,
            "a pet must not be made immortal in one sitting"
        );
    }

    #[test]
    #[should_panic(expected = "use a revival item")]
    fn a_starved_pet_needs_holy_water() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(alice(), 100 + STARVE_SECONDS + 1, ONE_NEAR).build());
        contract.buy_item(pet_id, 1); // Beef
    }

    #[test]
    fn holy_water_revives() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        let at = 100 + STARVE_SECONDS + 1;
        testing_env!(ctx(alice(), at, ONE_NEAR).build());
        contract.buy_item(pet_id, 4); // Holy Water

        assert!(contract.is_pet_alive(pet_id));
    }

    #[test]
    fn levelling_follows_the_published_curve() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        // 100 XP for level 1, then 18% more each time.
        assert_eq!(contract.get_pet(pet_id).unwrap().xp_for_next_level, 100);

        testing_env!(ctx(stake(), 100, 0).build());
        contract.award_xp(pet_id, 100);

        let pet = contract.get_pet(pet_id).unwrap();
        assert_eq!(pet.level, 1);
        assert_eq!(pet.xp, 0);
        assert_eq!(pet.xp_for_next_level, 118);
        // Stats grow with level, so combat keeps pace with staking weight.
        assert_eq!(pet.atk, 104);
    }

    #[test]
    #[should_panic(expected = "Only the staking contract")]
    fn players_cannot_award_themselves_xp() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);
        testing_env!(ctx(alice(), 100, 0).build());
        contract.award_xp(pet_id, 500);
    }

    // ── Locking ──────────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Only the staking contract can lock")]
    fn players_cannot_lock_pets() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);
        testing_env!(ctx(alice(), 100, 0).build());
        contract.set_locked(pet_id, true);
    }

    #[test]
    #[should_panic(expected = "at work in the AI Lab")]
    fn a_staked_pet_cannot_be_transferred() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(stake(), 100, 0).build());
        contract.set_locked(pet_id, true);

        testing_env!(ctx(alice(), 100, 1).build());
        contract.nft_transfer(bob(), pet_id.to_string(), None, None);
    }

    #[test]
    #[should_panic(expected = "at work in the AI Lab")]
    fn a_staked_pet_cannot_be_approved_either() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(stake(), 100, 0).build());
        contract.set_locked(pet_id, true);

        testing_env!(ctx(alice(), 100, ONE_NEAR).build());
        contract.nft_approve(pet_id.to_string(), bob(), None);
    }

    #[test]
    fn unlocking_makes_a_pet_transferable_again() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(stake(), 100, 0).build());
        contract.set_locked(pet_id, true);
        contract.set_locked(pet_id, false);

        testing_env!(ctx(alice(), 100, 1).build());
        contract.nft_transfer(bob(), pet_id.to_string(), None, None);

        // Both the standard and the game's own record move.
        assert_eq!(contract.nft_supply_for_owner(bob()), U128(1));
        assert_eq!(contract.get_pet(pet_id).unwrap().owner_id, bob());
        assert_eq!(contract.get_pet_attrs(pet_id).0, bob());
    }

    // ── Battle ───────────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "same owner")]
    fn cannot_attack_your_own_pet() {
        let mut contract = deploy();
        let a = mint(&mut contract, alice(), 100);
        let b = mint(&mut contract, alice(), 100);

        testing_env!(ctx(alice(), 100, 0).build());
        let _ = contract.attack(a, b);
    }

    #[test]
    #[should_panic(expected = "at work in the AI Lab")]
    fn cannot_attack_a_staked_pet() {
        let mut contract = deploy();
        let mine = mint(&mut contract, alice(), 100);
        let theirs = mint(&mut contract, bob(), 100);

        testing_env!(ctx(stake(), 100, 0).build());
        contract.set_locked(theirs, true);

        testing_env!(ctx(alice(), 100, 0).build());
        let _ = contract.attack(mine, theirs);
    }

    #[test]
    #[should_panic(expected = "has starved")]
    fn a_starved_pet_cannot_attack() {
        let mut contract = deploy();
        let mine = mint(&mut contract, alice(), 100);
        let theirs = mint(&mut contract, bob(), 100);

        testing_env!(ctx(alice(), 100 + STARVE_SECONDS + 1, 0).build());
        let _ = contract.attack(mine, theirs);
    }

    #[test]
    fn attacking_leaves_the_pets_untouched_until_the_callback() {
        let mut contract = deploy();
        let mine = mint(&mut contract, alice(), 100);
        let theirs = mint(&mut contract, bob(), 100);

        let before = contract.get_pet(theirs).unwrap();

        testing_env!(ctx(alice(), 100, 0).build());
        let _ = contract.attack(mine, theirs);

        // Nothing is applied yet: the settlement has not come back. This is the
        // whole reason `report_battle` is called before damage is dealt.
        let after = contract.get_pet(theirs).unwrap();
        assert_eq!(after.star, before.star);
        assert_eq!(after.starves_at.0, before.starves_at.0);
        assert_eq!(after.last_attacked.0, 0);
    }

    #[test]
    fn a_rejected_settlement_damages_nobody() {
        let mut contract = deploy();
        let mine = mint(&mut contract, alice(), 100);
        let theirs = mint(&mut contract, bob(), 100);
        let before = contract.get_pet(theirs).unwrap();

        testing_env!(ctx("game.near".parse().unwrap(), 100, 0).build());
        let applied = contract.on_battle_settled(mine, theirs, Err(PromiseError::Failed));

        assert!(!applied);
        let after = contract.get_pet(theirs).unwrap();
        assert_eq!(after.star, before.star);
        assert_eq!(after.starves_at.0, before.starves_at.0);
    }

    fn outcome(star_protected: bool) -> BattleOutcome {
        BattleOutcome {
            transferred: U128(9_500),
            rake: U128(500),
            star_protected,
        }
    }

    /// Win `times` battles against a fresh opponent each time, so the win
    /// streak builds the way it would in play.
    fn win_battles(contract: &mut Game, winner_pet: u64, times: u32) {
        for i in 0..times {
            testing_env!(ctx("game.near".parse().unwrap(), 100 + i as u64, 0).build());
            let loser = mint(contract, bob(), 100 + i as u64);
            contract.on_battle_settled(winner_pet, loser, Ok(outcome(false)));
        }
    }

    #[test]
    fn three_wins_earn_a_star() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        assert_eq!(contract.get_pet(pet_id).unwrap().star, 0);
        assert_eq!(contract.get_pet(pet_id).unwrap().wins_to_next_star, Some(3));

        win_battles(&mut contract, pet_id, 2);
        let after_two = contract.get_pet(pet_id).unwrap();
        assert_eq!(after_two.star, 0, "two wins is not enough");
        assert_eq!(after_two.win_streak, 2);
        assert_eq!(after_two.wins_to_next_star, Some(1));

        win_battles(&mut contract, pet_id, 1);
        let after_three = contract.get_pet(pet_id).unwrap();
        assert_eq!(after_three.star, 1, "the third win earns it");
        assert_eq!(after_three.win_streak, 0, "and the streak restarts");
    }

    #[test]
    fn a_defeat_wipes_the_streak_and_takes_a_star() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);
        let rival = mint(&mut contract, bob(), 100);

        // Earn one star, then build towards the next.
        win_battles(&mut contract, pet_id, 4);
        let before = contract.get_pet(pet_id).unwrap();
        assert_eq!(before.star, 1);
        assert_eq!(before.win_streak, 1);

        // Now lose.
        testing_env!(ctx("game.near".parse().unwrap(), 200, 0).build());
        contract.on_battle_settled(rival, pet_id, Ok(outcome(false)));

        let after = contract.get_pet(pet_id).unwrap();
        assert_eq!(after.star, 0, "the star is lost");
        assert_eq!(after.win_streak, 0, "and so is the progress towards the next");
    }

    #[test]
    fn a_star_charm_saves_an_earned_star() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);
        let rival = mint(&mut contract, bob(), 100);

        win_battles(&mut contract, pet_id, 3);
        assert_eq!(contract.get_pet(pet_id).unwrap().star, 1);

        testing_env!(ctx("game.near".parse().unwrap(), 200, 0).build());
        contract.on_battle_settled(rival, pet_id, Ok(outcome(true)));

        assert_eq!(
            contract.get_pet(pet_id).unwrap().star,
            1,
            "the charm is what makes this purchase worth anything"
        );
    }

    #[test]
    fn stars_stop_at_the_cap() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        win_battles(&mut contract, pet_id, WINS_PER_STAR * MAX_STAR + 6);

        let pet = contract.get_pet(pet_id).unwrap();
        assert_eq!(pet.star, MAX_STAR);
        assert_eq!(pet.wins_to_next_star, None, "nothing left to earn");
    }

    #[test]
    fn stars_raise_combat_strength() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);
        let before = contract.get_pet(pet_id).unwrap().effective_atk.0;

        win_battles(&mut contract, pet_id, 3);
        let after = contract.get_pet(pet_id).unwrap();

        assert_eq!(after.star, 1);
        assert!(
            after.effective_atk.0 > before,
            "a star has to mean something in a fight"
        );
    }

    #[test]
    fn losing_costs_a_star_and_time_but_never_the_pet() {
        let mut contract = deploy();
        let winner = mint(&mut contract, alice(), 100);
        let loser = mint(&mut contract, bob(), 100);

        // Give the loser a star to lose.
        testing_env!(ctx(stake(), 100, 0).build());
        contract.award_xp(loser, 0);
        let mut pet = contract.pets.get(&loser).unwrap();
        pet.star = 2;
        contract.pets.insert(&loser, &pet);
        let before = contract.get_pet(loser).unwrap();

        testing_env!(ctx("game.near".parse().unwrap(), 100, 0).build());
        assert!(contract.on_battle_settled(winner, loser, Ok(outcome(false))));

        let after = contract.get_pet(loser).unwrap();
        assert_eq!(after.star, 1, "one star lost");
        assert!(after.starves_at.0 < before.starves_at.0, "time lost");
        assert!(contract.is_pet_alive(loser), "but the pet survives");
        assert!(after.last_attacked.0 > 0);
    }

    #[test]
    fn a_star_charm_saves_the_star() {
        let mut contract = deploy();
        let winner = mint(&mut contract, alice(), 100);
        let loser = mint(&mut contract, bob(), 100);

        let mut pet = contract.pets.get(&loser).unwrap();
        pet.star = 2;
        contract.pets.insert(&loser, &pet);

        testing_env!(ctx("game.near".parse().unwrap(), 100, 0).build());
        contract.on_battle_settled(winner, loser, Ok(outcome(true)));

        assert_eq!(contract.get_pet(loser).unwrap().star, 2);
    }

    #[test]
    fn a_shield_absorbs_the_hit_before_the_star_does() {
        let mut contract = deploy();
        let winner = mint(&mut contract, alice(), 100);
        let loser = mint(&mut contract, bob(), 100);

        let mut pet = contract.pets.get(&loser).unwrap();
        pet.star = 2;
        pet.shield = 1;
        contract.pets.insert(&loser, &pet);

        testing_env!(ctx("game.near".parse().unwrap(), 100, 0).build());
        contract.on_battle_settled(winner, loser, Ok(outcome(false)));

        let after = contract.get_pet(loser).unwrap();
        assert_eq!(after.shield, 0, "the shield is spent");
        assert_eq!(after.star, 2, "and the star survives");
    }

    #[test]
    fn the_winner_gains_xp() {
        let mut contract = deploy();
        let winner = mint(&mut contract, alice(), 100);
        let loser = mint(&mut contract, bob(), 100);

        testing_env!(ctx("game.near".parse().unwrap(), 100, 0).build());
        contract.on_battle_settled(winner, loser, Ok(outcome(false)));

        assert_eq!(contract.get_pet(winner).unwrap().xp, 25);
    }

    #[test]
    fn battle_scores_follow_the_stats() {
        let mut contract = deploy();
        let weak = mint(&mut contract, alice(), 100);
        let strong = mint(&mut contract, bob(), 100);

        // Level the defender well past the attacker.
        testing_env!(ctx(stake(), 100, 0).build());
        contract.award_xp(strong, 400);

        let attacker = contract.pets.get(&weak).unwrap();
        let defender = contract.pets.get(&strong).unwrap();
        assert!(
            defender.effective_def() > attacker.effective_def(),
            "levels must actually make a pet tougher"
        );
    }

    // ── Evolution ────────────────────────────────────────────────────────────

    #[test]
    #[should_panic(expected = "Not enough levels")]
    fn evolving_needs_levels() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);
        testing_env!(ctx(alice(), 100, 0).build());
        contract.evolve(pet_id);
    }

    #[test]
    fn evolving_advances_the_phase() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(stake(), 100, 0).build());
        contract.award_xp(pet_id, 500);
        contract.award_xp(pet_id, 500);

        testing_env!(ctx(alice(), 100, 0).build());
        contract.evolve(pet_id);

        let pet = contract.get_pet(pet_id).unwrap();
        assert_eq!(pet.evolution_phase, 2);
        // The attributes pet-stake re-reads reflect it.
        assert_eq!(contract.get_pet_attrs(pet_id).1.evolution_phase, 2);
    }

    // ── Marketplace ──────────────────────────────────────────────────────────

    #[test]
    fn a_pet_sells_for_near_and_the_treasury_takes_its_cut() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(alice(), 100, 1).build());
        contract.list_pet(pet_id, U128(5 * ONE_NEAR));

        let listing = contract.get_pet_listing(pet_id).unwrap();
        assert_eq!(listing.seller_id, alice());
        assert_eq!(listing.price.0, 5 * ONE_NEAR);
        assert_eq!(
            contract.get_pet(pet_id).unwrap().listed_price,
            Some(U128(5 * ONE_NEAR))
        );

        testing_env!(ctx(bob(), 110, 5 * ONE_NEAR).build());
        let _ = contract.buy_pet(pet_id);

        // Both the standard's record and the game's own move together.
        assert_eq!(contract.get_pet(pet_id).unwrap().owner_id, bob());
        assert_eq!(contract.get_pet_attrs(pet_id).0, bob());
        assert_eq!(contract.nft_supply_for_owner(bob()), U128(1));
        assert_eq!(contract.nft_supply_for_owner(alice()), U128(0));

        // And the sale is over.
        assert!(contract.get_pet_listing(pet_id).is_none());
        assert!(contract.get_pet(pet_id).unwrap().listed_price.is_none());
    }

    #[test]
    #[should_panic(expected = "at work in the AI Lab")]
    fn a_staked_pet_cannot_be_listed() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(stake(), 100, 0).build());
        contract.set_locked(pet_id, true);

        testing_env!(ctx(alice(), 100, 1).build());
        contract.list_pet(pet_id, U128(ONE_NEAR));
    }

    #[test]
    fn staking_a_listed_pet_pulls_the_listing() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(alice(), 100, 1).build());
        contract.list_pet(pet_id, U128(ONE_NEAR));
        assert!(contract.get_pet_listing(pet_id).is_some());

        testing_env!(ctx(stake(), 100, 0).build());
        contract.set_locked(pet_id, true);

        assert!(
            contract.get_pet_listing(pet_id).is_none(),
            "a buyer must not be sold a pet that cannot be moved"
        );
    }

    #[test]
    fn transferring_a_listed_pet_ends_the_sale() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(alice(), 100, 1).build());
        contract.list_pet(pet_id, U128(ONE_NEAR));
        contract.nft_transfer(bob(), pet_id.to_string(), None, None);

        assert!(contract.get_pet_listing(pet_id).is_none());
        assert_eq!(contract.get_pet(pet_id).unwrap().owner_id, bob());
    }

    #[test]
    #[should_panic(expected = "at least the asking price")]
    fn underpaying_buys_nothing() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(alice(), 100, 1).build());
        contract.list_pet(pet_id, U128(5 * ONE_NEAR));

        testing_env!(ctx(bob(), 110, 4 * ONE_NEAR).build());
        let _ = contract.buy_pet(pet_id);
    }

    #[test]
    #[should_panic(expected = "listing is yours")]
    fn you_cannot_buy_your_own_pet() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(alice(), 100, 1).build());
        contract.list_pet(pet_id, U128(ONE_NEAR));
        testing_env!(ctx(alice(), 110, ONE_NEAR).build());
        let _ = contract.buy_pet(pet_id);
    }

    #[test]
    #[should_panic(expected = "not for sale")]
    fn you_cannot_buy_what_is_not_listed() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);
        testing_env!(ctx(bob(), 110, ONE_NEAR).build());
        let _ = contract.buy_pet(pet_id);
    }

    #[test]
    #[should_panic(expected = "listing is not yours")]
    fn only_the_seller_can_unlist() {
        let mut contract = deploy();
        let pet_id = mint(&mut contract, alice(), 100);

        testing_env!(ctx(alice(), 100, 1).build());
        contract.list_pet(pet_id, U128(ONE_NEAR));

        testing_env!(ctx(bob(), 100, 1).build());
        contract.unlist_pet(pet_id);
    }

    #[test]
    fn the_fee_is_capped() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0, 0).build());
        contract.set_market_fee_bps(1_000);
        assert_eq!(
            contract.get_config()["market_fee_bps"].as_u64().unwrap(),
            1_000
        );
    }

    #[test]
    #[should_panic(expected = "cannot exceed 10%")]
    fn the_fee_cannot_be_set_above_the_cap() {
        let mut contract = deploy();
        testing_env!(ctx(owner(), 0, 0).build());
        contract.set_market_fee_bps(1_001);
    }

    // ── Migration ────────────────────────────────────────────────────────────

    #[test]
    fn importing_v1_pets_keeps_their_ids_and_reads_the_timestamp_correctly() {
        let mut contract = deploy();

        // The real pet #1 from mainnet.
        let legacy = LegacyPet {
            pet_id: 1,
            owner_id: "huunhanz-hot.tg".parse().unwrap(),
            name: "PET #1".to_string(),
            category: "blackdragon".to_string(),
            level: 1,
            star: 0,
            pet_evolution_phase: 1,
            time_pet_born: U64(1_722_009_513_921_106_082),
            time_until_starving: U64(1_722_875_218_337_755_475),
        };

        testing_env!(ctx(owner(), 0, 0).build());
        assert_eq!(contract.import_pets(vec![legacy]), 1);

        // Read it from a present-day block, not from timestamp zero.
        testing_env!(ctx(owner(), 1_780_000_000, 0).build());
        let pet = contract.get_pet(1).unwrap();
        assert_eq!(pet.pet_id, 1, "ids are preserved");
        assert_eq!(pet.owner_id.to_string(), "huunhanz-hot.tg");
        // Read as an absolute deadline, which is what it always was.
        assert_eq!(pet.starves_at.0, 1_722_875_218_337_755_475);
        assert_eq!(pet.status, PetStatus::Dying, "it starved in 2024");
        // A new mint must not collide with an imported id.
        assert!(contract.next_pet_id > 1);
    }

    #[test]
    fn importing_is_idempotent() {
        let mut contract = deploy();
        let legacy = || LegacyPet {
            pet_id: 5,
            owner_id: alice(),
            name: "DUPE".to_string(),
            category: "greendragon".to_string(),
            level: 1,
            star: 0,
            pet_evolution_phase: 1,
            time_pet_born: U64(1),
            time_until_starving: U64(2),
        };

        testing_env!(ctx(owner(), 0, 0).build());
        assert_eq!(contract.import_pets(vec![legacy()]), 1);
        assert_eq!(contract.import_pets(vec![legacy()]), 0, "no double import");
        assert_eq!(contract.nft_total_supply(), U128(1));
    }

    #[test]
    #[should_panic(expected = "Only the owner")]
    fn only_the_owner_imports() {
        let mut contract = deploy();
        testing_env!(ctx(alice(), 0, 0).build());
        contract.import_pets(vec![]);
    }
}
