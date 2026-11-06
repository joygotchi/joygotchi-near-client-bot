# Game contract v2 — interface spec

> **Status: implemented.** The contract lives at
> [`contracts/game`](../contracts/game) with 32 unit tests. This document is
> now the specification it was built to, kept as the reference for what each
> method is for and which decisions were taken. Section 8 records how the five
> open questions were resolved.

Everything else in this repository was finished and tested before this existed,
and none of it worked end to end, because all four integration points live in
the pet contract and the deployed `game.joyv1.near` has none of them.

Every signature here is the one `pet-stake` and `mint-router` actually call,
copied from their `#[ext_contract]` traits.

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

**Decided: rewrite, and migrate the 20 pets.** Implemented in
[`contracts/game`](../contracts/game); `import_pets` does the migration.

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

> **A staked pet must not be transferable**, and the implementation mirrors
> the flag rather than reading it across contracts, because a transfer has to
> be atomic and a cross-contract view cannot be. `pet-stake` calls
> `set_locked(pet_id, bool)` from `internal_stake` and `unstake_pet`; nobody
> else may call it. `nft_transfer`, `nft_transfer_call` and `nft_approve` all
> go through `assert_transferable`.

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

The shop is repriced in NEAR rather than retired, keeping the v1 ids and names
so explorers and old clients still make sense:

```rust
#[payable] fn buy_item(&mut self, pet_id: u64, item_id: u16);  // deposit = price
```

| id | Item | Price | Effect |
|---|---|---|---|
| 1 | Beef | 0.10 NEAR | +3h, 30 XP |
| 2 | Water | 0.05 NEAR | +3h, 10 XP |
| 3 | Shield | 0.20 NEAR | blocks one incoming attack |
| 4 | Holy Water | 0.50 NEAR | +3h, and revives a starved pet |

Banked food is capped at seven days, so a pet cannot be made effectively
immortal in one sitting. Feeding that levels the pet re-syncs staking weight.

Credit items in `pet-stake` (Tonic, Ward, Lens, Charm) are a separate set and
unaffected.

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
| `award_xp` | **`pet-stake` only**, or the owner |
| `migrate` | owner, once |

`award_xp` is where feed XP arrives, via `pet-stake::award_feed_xp`, which is
in turn gated on that contract's `oracle`. Keeping the game contract's rule at
"the staking contract only" is what stops the AI backend needing a key on the
game owner's account, which can mint, pause and transfer pets.

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

## 8 · How the five decisions were resolved

Each is marked `DECISION:` in the source where it is implemented.

**1 · Rewrite or extend → rewrite.** 20 pets and 6 owners is a migration that
fits in one call, and `rarity` plus NEP-171 would have been most of a rewrite
anyway.

**2 · The battle formula → mostly deterministic, with a band.** Each side
scores its effective attack against the other's effective defence, then a
±15% roll from the block seed is applied:

```
attacker_score = atk × (1 + level×0.05) × (1 + star×0.10) × roll(0.85..1.15)
defender_score = def × (1 + level×0.04) × (1 + star×0.10) × roll(0.85..1.15)
```

Both scores go into the `battle_rolled` event, so a player can see why they
lost. The band stops the matchup being perfectly solvable in advance without
making the result a coin flip — which matters, because the seed is not
manipulation-proof.

Pets had no combat stats at all under v1; the client hard-coded 100/100 for
every pet. ATK and DEF are now real fields, seeded from rarity and growing
with level.

**3 · Transfer-vs-staked → a mirrored flag.** `set_locked`, callable only by
`pet-stake`, which fires it from `internal_stake` and `unstake_pet`. The
promise is fire-and-forget: a failed lock leaves a pet transferable briefly,
which is repairable, whereas blocking a stake on a lock call is not worth the
coupling.

**4 · The shop → repriced in NEAR**, see section 4.5.

**5 · Rarity → rolled at mint, with published odds.**

| Rarity | Odds | Base ATK / DEF |
|---|---|---|
| Common | 70% | 100 / 100 |
| Rare | 20% | 130 / 125 |
| Epic | 8% | 170 / 160 |
| Legendary | 2% | 220 / 200 |

`get_mint_odds` returns them on-chain so a client never has to hard-code them.
This is also what makes minting a second pet a real decision rather than a
formality — rarity sets base weight, which sets both credit income and the
item drop rate.

---

## 9 · Acceptance criteria

Deployed and exercised on testnet with
[`contracts/deploy-testnet.sh`](../contracts/deploy-testnet.sh):

| | |
|---|---|
| game | `jg-game-5656.testnet` |
| pet-stake | `jg-stake-5656.testnet` |
| mint-router | `jg-mint-5656.testnet` |

A fresh set of accounts, because the `Game` struct gained `listings`,
`market_fee_bps` and `treasury` — see the note on Borsh below.

| Criterion | Result |
|---|---|
| `stake_pet` succeeds without `admin_set_pet`, refusing a pet the caller does not own | **pass** — `get_pet_attrs` resolved across contracts, weight 100 |
| A staked pet cannot be transferred or listed | **pass** — with a readable error |
| Attacking settles through `report_battle` and the callback applies the damage | **pass** |
| A pet sells for NEAR, fee to the treasury | **pass** — 1 NEAR, 0.025 fee |
| A star is earned after three wins | partial — the streak advances correctly; three wins need three hours of attack cooldown |
| Levelling a staked pet changes its credit rate | **pass**, after a fix — see the full sweep below |
| Paying ZEC through 1Click produces a pet | **not run** — needs a 1Click partner JWT |

### The live run, in order

```
create_pet ×2         → pets 1 and 2, to two different owners
stake_pet             → get_pet_attrs across contracts
                      → pet_staked  weight 100
                      → set_locked  back on the game contract
list_pet (staked)     → "This pet is at work in the AI Lab — unstake it before selling"
unstake_pet           → pet_unstaked + set_locked(false)
attack                → battle_rolled   attacker 93 vs defender 95
                      → the ATTACKER LOST
                      → battle_settled  defender +25 XP, win_streak 1, wins_to_next_star 2
                      → attacker wounded 6h, not attackable
list_pet              → pet_listed      1 NEAR
buy_pet               → pet_sold        fee 0.025 NEAR (2.5%)
                      → ownership moved, listing cleared
```

The attacker losing is worth noting rather than retrying: the ±15% band on an
otherwise even matchup did exactly what it is for. Attacking is not a free win,
which is the point of putting a wager behind it.

### One real problem, found only by deploying — now fixed

Every error message was invisible on-chain. `require!`, `.expect()` and even a
direct `env::panic_str` all surfaced as:

```
ExecutionError("WebAssembly trap: An `unreachable` opcode was executed.")
```

The cause was the build, not the contracts: `cargo near build`'s own help says
its `wasm-opt -O` post-step is **"required for correctness of build on rustc >=
1.82"**, and `build.sh` was running plain
`cargo build --release --target wasm32-unknown-unknown`, skipping it.

Running `cargo near build` surfaced the reason it had never been used here —
`Bulk memory operations require bulk memory`, because rustc now emits
`memory.fill` from precompiled `std` and the wasm-opt bundled with cargo-near
0.13.4 rejects it. Disabling the target feature does not help, since the
instructions come from std rather than from this code. Upgrading to cargo-near
0.22 fixed it.

Verified on testnet after rebuilding through `cargo near build`:

| Site | Before | After |
|---|---|---|
| `require!` in game | `unreachable` | `Smart contract panicked: That pet belongs to someone else` |
| `.expect()` in game | `unreachable` | `…panicked at game/src/lib.rs:180: No such pet` |
| `require!` in pet-stake | `unreachable` | `Smart contract panicked: Nothing to claim yet` |
| `require!` in mint-router | `unreachable` | `Smart contract panicked: This contract only accepts wNEAR` |

The optimised builds are also about 20% smaller — 311 KB rather than 396 KB for
the game contract — which is its own evidence that the pass had been missing.

`build.sh` now goes through `cargo near build` and refuses to run with
cargo-near older than 0.22.

### Changing a stored struct breaks existing state

This has now bitten twice. `win_streak` on `Pet` made the pets already on
testnet unreadable; adding `listings`, `market_fee_bps` and `treasury` to the
contract struct itself meant the whole deployment had to move to fresh
accounts.

On testnet that costs a faucet call. On mainnet it is not an option, and it
applies to any stored struct, not only `Pet`. Either freeze the layouts before
the mainnet deploy, or accept that the first post-launch field addition is a
migration exercise: an `#[init(ignore_state)]` entry point that keeps the
previous struct definition around to deserialise with.

That exercise has since been done, for `oracle` on `PetStake` — see the end of
§10. It works, and it is cheap once the new field is appended rather than
inserted, so there is now a worked example to copy rather than a plan.

## 10 · The full testnet sweep

A second pass over the deployed contracts, this time every path and every
guard rather than the happy one. 44 calls against the three live contracts.

Signers: `jg-p2-72362.testnet` and `jg-p3-72362.testnet` as players,
`jg-game-5656.testnet` as the game owner. Views go through FastNEAR;
`archival-rpc.testnet.near.org`, the near-cli default, rate-limits a run this
size within a minute.

### What passed

```
mint            create_pet 2 NEAR          → pet 3, common blackdragon
                underpaid                  → "Attach at least the mint price"
                mint revenue stays on the game contract until `sweep`
stake           stake_pet (1 yocto)        → weight 100, locked on the game contract
                a pet already staked       → "Pet is already staked"
                list a staked pet          → "…at work in the AI Lab — unstake it before selling"
credits         claim_faucet               → 10,000 once
                claim_faucet again         → "You have already used the faucet"
                claim_mining               → 5,000, then "Mining is still on cooldown"
                claim_credits              → 32,066, matching the view's projection exactly
                spend over balance         → "Not enough credits — claim or convert some first"
                ai_feed                    → credit_spent, lens_applied false
items           buy_credit_item 103        → lens into inventory
                use_item                   → lens_feeds_left 10
                ai_feed with the lens      → lens_applied TRUE, 10 → 9
                list_item 15,000           → listing 1
                buy_listing (not owner)    → seller +14,250, buyer −15,000, rake 750 to reserve
                cancel someone else's      → refused
battle          attack                     → 118 vs 111, winner pet 3
                                           → 9,500 transferred, 500 rake, loser wounded 6h
                                           → win_streak 1, wins_to_next_star 2
                immediate re-attack        → cooldown holds
                pet vs itself / not owner  → both refused
pet market      list_pet 1 NEAR            → pet_listed
                buy_pet 0.5 NEAR           → refused
                buy_pet 1 NEAR             → seller +0.975, treasury +0.025, ownership moved
mint-router     wNEAR + mint_pet msg       → pet 4 to the payer, 14.0 Tgas
                wNEAR + buy_credits msg    → 0.5 wNEAR → 1.000000 credit, debited from the reserve
                wNEAR + nonsense msg       → "Unrecognised payment message — refunding"
guards          grant_credits as a player  → "Only the mint router can grant credits"
                create_pet_for             → "Only the mint router can mint for another account"
                set_locked                 → "Only the staking contract can lock a pet"
                report_battle              → "Only the game contract can settle battles"
                award_xp                   → "Only the staking contract can award XP"
                sweep / set_paused / set_epoch_budget → "Only the owner can call this"
```

Every guard holds and every error message arrives readable, which is the
cargo-near fix above still doing its job.

### The pool was double-counting weight

Found by levelling a staked pet — the one row the first run had left unticked.

`sync_pet` re-reads a pet after it levels up and lands in `on_pet_attrs`, the
same callback a first stake uses, which called `internal_stake` — and
`internal_stake` *added* the weight:

```
stake pet 3                   account 100,  pool 200   (pet 1 + pet 3)
award_xp → level 1 → sync     account 205,  pool 305   ← 100 + 105, for one pet
unstake pet 3                 account 100,  pool 200   ← subtracted only the newer 105
```

The account kept 100 units of weight with nothing staked, and the pool kept
paying out against it: a share of every epoch going to a player with no pet at
work, diluting everyone who did have one. Every level-up and every evolve added
another pet's worth. `account.staked` also listed the pet twice.

All 37 pet-stake tests passed throughout, because every one of them staked
through `admin_set_pet` — which did unwind the old record by hand. Nothing
exercised the callback that production actually uses.

The fix replaces rather than adds: `drop_stake_record` takes the old weight
back out of both the account and the pool before the new one goes in, and
`admin_set_pet` now leans on it instead of repeating it. A re-weigh also stops
reporting itself as a new stake — both emitted `pet_staked`, so an indexer
counted one pet as two.

Re-run live after redeploying:

```
stake pet 4                   pool 200 → 300
award_xp → level 1 → sync     pool 300 → 305   pet_resynced weight 105
unstake pet 4                 pool 305 → 200   account back to pet 1 alone
```

Three Rust tests pin it, and two of them fail on the old code.

### And nothing could repair the damage

The corrupted account was the second finding: no method on the contract could
take phantom weight off. The only repair was a redeploy with fresh state, which
costs every player their stake.

`admin_rebuild_account` is the safety valve — owner-only, recomputes an
account's weight and `staked` list from the `pets` records, which are the
authority, and keeps credits already earned. Used on testnet to clear the 100
phantom units left by the pre-fix round:

```
account_rebuilt  jg-p2-72362.testnet  weight 0
pool             total_weight 100     ← one staked pet, 100 weight. Correct.
```

### Feed XP reaches the chain — fixed

**What the sweep found.** `spend_credits("ai_feed")` debited credits and
emitted `credit_spent`. `/api/ai/chat` verified that receipt, ran the
inference, computed `xpGained` — and returned it to the browser. Nothing
called `award_xp`, which accepts only the staking contract or the game owner,
and no code in this repository called it from either. So a pet's XP stayed 0
however much it was fed: no levels, no weight growth, no evolve, no star gates,
and no reason to feed at all. Confirmed by feeding pet 3 twice and reading
`xp: 0`, then levelling it by hand as the owner, which worked — the on-chain
half was sound, there was simply no caller.

**What happens now.** The award cannot live inside `spend_credits`, because the
taste multiplier is only known after NEAR AI has judged the meal. So it is a
second call, signed by the backend:

```
browser    spend_credits("ai_feed", pet_id, amount)        → credit_spent event
backend    verify the receipt, run the inference, feedXp(match, lens)
backend    pet-stake::award_feed_xp(pet_id, xp)            ← function-call key
pet-stake  game::award_xp(pet_id, min(xp, MAX_FEED_XP))
game       grant_xp → level up → sync_weight → pet-stake
```

Three pieces:

- **`pet-stake::award_feed_xp(pet_id, xp)`**, gated on a new `oracle` account
  (plus the owner, like every other admin method) and settable with
  `set_oracle`. It forwards through the existing `ext_game` interface, so the
  game contract's "only the staking contract" rule stands unchanged. The
  alternative was handing the backend a key on the *game owner's* account,
  which can mint, pause and transfer pets.
- **A cap of `MAX_FEED_XP` = 72**, which is `feedXp(1, true)` — one perfect
  match with a Focus Lens — and nothing more, so the number needs no separate
  justification. `game::award_xp`'s own clamp is 500, seven feeds' worth: wide
  enough for a compromised backend to take a pet five levels in five calls.
  The contract clamps rather than rejecting, and the event records both the
  awarded and the requested value, so an oracle gone wrong is visible in the
  log rather than silently failing.
- **A server signer**, `src/lib/server/near-signer.ts`: hand-written Borsh and
  base58, signing with `node:crypto`, ~300 lines and no new dependency. It
  signs any function call from any key, so auto-feed (issue #8) can use it as
  it stands.

The key is a **function-call** access key on `oracle.jg-stake-5656.testnet`,
limited to `award_feed_xp` on the staking contract. Everything else it was
pointed at was refused by the chain before reaching a contract:

| Attempt with the oracle key | Result |
|---|---|
| `jg-stake::award_feed_xp` | **succeeded** |
| `jg-stake::claim_faucet` | `InvalidAccessKeyError::MethodNameMismatch` |
| `jg-stake::set_oracle` | `InvalidAccessKeyError::MethodNameMismatch` |
| `jg-game::award_xp` (direct) | `InvalidAccessKeyError::ReceiverMismatch` |

`/api/ai/chat` never fails the request over a failed award. By the time it runs
the credits are spent and the inference is paid for, so it degrades the way the
JSON-parse fallback already does and reports `xpAwarded`, `xpTxHash` and
`xpError` in the response. The toast reads the first of those: before this
existed, every feed said "+63 XP" while the pet stayed at `xp: 0`, which is the
worse half of the bug — a player watching for a level that was never coming.

**Proved live**, feeding through the real route against
`jg-stake-5656.testnet`:

| | Before | After |
|---|---|---|
| Pet 4 `xp` | 0 | **60** (taste match 0.9, 1.72x) |
| Pet 1 `level` / `xp`, two feeds | 0 / 0 | **1 / 20** |
| Pet 1 `atk` / `def` | 100 / 100 | **104 / 103** |
| Pool `total_weight` (pet 1 is staked) | 100 | **105** |
| Pet 4 after a 500-XP request from the key | 62 | **level 2, xp 16** — +72, the cap |

The level-up, the stat growth and the staking-weight re-sync all followed from
one feed, which is the whole chain in section 5 running for the first time.
Only the NEAR AI call itself was stubbed: this checkout has no
`NEAR_AI_API_KEY`, so a local endpoint returned an OpenAI-shaped completion
with `taste_match: 0.9`. Everything else — receipt verification, the replay
guard, `feedXp`, the signer, both contracts — was the real thing.

### Adding `oracle` was migrated, not redeployed

`oracle` is a new field on the `PetStake` struct, which is the Borsh layout
change §9 says has already cost this repository two deployments. It was
appended last, so the old bytes are exactly the new layout minus a trailing
`AccountId`, and `migrate` — an `#[init(ignore_state)]` entry point holding the
previous struct definition — reads them and fills in the field.

Fresh accounts were the other option and would have been easier. They were not
taken because on testnet the cost is a faucet call, while the same change on
mainnet has no fresh-accounts option at all: a redeploy with empty state costs
every player their staked pet and their credit balance. Writing the migration
on testnet is the only way to find out whether the migration works.

It does. Deploy and `migrate` go in one transaction, so there is no window in
which the new code is live over state it cannot read, and every number came
back unchanged:

```
before   total_weight 100  staked_pets 1  total_distributed 244399  total_spent 34000  sale_reserve 48986250
after    total_weight 100  staked_pets 1  total_distributed 244399  total_spent 34000  sale_reserve 48986250
         oracle oracle.jg-stake-5656.testnet   max_feed_xp 72
```

`contracts/migrate-testnet.sh` is that path; `deploy-testnet.sh` still runs
`new` and is for fresh accounts only, which its header now says.

### A sink that charged for nothing — removed

`revive` was seeded into `pet-stake`'s `prices` at 1 credit and listed in the
client's "What credits buy" panel, and **no contract implemented it**. There is
no `revive` method anywhere in the three contracts. The panel is a price list
with no buttons, so nobody was ever charged, but a hand-made
`spend_credits("revive")` would have debited a whole credit and done nothing —
the most expensive sink in the game, for no effect.

Reviving a starved pet does work, by the other rail: the game contract's Holy
Water (item 4, 0.50 NEAR) and its `is_revival` flag, which is the only thing in
the system that takes a pet back past its deadline. A second, credit-priced way
to do the same thing was never needed, which is presumably why nobody wrote it.

Same shape as the two findings above it — a price, a UI promise, and nothing
behind them — and the third in this sweep after `award_xp` with no caller and
`auto_feed_budget` with no spender.

Removing it from `new`'s seed only fixes fresh deployments: the price lives in
contract state. So `remove_action_price(action)` is new, owner-only, and
`set_action_price(action, 0)` is **not** a substitute — zero passes
`amount >= price` and `balance >= price`, so the action stays live and merely
becomes free. A unit test pins that distinction down so nobody simplifies the
method away.

### Smaller things

- **Pending credits are not spendable.** `harvest` folds earnings into
  `pending`; only `claim_credits` moves `pending` into `credit_balance`, and
  `spend_credits`, `buy_credit_item` and `buy_listing` all check the balance.
  A player with 0 balance and 31,750 pending is told to claim first. The
  message says so, but the client should batch the claim into the same
  transaction rather than make them find the button.
- **`battle_settled` is emitted twice**, once by each contract, under the same
  event name. An indexer counting battles counts double.
- **Views lag the transaction.** A read immediately after a call returns
  pre-call state at `finality: final`; the lens looked like it had done nothing
  until the block settled. Worth knowing before reporting a bug.

### The client called a failed action a success

Not a contract problem, but the sweep is what exposed it: staking a pet
belonging to somebody else came back as a **clean success**.

`stake_pet` only schedules the cross-contract read; the ownership check is in
the callback, which logs and returns `false`. A NEAR transaction reports success
as soon as its first receipt is accepted, so nothing in
`src/lib/near/tx.ts` saw a failure — not for this, and not for a receipt that
panics outright.

Both halves are closed now: `assertReceiptsSucceeded` walks every receipt and
throws the contract's own message, wired into every signed call, and `stakePet`
additionally reads the log line a callback leaves when it refuses by returning
rather than panicking.
