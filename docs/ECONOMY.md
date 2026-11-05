# JOYGOTCHI economy

## The one idea

Most tamagotchi-with-a-token games mint their premium currency out of nothing,
then spend the rest of their life defending the price. JOYGOTCHI does not have a
premium token. It has **AI credit**, and one credit is one US dollar of real
NEAR AI Cloud compute.

That changes the shape of the whole economy:

```
   ZEC / SOL / BTC                 (players bring outside money)
          │
          │  NEAR Intents · 1Click
          ▼
       wNEAR  ────────────────▶  treasury
          │                          │
          │                          │  stake with NEAR AI
     mint-router                     ▼
          │                   stake-to-compute
          ▼                          │
        a pet                        │  monthly $ allowance
          │                          ▼
          │  stake in the AI Lab   epoch_budget  (published on-chain)
          ▼                          │
      pet-stake  ◀───────────────────┘
          │
          │  streamed by weight, every second
          ▼
      AI credit
          │
          │  spent on AI feed, training, crafting, chat
          ▼
   NEAR AI inference  (the credit is consumed for real)
```

Emission is bounded by something that exists outside the game: the compute
allowance NEAR AI grants for the treasury's staked NEAR. We cannot print more
credit than we can actually pay for, and every credit a player spends is
compute we genuinely buy. There is no float to defend, because credits are
never tradeable.

## The two resources

| | **$JOY** | **AI credit (ACR)** |
|---|---|---|
| Standard | NEP-141, 18 decimals | account balance in `pet-stake`, 6 decimals |
| Transferable | yes | **no, on purpose** |
| Where it comes from | mining claims, battle wins, missions | the treasury's NEAR AI allowance, streamed to pet stakers |
| Where it goes | items, renaming, training fees, conversion to credit | AI feeding, training, crafting, chat, evolving, reviving |
| Peg | none, it floats | 1 ACR = US$1 of NEAR AI compute |
| Role | the grind currency | the scarce, *useful* currency |

$JOY is the thing you earn by showing up. AI credit is the thing that makes your
pet better, and it cannot be farmed — only staked for, or bought at a bad rate.

## How credit accrues

Stake a pet in the **AI Lab** (`pet-stake`) and it starts working. While staked:

- it **cannot battle** — that is the real cost, not a cosmetic one,
- it **still starves**, so you still have to feed it,
- it earns a share of the stream proportional to its weight.

```
weight = base(rarity) × (1 + level/20) × (1 + star × 0.15) × evolution

base:        common 100 · rare 160 · epic 260 · legendary 400
evolution:   phase 1 ×1.00 · phase 2 ×1.15 · phase 3 ×1.35 · phase 4 ×1.60
```

A brand-new common pet is weight 100. A fully maxed legendary (level 20, 5
stars, phase 4) is weight 2,240 — about 22 commons, not 22,000. Growth is
sub-linear on purpose: one whale must not be able to flatten the stream for a
hundred casual players, or the AI Lab stops being worth opening.

Your rate is then:

```
credits_per_second = (epoch_budget / 30 days) × (your_weight / total_weight)
```

This is the standard accumulator pattern — the same `reward_debt` shape the
existing pet contract already uses, so it should read as familiar. The accrual
is continuous, which matters for feel: the number in the header ticks up while
you look at it, and claiming is one transaction that batches whatever has piled
up.

### A worked example

Treasury stakes 50,000 NEAR with NEAR AI. At the published rate (≈US$5/month of
credits per 500 NEAR staked) that is **$500/month**, so
`epoch_budget = 500_000_000` micro-credits.

| Pool | Your pets | Your weight | Pool weight | Your credits/day | What that buys daily |
|---|---|---|---|---|---|
| early | 1 common | 100 | 5,000 | 0.33 | 16 AI feeds |
| growing | 1 common | 100 | 50,000 | 0.033 | 1 AI feed |
| growing | 3 pets, lvl 10 | 900 | 50,000 | 0.30 | 15 AI feeds |
| mature | 3 pets, lvl 10 | 900 | 400,000 | 0.037 | 1 AI feed + change |

Note what happens as the game grows: a single unupgraded pet drifts toward "one
feed a day", while an invested player stays comfortably above water. That is the
intended pressure — it pushes levelling and evolving, which are themselves
credit sinks, which pulls credits back out of circulation.

And when the pool grows, revenue grows with it: more players means more
multichain mints, means more wNEAR staked, means a larger `epoch_budget`. The
treasury raises the number on-chain with `set_epoch_budget`.

## What credits buy

| Action | Cost | $JOY too | Real inference | Effect |
|---|---|---|---|---|
| **AI Feed** | 0.002 ACR | — | yes | The pet decides what it wants to eat. Matching its taste multiplies XP by **1.0–1.8×** |
| **Train** | 0.008 ACR | 10 | yes | A generated sparring drill raises ATK or DEF |
| **Craft item** | 0.02 ACR | 25 | yes | An item rolled from your pet's own history |
| **Talk to pet** | 0.001 ACR | — | yes | One message, answered in the pet's voice |
| **Evolve** | 0.50 ACR | 100 | no | Next evolution phase — permanently more weight |
| **Revive** | 1.00 ACR | — | no | Bring a dying pet back |
| **Auto-feed** | 0.015 ACR | — | no | The pet feeds itself while you are away |

### On the price of a feed

An earlier version of this document claimed these prices were "roughly what the
model call costs". That was wrong, and wrong in a way that mattered: a ~200-token
completion costs on the order of **$0.0004**, so the original 0.02 ACR was a
~50× markup, not cost recovery.

The markup itself is fine — it is the margin that funds the game. The problem
was what it did to playability:

| Price of a feed | Feeds per month from a 500 ACR budget | Real NEAR AI cost | Feeds/day at 1,000 active players |
|---|---|---|---|
| 0.02 ACR | 25,000 | ~$10 | **0.8** |
| 0.005 ACR | 100,000 | ~$40 | 3.3 |
| **0.002 ACR** | 250,000 | ~$100 | **8.3** |

Under one feed per player per day is unplayable — pets starve faster than that.
The prices in the table above are the corrected ones: still a 5× margin over
real inference, but a game you can actually play.

AI Feed is the centrepiece. It is a slot machine you can actually read: the pet
has a taste profile, you do not know exactly what it wants today, and a good
match is worth nearly twice the XP of a bad one. It gives a reason to open the
app that does not reduce to "tap the button".

## Items

Two shops, because two contracts own them.

**The game contract's $JOY shop** is fixed on-chain and cannot be extended from
here. Its real contents, read back out of the `items` array on live pets:

| id | Item | Effect |
|---|---|---|
| 1 | Beef | +3h before starving, 1,000 points |
| 2 | Water | +3h before starving, 10,000 points |
| 3 | Shield | blocks one attack |
| 4 | Holy Water | revives a starved pet |

The old React client had Water on id 1 and Beef on id 2 — the wrong way round,
so buying one gave you the other. Fixed in `JOY_ITEMS`.

**Credit items** live in `pet-stake`, which is why there can be new ones:

| id | Item | Cost | Effect |
|---|---|---|---|
| 101 | AI Tonic | 0.01 ACR | clears a wound immediately |
| 102 | Sentinel Ward | 0.05 ACR | un-attackable for 24h; stacks rather than resetting |
| 103 | Focus Lens | 0.02 ACR | +0.25 XP multiplier on the next 10 AI feeds |
| 104 | Star Charm | 0.015 ACR | keeps your star if you lose the next battle |

Every effect here is pet *state* with a deadline or a counter. **None of them
touch staking weight.** That is deliberate: the credit accumulator is exact and
well tested, and a weight that changed on a deadline would need the pool updated
at a moment when nothing is calling it. Keeping items out of the weight maths is
what makes them safe to add.

The Focus Lens is consumed by the contract inside `spend_credits`, and whether it
applied goes into the `credit_spent` event. The backend reads that event from the
transaction logs, so the bonus cannot be claimed by a client that merely says it
has one.

## After a battle: nothing dies

Pets do not die from fighting. Three reasons, in order of weight.

**1. Death plus a share-based stream pays people to grief.** Credits are
distributed by `your_weight / total_weight`. Killing another staker's pet lowers
`total_weight`, which **raises the killer's own income**. That is not a side
effect, it is a computable profit. Death and this stream must not ship together.

**2. The defender is offline and did not choose.** The attacker picks the
target. Dying of neglect is your own doing; losing a fight you started is a risk
you took; having a stranger destroy a pet you paid for while you sleep is
neither. That third case is the one that makes people uninstall.

**3. Death's economic job is already done.** Supply sinks exist to keep mint
demand alive, but pool dilution does that already: as `total_weight` grows your
share shrinks, so you always want more or better pets. No need to destroy what
players bought.

So losing a battle costs this instead:

| | |
|---|---|
| Wager | 0.01 ACR, moved from loser to winner |
| Rake | 5% of the pot → `sale_reserve`, never out of `epoch_budget` |
| Wound | 6 hours unable to battle; an AI Tonic clears it at once |
| Star | lost, unless a Star Charm absorbs it |

The wager is a **transfer, not a destruction**: the loser's loss is the winner's
gain, so PvP is zero-sum between players rather than eating into the asset base
the treasury has to back with real compute. Rake funds the reserve behind $JOY
conversions and the faucet, so PvP never dilutes stakers.

A loser who cannot cover the wager still loses the fight — the pot is simply
whatever they had. Battles never fail for being poor.

### Staked pets are off limits in both directions

`is_attackable(pet_id)` returns false while a pet is staked, warded or wounded,
and the game contract must consult it. A staked pet not being able to *attack*
was never enough on its own: without the other half, staking would mark you as a
target, and cutting a staker's weight is exactly what an attacker profits from.
The AI Lab would have been a honeypot.

## Keeping it smooth

The failure mode of every pet game is the player who comes back to a dead pet
and never opens the app again. Four things guard against it:

1. **Credits never hard-stop you.** Out of credits? Feed with $JOY instead —
   `JOY_FEED_COST` for 40% of the XP. Worse, never blocked.
2. **Auto-feed.** Set a daily budget and your pets keep themselves alive from
   your credit balance while you sleep. Capped by you, off by default.
3. **A one-off faucet.** `claim_faucet` grants 0.01 credit, once per account,
   from the treasury's sale reserve — five AI feeds, enough to see the good part
   before you own anything.

   It used to be 0.05 a day, which was a hole rather than a gift: NEAR implicit
   accounts are nearly free to create, so a script with a thousand accounts
   would have drawn $1,500/month against a $500 budget. One grant per account
   caps the damage at a finite amount instead of a rate.
4. **Accrual is visible.** The header ticks. You can watch the thing you earn.

## $JOY ↔ credit

One way only: **$JOY → credit, at 400 JOY per credit**, and never back.

That rate is deliberately poor. Its job is not to be a good deal — it is to be
a permanent $JOY sink and a safety valve for a player who wants to evolve a pet
*today*. Conversions are served out of a separate `sale_reserve` that the
treasury tops up with `fund_reserve` only after it has actually bought those
credits on NEAR AI. When the reserve runs dry the contract **refunds the $JOY**
rather than issuing credit it cannot back.

Credits do not convert back, and cannot be transferred between accounts. This is
the single most important constraint in the design: the moment credits become
tradeable they become a second speculative asset with an implicit redemption
promise behind it, and the treasury is on the hook for a float it never agreed
to carry.

## Multichain money-in

The player picks ZEC, SOL, BTC or USDC. The app asks our server for a 1Click
quote (`EXACT_OUTPUT`, so the price is fixed in wNEAR and any excess is refunded
automatically), shows a deposit address and a QR, and polls until the swap
settles.

1Click withdraws the wNEAR to `mint-router` with `ft_transfer_call`, carrying
the `customRecipientMsg` our server attached to the quote. The router validates
the payment, queues the mint, and calls the game contract.

The server builds the entire quote: the client sends only *which coin* and
*which refund address*. Recipient, amount and message come from server config,
so a tampered request cannot redirect funds or mint a pet for free. The 1Click
partner JWT never reaches the browser.

Two details worth knowing:

- **Overpayment is change, not a donation.** The router returns the excess and
  1Click forwards it to the player's refund address on the origin chain.
- **A mint that fails is not a mint that is lost.** If the game contract rejects
  the call (paused, out of gas, mid-upgrade), the paid claim stays in
  `get_pending` and anyone can retry it with `redeem_pending`.

## Why the AI key is safe

The NEAR AI key sits on the server, and the endpoint that uses it is public. The
metering is therefore on-chain, in two steps:

1. The browser sends `spend_credits` to `pet-stake`. The contract charges **its
   own configured price**, not whatever the client asked for, debits the
   balance, and emits `credit_spent`.
2. The browser hands `/api/ai/chat` that transaction hash. The server looks the
   transaction up on the RPC and checks it succeeded, was signed by the account
   asking, went to our staking contract, called `spend_credits`, carried the
   action about to be performed, and **has not been presented before**.

No valid, unused receipt, no inference. The replay guard is in memory, which is
enough for one instance and for local development — point it at Redis or Vercel
KV before production, or two concurrent requests can share one receipt.

## Parameters, and where they live

Every number above exists in exactly two places, which must be kept in step:

- [`src/lib/economy/params.ts`](../src/lib/economy/params.ts) — what the client
  shows and projects.
- [`contracts/pet-stake/src/lib.rs`](../contracts/pet-stake/src/lib.rs) — what
  is actually enforced. Prices are seeded in `new()` and changeable afterwards
  with `set_action_price`.

The contract is the authority. The client numbers are for display and for
telling the player what something will cost before they sign.

| Parameter | Value | Set by |
|---|---|---|
| `epoch_budget` | 500 ACR / 30 days | treasury, `set_epoch_budget` |
| `joy_per_credit` | 400 JOY | treasury, `set_joy_per_credit` |
| `faucet_amount` | 0.01 ACR, once per account | treasury, `set_faucet_amount` |
| `battle_wager` | 0.01 ACR | treasury, `set_battle_params` |
| `battle_rake_bps` | 500 (5%, capped at 20%) | treasury, `set_battle_params` |
| ward / wound | 24h / 6h | fixed in `types.rs` |
| mint price | 2 wNEAR | `mint-router::set_mint_price` |
| credit decimals | 6 | fixed |
| rarity weights | 100 / 160 / 260 / 400 | fixed in `Rarity::base_weight` |

## Open issues

Written down because they are real, not because they are solved:

- **The game contract does not call our views yet.** `pet-stake` exposes
  `is_attackable(pet_id)` and `report_battle(...)`, and the client respects
  them, but the deployed game contract knows about neither. Until it does, a
  determined player can attack a staked or warded pet by talking to the game
  contract directly, and no battle settles any credits.
- **Pets are not transferable at all.** `game.joyv1.near` is not NEP-171 — it
  has no `nft_token`, `nft_transfer` or `nft_approve` (verified against mainnet).
  So there is no marketplace and no exit liquidity: minting is a one-way
  purchase, which caps what anyone will pay for a pet, which caps the mint
  revenue that funds `epoch_budget`.
- **Nobody has ever battled.** Of the 20 pets on mainnet, zero have ever
  attacked or been attacked, and none is above level 1. The wager above is the
  first reason to fight that the game has had; whether it is enough is untested.
- **`get_pet_attrs`.** `stake_pet` reads the pet's owner and attributes from the
  game contract through this view, which the current deployment does not expose.
  `admin_set_pet` is the interim path for an operator or indexer to push them.
- **Rarity is not on-chain.** The client derives a stand-in from star count. The
  real field belongs in pet metadata.
- **Auto-feed is budgeted but not yet executed.** The budget is stored
  on-chain; something has to actually spend it on a schedule — a cron worker
  calling `spend_credits` on the player's behalf needs a delegated key, which is
  a design decision in its own right.
- **Credit accounting is trust-minimised, not trustless.** The on-chain ledger
  says what players are owed; the treasury's NEAR AI balance is what honours it.
  `get_pool` publishes `total_distributed`, `total_spent` and `sale_reserve` so
  the two can be reconciled publicly, but the backing itself is custodial.
