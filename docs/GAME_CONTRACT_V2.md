# Game contract v2 — interface spec

Everything in this repository is finished and tested. None of it works end to
end, because all four integration points live in the pet contract and the
deployed one has none of them.

This is the contract that unblocks it. It is written to be implementable
without reading the rest of the repo: every signature here is the one the
deployed `pet-stake` and `mint-router` actually call, copied from their
`#[ext_contract]` traits.

---

## 1 · What exists today

Verified against mainnet, not taken from the old client:

| Probe on `game.joyv1.near` | Result |
|---|---|
| `nft_token`, `nft_transfer`, `nft_approve`, `nft_metadata`, `nft_total_supply` | **MethodNotFound** — not NEP-171 |
| `get_pet_metadata`, `get_pet_attrs` | **MethodNotFound** |
| `get_all_pet_metadata` | exists, returns the full array |
| `is_pet_alive`, `create_pet`, `buy_item`, `change_name_pet`, `attack` | exist |

And on `faucet.joyv1.near`:

| Probe | Result |
|---|---|
| `ft_metadata`, `ft_balance_of`, `ft_total_supply`, `storage_balance_bounds` | **MethodNotFound** — not a NEP-141 |
| `get_joychi` | exists (a change method), but nothing can read what it credits |

So `$JOY` was never a token. The economy has been rebuilt around NEAR and AI
credit; see [ECONOMY.md](ECONOMY.md).

### The population

20 pets, 6 owners. Zero have ever attacked or been attacked. Zero are above
level 1. Zero have a star. 19 of 20 passed their starving deadline in 2024.

This matters for section 2: there is almost nothing to preserve.

### What `get_all_pet_metadata` returns

```jsonc
{
  "pet_id": 1,
  "name": "PET #1",
  "owner_id": "huunhanz-hot.tg",
  "category": "blackdragon",        // or "greendragon"
  "pet_species": 1,                 // 1 | 2, duplicates `category`
  "pet_evolution_phase": 1,
  "level": 1,
  "star": 0,
  "score": 3000,
  "status": "HAPPY",                // stale: only rewritten when touched
  "pet_shield": 0,
  "reward_debt": 0,
  "time_pet_born": 1722009513921106082,
  "time_until_starving": 1722875218337755475,  // ABSOLUTE ns, not a duration
  "last_attack_used": 0,
  "last_attacked": 0,
  "items": [ { "item_id": 1, "name": "Beef", "points": 1000, "price": 1,
               "time_extension": 10800000, "is_revival": false } ],
  "pet_need_evolution_item": true,
  "pet_has_evolution_item": true,
  "pet_evolution_item_id": 1,
  "extra_permission": []
}
```

Two traps worth carrying forward into v2:

- **`time_until_starving` is an absolute nanosecond timestamp.** The old client
  divided it by `1e7` and treated the result as seconds remaining, which
  rendered a countdown of about 5,460 years. Either rename it
  (`starves_at_ns`) or keep the name and document it loudly.
- **`status` is stored, not derived.** 14 of 20 pets say `HAPPY` while long
  past their deadline. v2 should compute status on read, or drop the field and
  let clients derive it.

---

## 2 · Extend or rewrite

**Recommendation: rewrite, and migrate the 20 pets.**

The case for it:

- Not NEP-171, so pets cannot be transferred at all. Retrofitting the standard
  onto existing state is most of a rewrite anyway.
- `rarity` does not exist and is needed by staking weight. The client currently
  fakes it from star count.
- 20 pets and 6 owners is a migration that fits in one transaction.
- The stale `status` and misnamed `time_until_starving` are baked into the
  current storage layout.

The case against is the usual one — a deployed contract is a known quantity —
and it is weaker than normal here because essentially nobody has played.

If you extend instead, sections 3–6 still apply verbatim; only section 7
changes.

---

## 3 · Types

These must serialise exactly as written. `pet-stake` deserialises them.

```rust
/// serde: lowercase. "common" | "rare" | "epic" | "legendary"
#[near(serializers = [json, borsh])]
#[serde(rename_all = "lowercase")]
pub enum Rarity { Common, Rare, Epic, Legendary }

/// The only pet fields staking weight depends on.
#[near(serializers = [json, borsh])]
pub struct PetAttrs {
    pub rarity: Rarity,
    pub level: u32,
    pub star: u32,
    pub evolution_phase: u8,   // 1..=4
}
```

`pet-stake` turns these into weight:

```
weight = base(rarity) × (1 + level/20) × (1 + star × 0.15) × evolution
base:      common 100 · rare 160 · epic 260 · legendary 400
evolution: phase 1 ×1.00 · 2 ×1.15 · 3 ×1.35 · 4 ×1.60
```

A fresh common is 100; a maxed legendary is 2,240. Weight drives both the
credit stream and the item drop rate, so **any method that changes `level`,
`star` or `evolution_phase` must trigger a re-sync** (section 5).

---

## 4 · Required methods

### 4.1 NEP-171

Core, enumeration and approval, per the standard. Nothing unusual:

```rust
fn nft_metadata(&self) -> NFTContractMetadata;
fn nft_token(&self, token_id: TokenId) -> Option<Token>;
fn nft_total_supply(&self) -> U128;
fn nft_tokens(&self, from_index: Option<U128>, limit: Option<u64>) -> Vec<Token>;
fn nft_supply_for_owner(&self, account_id: AccountId) -> U128;
fn nft_tokens_for_owner(&self, account_id: AccountId, from_index: Option<U128>, limit: Option<u64>) -> Vec<Token>;

#[payable] fn nft_transfer(&mut self, receiver_id: AccountId, token_id: TokenId, approval_id: Option<u64>, memo: Option<String>);
#[payable] fn nft_transfer_call(&mut self, receiver_id: AccountId, token_id: TokenId, approval_id: Option<u64>, memo: Option<String>, msg: String) -> PromiseOrValue<bool>;
#[payable] fn nft_approve(&mut self, token_id: TokenId, account_id: AccountId, msg: Option<String>) -> Option<Promise>;
#[payable] fn nft_revoke(&mut self, token_id: TokenId, account_id: AccountId);
```

`token_id` should be the decimal `pet_id` as a string, so the two id spaces
never diverge.

> **A staked pet must not be transferable.** `nft_transfer` and
> `nft_approve` have to reject any pet for which
> `stake.is_staked(pet_id)` is true. Because that is a view on another
> contract, the practical implementation is a local mirror: `pet-stake`
> already emits `pet_staked` / `pet_unstaked` events, so either keep a
> `staked: LookupSet<u64>` updated by a privileged `set_staked(pet_id, bool)`
> that only `pet-stake` may call, or accept a one-block race. **Decision
> needed** — see section 8.

### 4.2 Pet lifecycle

```rust
/// Mint for the caller. Keep for players paying with NEAR directly.
#[payable] fn create_pet(&mut self, name: String, category: String) -> u64;

/// Mint for somebody else. Only `mint-router` may call this: a multichain
/// payment arrives from a bridge, not from the player's wallet, so the caller
/// is never the owner.
fn create_pet_for(&mut self, owner_id: AccountId, name: String, category: String) -> u64;

fn change_name_pet(&mut self, pet_id: u64, name: String);
```

`create_pet_for` is called by `mint-router` with 30 TGas and must return the
new `pet_id`. Its callback uses the return value for the `pet_minted` event; a
failure leaves the paid mint claimable through `redeem_pending`, so panicking
is safe but should be rare.

### 4.3 Views `pet-stake` depends on

```rust
/// Owner and attributes in one call. `pet-stake` calls this with 10 TGas from
/// `stake_pet` and `sync_pet`, and refuses the stake if the owner does not
/// match the caller.
fn get_pet_attrs(&self, pet_id: u64) -> (AccountId, PetAttrs);
```

This is the single most important addition. Without it `stake_pet` cannot
verify ownership and the operator has to push attributes by hand with
`admin_set_pet`.

### 4.4 Battle

The ordering here is the part that is easy to get wrong.

```
game.attack(my_pet_id, target_pet_id)
  │
  ├─ local checks: both alive, level rule, attack cooldown, caller owns my_pet
  │
  ├─ promise → stake.report_battle(winner_pet_id, winner_id, loser_pet_id, loser_id)
  │     └─ panics if the loser is staked, warded or wounded
  │     └─ panics if both pets have the same owner
  │     └─ returns BattleOutcome
  │
  └─ callback on_battle_settled(outcome)
        ├─ apply damage / set the loser dying
        ├─ take a star from the loser UNLESS outcome.star_protected
        └─ write last_attack_used / last_attacked
```

**Call `report_battle` first and apply consequences in the callback.** If you
damage the pet first and `report_battle` then fails, a protected pet has been
hurt for free — and "protected" includes every pet currently at work in the AI
Lab.

```rust
#[near(serializers = [json])]
pub struct BattleOutcome {
    pub transferred: U128,     // credits moved from loser to winner
    pub rake: U128,            // credits taken to the reserve
    pub star_protected: bool,  // a Star Charm absorbed it; leave the star alone
}

fn report_battle(
    &mut self,
    winner_pet_id: u64,
    winner_id: AccountId,
    loser_pet_id: u64,
    loser_id: AccountId,
) -> BattleOutcome;
```

Only `game_contract` or the owner may call it, which is why the winner must be
decided by this contract.

Before offering an attack in the UI the client already checks
`stake.is_attackable(pet_id)`; the contract should treat it as advisory and
rely on `report_battle` panicking, since that is atomic.

**No pet dies from a fight.** Credits are distributed by weight share, so
destroying another staker's pet lowers `total_weight` and raises the killer's
own income — death plus a share-based stream makes griefing a computable
profit. Losing costs the wager plus six hours wounded, and nothing more.

### 4.5 Items and feeding

Items are credit items in `pet-stake` now. The game contract's own shop is
priced in a token that never existed, so either reprice it in NEAR:

```rust
#[payable] fn buy_item(&mut self, pet_id: u64, item_id: u16);  // attached deposit = price
```

…or drop it and let feeding happen through `pet-stake`'s credit actions. The
client currently surfaces neither. **Decision needed** — section 8.

Whatever is chosen, feeding must move the starving deadline, and the method
that does it has to re-sync weight if it also grants XP that levels the pet.

---

## 5 · Keeping weight honest

`pet-stake` caches a staked pet's weight. After anything that changes
`level`, `star` or `evolution_phase`, the game contract should call:

```rust
stake.sync_pet(pet_id)   // re-reads get_pet_attrs and re-weights, ~25 TGas
```

It is safe to call for an unstaked pet (it panics with "Pet is not staked", so
guard it with a local `staked` flag or ignore the failure).

Skipping this is not a correctness bug — the stream keeps paying the old weight
— but a player who levels up and sees no change in their rate will file it as
one.

---

## 6 · Access control

| Method | Caller |
|---|---|
| `create_pet` | anyone, payable |
| `create_pet_for` | **`mint-router` only** |
| `attack` | pet owner |
| `change_name_pet` | pet owner |
| `nft_transfer` / `nft_approve` | pet owner or approved, **and not staked** |
| `set_staked` (if adopted) | **`pet-stake` only** |
| `migrate` | owner, once |

Deploy-time configuration the contract needs: `owner_id`, `stake_contract`,
`mint_router`.

---

## 7 · Migration

```rust
#[init(ignore_state)]
fn migrate_from_v1(pets: Vec<LegacyPet>) -> Self;
```

With 20 pets this fits in one call. For each pet: keep `pet_id` so existing
links and `pet-stake` records stay valid, map `category` → `rarity` (all 20 are
level 1 star 0, so they are all `common`), and convert `time_until_starving`
into whatever v2 calls it.

Then:

```bash
near call stake.joyv1.near set_game_contract '{"game_contract":"game-v2.joyv1.near"}' --accountId joyv1.near
near call mint.joyv1.near  set_game_contract '{"game_contract":"game-v2.joyv1.near"}' --accountId joyv1.near
```

and set `PUBLIC_CONTRACT_GAME` in `.env`.

Whether the 19 starved pets are revived as a courtesy is a product call.

---

## 8 · Decisions needed before implementation

1. **Rewrite or extend.** Section 2 recommends rewrite.
2. **The battle formula.** There is none today, and `attack` cannot be written
   without it. Deterministic from ATK/DEF and level, or seeded from
   `env::random_seed()`? Note that 1Click-style determinism matters here: a
   player should be able to see why they lost.
3. **Transfer-vs-staked enforcement.** A mirrored `staked` set with a
   privileged setter, or accept a one-block race where a pet could be
   transferred in the same block it is staked.
4. **The $JOY shop.** Reprice `buy_item` in NEAR, or retire it and feed only
   through credits.
5. **Who assigns rarity.** Mint-time roll, or a property of `category`? Weight
   — and therefore income and drop rate — hangs on this.

---

## 9 · Acceptance criteria

The integration is done when all of these pass on testnet:

- [ ] A pet transfers between two accounts, and **fails** while staked.
- [ ] `stake_pet` succeeds without `admin_set_pet`, and refuses a pet the
      caller does not own.
- [ ] Paying ZEC through 1Click produces a pet in the payer's NEAR account.
- [ ] Attacking a staked pet fails; attacking an ordinary pet moves the wager
      and wounds the loser.
- [ ] A Star Charm is consumed and the star survives.
- [ ] Levelling a staked pet changes its credit rate within one block of
      `sync_pet`.
- [ ] A starved pet reads as dying from a derived status, not a stored one.
- [ ] The countdown on the pet screen shows a plausible time.

Contracts in this repo are unit-tested (49 tests), but nothing exercises the
cross-contract path — these are the first tests that will.
