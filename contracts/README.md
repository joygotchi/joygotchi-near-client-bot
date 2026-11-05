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
| `spend_credits(action, pet_id, amount)` | player | charges the contract's own price; emits `credit_spent` |
| `claim_faucet()` | player | 0.05 credit, once per 24h, from `sale_reserve` |
| `set_auto_feed_budget(budget)` | player | micro-credits per day, 0 = off |
| `ft_on_transfer` | $JOY only | `{"action":"buy_credits"}`; refunds if the reserve is dry |
| `set_epoch_budget(amount)` | owner | **the one number that sets total emission** |
| `fund_reserve(credits)` | owner | record credits actually bought on NEAR AI |
| `set_action_price(action, credits)` | owner | keep in step with `src/lib/economy/params.ts` |
| `admin_set_pet(pet_id, owner_id, attrs)` | owner | escape hatch, see below |
| `get_pool()` / `get_account(id)` / `get_staked_pets(id)` / `is_staked(id)` | view | |

`get_account` projects the accumulator forward to the current block, so the
number it returns is what a claim right now would actually pay.

### What it needs from the game contract

```rust
fn get_pet_attrs(&self, pet_id: u64) -> (AccountId, PetAttrs);

// PetAttrs { rarity: "common"|"rare"|"epic"|"legendary",
//            level: u32, star: u32, evolution_phase: u8 }
```

The currently deployed `game.joyv1.near` does not expose this. Until it does,
use `admin_set_pet` from an operator account or indexer. The game contract
should also call `is_staked(pet_id)` in `attack` — the client blocks the button,
but only the contract can enforce it.

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
