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
| **AI Feed** | 0.02 ACR | — | yes | The pet decides what it wants to eat. Matching its taste multiplies XP by **1.0–1.8×** |
| **Train** | 0.05 ACR | 10 | yes | A generated sparring drill raises ATK or DEF |
| **Craft item** | 0.10 ACR | 25 | yes | An item rolled from your pet's own history |
| **Talk to pet** | 0.005 ACR | — | yes | One message, answered in the pet's voice |
| **Evolve** | 0.50 ACR | 100 | no | Next evolution phase — permanently more weight |
| **Revive** | 1.00 ACR | — | no | Bring a dying pet back |
| **Auto-feed** | 0.015 ACR | — | no | The pet feeds itself while you are away |

The four actions marked *real inference* are why this works. Their cost is not a
number someone invented for a tokenomics slide — it is roughly what the model
call costs. The sink can never run away from the source.

AI Feed is the centrepiece. It is a slot machine you can actually read: the pet
has a taste profile, you do not know exactly what it wants today, and a good
match is worth nearly twice the XP of a bad one. It gives a reason to open the
app that does not reduce to "tap the button".

## Keeping it smooth

The failure mode of every pet game is the player who comes back to a dead pet
and never opens the app again. Four things guard against it:

1. **Credits never hard-stop you.** Out of credits? Feed with $JOY instead —
   `JOY_FEED_COST` for 40% of the XP. Worse, never blocked.
2. **Auto-feed.** Set a daily budget and your pets keep themselves alive from
   your credit balance while you sleep. Capped by you, off by default.
3. **A daily faucet.** `claim_faucet` grants 0.05 credit a day from the
   treasury's sale reserve — enough to try AI Feed twice before you own
   anything. New players get to see the good part first.
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
| `faucet_amount` | 0.05 ACR / day | treasury, `set_faucet_amount` |
| mint price | 2 wNEAR | `mint-router::set_mint_price` |
| credit decimals | 6 | fixed |
| rarity weights | 100 / 160 / 260 / 400 | fixed in `Rarity::base_weight` |

## Open issues

Written down because they are real, not because they are solved:

- **Staked pets and battling.** `pet-stake` exposes `is_staked(pet_id)` and the
  client blocks the attack button, but the deployed game contract does not yet
  call it. Until it does, a determined player could attack with a staked pet by
  talking to the game contract directly.
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
