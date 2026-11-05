# JOYGOTCHI contracts

Three contracts, built with `near-sdk` 5.30. The economic reasoning behind them
is in [../docs/ECONOMY.md](../docs/ECONOMY.md); this file is the operational
side.

```bash
cargo test     # 24 unit tests
cargo clippy --all-targets
./build.sh     # -> res/{joy_token,pet_stake,mint_router}.wasm
```

## joy-token — $JOY

A plain NEP-141 with a minter allow-list. $JOY is the soft currency: earned by
mining, battling and missions, spent on items and on the one-way conversion to
AI credit.

| Method | Who | Notes |
|---|---|---|
| `new(owner_id, total_supply)` | deploy | mints the initial supply to the owner |
| `mint(account_id, amount, memo)` | allow-listed minters | registers storage for the receiver if needed |
| `burn(amount, memo)` | anybody, own balance | every sink is visible on-chain |
| `add_minter` / `remove_minter` | owner | keep this list short: game, faucet, mint router |

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
| `buy_credit_item(item_id, pet_id)` | player | items 101–104, see below |
| `claim_faucet()` | player | 0.01 credit, **once per account**, from `sale_reserve` |
| `set_auto_feed_budget(budget)` | player | micro-credits per day, 0 = off |
| `report_battle(winner_pet, winner_id, loser_pet, loser_id)` | **game contract** | moves the wager, wounds the loser, returns `star_protected` |
| `ft_on_transfer` | $JOY only | `{"action":"buy_credits"}`; refunds if the reserve is dry |
| `set_epoch_budget(amount)` | owner | **the one number that sets total emission** |
| `fund_reserve(credits)` | owner | record credits actually bought on NEAR AI |
| `set_action_price(action, credits)` | owner | keep in step with `src/lib/economy/params.ts` |
| `set_item_price(item_id, credits)` | owner | same |
| `set_battle_params(wager, rake_bps)` | owner | rake capped at 20% |
| `admin_set_pet(pet_id, owner_id, attrs)` | owner | escape hatch, see below |
| `get_pool()` / `get_account(id)` / `get_staked_pets(id)` | view | |
| `is_staked(id)` / `is_attackable(id)` / `get_pet_state(id)` / `get_pet_states(ids)` | view | |
| `get_items()` / `get_item_price(id)` / `get_action_price(action)` | view | |

### Credit items

| id | Item | Effect |
|---|---|---|
| 101 | AI Tonic | clears a wound now |
| 102 | Sentinel Ward | un-attackable 24h; a second ward extends rather than restarts |
| 103 | Focus Lens | +0.25 XP multiplier on the next 10 AI feeds |
| 104 | Star Charm | keeps a star on the next loss, then is consumed |

Every effect is pet state with a deadline or a counter. None of them changes
staking weight: the accumulator is exact, and a weight that moved on a deadline
would need the pool updated when nothing is calling it.

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
| `ft_on_transfer` | wNEAR only | reads the `customRecipientMsg` from the quote |
| `redeem_pending(account_id, index)` | anybody | retries a paid-but-unminted claim |
| `get_pending(account_id)` | view | paid mints not yet created |
| `sweep(amount)` | owner | forwards collected wNEAR to the treasury |
| `set_mint_price` / `set_joy_per_wnear` / `set_game_contract` | owner | |

Payment message:

```json
{ "action": "mint_pet", "account_id": "alice.near",
  "category": "blackdragon", "name": "SMAUG", "quantity": 1 }

{ "action": "buy_joy", "account_id": "alice.near" }
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
be an allow-listed minter on `joy-token` for `buy_joy` to work.

## Deploying

```bash
export NEAR_ENV=mainnet
OWNER=joyv1.near

near deploy joy.joyv1.near   res/joy_token.wasm \
  --initFunction new --initArgs "{\"owner_id\":\"$OWNER\",\"total_supply\":\"1000000000000000000000000000\"}"

near deploy stake.joyv1.near res/pet_stake.wasm \
  --initFunction new --initArgs "{\"owner_id\":\"$OWNER\",\"game_contract\":\"game.joyv1.near\",\"joy_token\":\"joy.joyv1.near\",\"epoch_budget\":\"500000000\"}"

near deploy mint.joyv1.near  res/mint_router.wasm \
  --initFunction new --initArgs "{\"owner_id\":\"$OWNER\",\"game_contract\":\"game.joyv1.near\",\"joy_token\":\"joy.joyv1.near\",\"wnear\":\"wrap.near\",\"treasury\":\"treasury.joyv1.near\",\"mint_price\":\"2000000000000000000000000\",\"joy_per_wnear\":\"1000000000000000000000\"}"

# the router mints $JOY for buy_joy, and needs somewhere to put wNEAR
near call joy.joyv1.near add_minter '{"account_id":"mint.joyv1.near"}' --accountId "$OWNER"
near call wrap.near storage_deposit '{"account_id":"mint.joyv1.near"}' \
  --accountId "$OWNER" --deposit 0.00125

# credits the treasury has actually bought, available for $JOY conversion + faucet
near call stake.joyv1.near fund_reserve '{"credits":"50000000"}' --accountId "$OWNER"
```

Then put the deployed ids into `.env` as `PUBLIC_CONTRACT_JOY`,
`PUBLIC_CONTRACT_STAKE` and `PUBLIC_CONTRACT_MINT_ROUTER`.
