# JOYGOTCHI contracts

Three contracts, built with `near-sdk` 5.30. The economic reasoning behind them
is in [../docs/ECONOMY.md](../docs/ECONOMY.md); this file is the operational
side.

```bash
cargo test     # 81 unit tests
cargo clippy --all-targets
./build.sh     # -> res/{game,pet_stake,mint_router}.wasm
```

There is no game token. NEAR is the only money, and AI credit — a balance in
`pet-stake` that cannot be transferred — is the only thing players earn and
spend. See [../docs/ECONOMY.md](../docs/ECONOMY.md) for why.

## game — the pets

A NEP-171 collection with the game on top, replacing `game.joyv1.near`, which
is not NEP-171, has no combat stats, and stores a status field it only
refreshes when something touches the pet. Built to
[../docs/GAME_CONTRACT_V2.md](../docs/GAME_CONTRACT_V2.md), which also records
the five design decisions taken along the way.

| Method | Who | Notes |
|---|---|---|
| `create_pet(name, category)` | anyone, payable | rarity rolled at mint, odds in `get_mint_odds` |
| `create_pet_for(owner, name, category)` | **mint router** | a bridge pays, so the caller is never the owner |
| `buy_item(pet_id, item_id)` | pet owner, payable | the shop, priced in NEAR; feeding is capped at 7 banked days |
| `evolve(pet_id)` | pet owner | needs `level >= phase × 5`; re-syncs staking weight |
| `attack(pet_id, target_pet_id)` | pet owner | settles through `pet-stake::report_battle` |
| `award_xp(pet_id, xp)` | **staking contract** | XP for an action it charged credits for |
| `set_locked(pet_id, locked)` | **staking contract** | a locked pet cannot be transferred or approved |
| `import_pets(Vec<LegacyPet>)` | owner | migration; preserves `pet_id`, idempotent |
| `get_pet_attrs(pet_id)` | view | **what `pet-stake` calls** — owner plus rarity/level/star/phase |
| `get_pet` / `get_all_pet_metadata` / `get_pets_for_owner` | view | status is derived, not stored |
| NEP-171 core, enumeration, approval | | `token_id` is the decimal `pet_id` |

### The ordering in `attack`

```
attack → report_battle FIRST → callback applies damage
```

The other way round looks equivalent and is not: `report_battle` reverts for
any pet that is staked, warded or wounded, and applying damage first would mean
a protected pet had already been hurt. Two unit tests pin this down — one that
`attack` mutates nothing, and one that an `Err` settlement leaves the loser
untouched.

**`pet-stake` never holds funds.** It has no payable method and accepts no
token. Every payment lands on `mint-router`, which calls `grant_credits`. One
contract in the system custodies money, and it is not the one with the game
logic.

## pet-stake — the AI Lab

Streams the treasury's NEAR AI compute allowance to whoever has pets staked,
using the usual accumulator pattern.

| Method | Who | Notes |
|---|---|---|
| `stake_pet(pet_id)` | player, 1 yocto | reads owner + attributes from the game contract |
| `unstake_pet(pet_id)` | player, 1 yocto | accrued credit stays in `pending` |
| `sync_pet(pet_id)` | anybody | re-reads attributes after a level-up or evolution |
| `claim_credits()` | player | `pending` → `credit_balance` |
| `spend_credits(action, pet_id, amount)` | player | charges the contract's own price; consumes a Focus Lens; emits `credit_spent` |
| `claim_faucet()` | player | 0.01 credit, **once per account**, from `sale_reserve` |
| `claim_mining()` | player | 0.005 credit every 6h; requires staked weight |
| `grant_credits(account_id, credits)` | **mint router** | returns what the reserve could cover, so the router can refund the rest |
| `buy_credit_item(item_id, quantity)` | player | into your inventory |
| `use_item(item_id, pet_id)` | player | applies one from the inventory |
| `list_item` / `buy_listing` / `cancel_listing` | player | the item market, 5% rake to the reserve |
| `set_auto_feed_budget(budget)` | player | micro-credits per day, 0 = off |
| `report_battle(winner_pet, winner_id, loser_pet, loser_id)` | **game contract** | moves the wager, wounds the loser, returns `star_protected` |
| `set_epoch_budget(amount)` | owner | **the one number that sets total emission** |
| `fund_reserve(credits)` | owner | record credits actually bought on NEAR AI |
| `set_action_price(action, credits)` | owner | keep in step with `src/lib/economy/params.ts` |
| `set_item_price(item_id, credits)` | owner | same |
| `set_mining_reward` / `set_market_rake_bps` / `set_granter` | owner | |
| `set_battle_params(wager, rake_bps)` | owner | rake capped at 20% |
| `admin_set_pet(pet_id, owner_id, attrs)` | owner | escape hatch, see below |
| `get_pool()` / `get_account(id)` / `get_staked_pets(id)` | view | |
| `is_staked(id)` / `is_attackable(id)` / `get_pet_state(id)` / `get_pet_states(ids)` | view | |
| `get_items()` / `get_item_price(id)` / `get_action_price(action)` | view | |
| `get_inventory(id)` / `get_listings(…)` / `get_listings_by(id)` | view | |
| `get_drop_odds(id)` / `mining_available_at(id)` | view | |

### Drops

Every AI feed, training session, craft, mining claim and battle win rolls for an
item. The chance is `min(25%, 2% + weight basis points)` and the weight also
gates which items can appear — a maxed legendary is at 24.4% and can find
anything; a fresh common is at 3% and finds tonics.

Items sell between players, never back to the contract: drops are free and the
rate rises with strength, so a contract buyback would be an inflation pump with
a difficulty slider on it.

### Credit items

| id | Item | Effect |
|---|---|---|
| 101 | AI Tonic | clears a wound now |
| 102 | Sentinel Ward | un-attackable 24h; a second ward extends rather than restarts |
| 103 | Focus Lens | +0.25 XP multiplier on the next 10 AI feeds |
| 104 | Star Charm | keeps a star on the next loss, then is consumed |

Every effect is pet state with a deadline or a counter. None of them changes
staking weight: the accumulator is exact, and a weight that moved on a deadline
would need the pool updated when nothing is calling it. That is also what makes
items safe to drop and trade.

### Battles

`report_battle` moves the wager from loser to winner, takes 5% rake into
`sale_reserve`, and wounds the loser for 6 hours. **No pet dies** — credits are
streamed by weight share, so destroying a staker's pet would raise the
attacker's own income. See the "After a battle" section of
[../docs/ECONOMY.md](../docs/ECONOMY.md).

Owner accounts are arguments rather than lookups: a pet that can be attacked is
by definition not staked, so this contract holds no record of who owns it. The
game contract is the authority and the only caller allowed.

`get_account` projects the accumulator forward to the current block, so the
number it returns is what a claim right now would actually pay.

### What it needs from the game contract

Specified in full, with the call ordering that matters, in
[../docs/GAME_CONTRACT_V2.md](../docs/GAME_CONTRACT_V2.md).

```rust
fn get_pet_attrs(&self, pet_id: u64) -> (AccountId, PetAttrs);

// PetAttrs { rarity: "common"|"rare"|"epic"|"legendary",
//            level: u32, star: u32, evolution_phase: u8 }
```

The currently deployed `game.joyv1.near` does not expose this (verified against
mainnet: it is not NEP-171 either — no `nft_token`, `nft_transfer` or
`nft_approve`). Until it does, use `admin_set_pet` from an operator account or
indexer.

The game contract also needs two changes for battles to mean anything:

```rust
// before allowing an attack
stake.is_attackable(pet_id) -> bool

// after deciding a winner
stake.report_battle(winner_pet_id, winner_id, loser_pet_id, loser_id) -> BattleOutcome
```

The client already respects both, but only the contract can enforce them.

## mint-router — multichain mints

Receives wNEAR from 1Click and turns it into a pet or into $JOY.

| Method | Who | Notes |
|---|---|---|
| `ft_on_transfer` | wNEAR only | reads the `customRecipientMsg` from the quote; mints a pet or buys credits |
| `redeem_pending(account_id, index)` | anybody | retries a paid-but-unminted claim |
| `get_pending(account_id)` | view | paid mints not yet created |
| `sweep(amount)` | owner | forwards collected wNEAR to the treasury |
| `set_mint_price` / `set_near_per_credit` / `set_game_contract` | owner | |

Payment message:

```json
{ "action": "mint_pet", "account_id": "alice.near",
  "category": "blackdragon", "name": "SMAUG", "quantity": 1 }

{ "action": "buy_credits", "account_id": "alice.near" }
```

Behaviour that matters: underpayment and unknown categories are refunded in
full; overpayment is returned as change so 1Click forwards it to the player's
refund address; and a mint the game contract rejects stays claimable rather than
being lost.

### What it needs from the game contract

```rust
fn create_pet_for(&mut self, owner_id: AccountId, name: String, category: String) -> u64;
```

The payment arrives from a bridge, not from the player's wallet, so the router
has to be able to assign a pet to somebody other than the caller. It must also
be set as `granter` on `pet-stake` for `buy_credits` to work.

## Deploying

```bash
export NEAR_ENV=mainnet
OWNER=joyv1.near

near deploy stake.joyv1.near res/pet_stake.wasm \
  --initFunction new --initArgs "{\"owner_id\":\"$OWNER\",\"game_contract\":\"game.joyv1.near\",\"granter\":\"mint.joyv1.near\",\"epoch_budget\":\"500000000\"}"

near deploy mint.joyv1.near  res/mint_router.wasm \
  --initFunction new --initArgs "{\"owner_id\":\"$OWNER\",\"game_contract\":\"game.joyv1.near\",\"stake_contract\":\"stake.joyv1.near\",\"wnear\":\"wrap.near\",\"treasury\":\"treasury.joyv1.near\",\"mint_price\":\"2000000000000000000000000\",\"near_per_credit\":\"500000000000000000000000\"}"

# the router needs somewhere to put the wNEAR it collects
near call wrap.near storage_deposit '{"account_id":"mint.joyv1.near"}' \
  --accountId "$OWNER" --deposit 0.00125

# credits the treasury has actually bought, backing sales, mining and the faucet
near call stake.joyv1.near fund_reserve '{"credits":"50000000"}' --accountId "$OWNER"
```

Then put the deployed ids into `.env` as `PUBLIC_CONTRACT_STAKE` and
`PUBLIC_CONTRACT_MINT_ROUTER`.
