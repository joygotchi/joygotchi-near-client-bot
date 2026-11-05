# JOYGOTCHI

An on-chain pet game on NEAR that runs in an ordinary web browser — desktop,
tablet or phone. Pets live in a NEAR contract, you pay for them in whatever coin
you already hold, and they earn real AI compute credit while they work in the
AI Lab.

Built with **SvelteKit 3 + Svelte 5 + Tailwind 4**, **NEAR Connect** (HOT Wallet
and Meteor Wallet), **NEAR Intents 1Click** for multichain payments, and
**NEAR AI Cloud** for the parts where the pet actually thinks.

## What it does

- **Connect** with Meteor, HOT Wallet or MyNearWallet through
  [`@hot-labs/near-connect`](https://github.com/hot-dao/near-selector), with a
  picker covering eight more. MyNearWallet needs no extension and no app, so
  the game is playable from any browser.
- **Mint a pet with ZEC, SOL, BTC or USDC.** The app quotes the swap through
  NEAR Intents, shows a deposit address and QR, and watches the swap until the
  pet lands on NEAR. No bridging, no NEAR balance needed first.
- **Feed with AI.** Your pet decides what it wants to eat; a good taste match is
  worth up to 1.8× the XP. The call really does run on NEAR AI Cloud.
- **Stake pets in the AI Lab** to earn AI credit every second, proportional to
  pet weight. A staked pet can neither attack nor be attacked — that is the
  trade.
- **Find and trade items.** Playing rolls for drops, and a stronger pet finds
  more and better things — 3% for a fresh pet, 24% for a maxed one. Items sell
  to other players for credits.
- **Battle for credits, not for blood.** The loser's wager goes to the winner
  and the pet is wounded for a while. Nothing dies in a fight — see
  [docs/ECONOMY.md](docs/ECONOMY.md) for why.
- **Mine** for credits every six hours, if you have a pet at work.

The full economic design, with worked numbers and the open issues, is in
[docs/ECONOMY.md](docs/ECONOMY.md).

## Getting started

```bash
pnpm install
cp .env.example .env
pnpm dev
```

The app runs without any secrets: contracts fall back to the mainnet defaults
and every panel renders. Two features need a key:

| Feature | Variable | Where to get it |
|---|---|---|
| Multichain payments at partner rates | `ONECLICK_JWT` | [partners.near-intents.org](https://partners.near-intents.org/home) |
| AI feed / train / craft / chat | `NEAR_AI_API_KEY` | [cloud.near.ai](https://cloud.near.ai) |

Without `ONECLICK_JWT` quotes still work, but carry an extra 0.25%
unauthenticated fee, and the UI says so. Without `NEAR_AI_API_KEY` the AI
endpoint returns a 503 and the rest of the game is unaffected.

Both are **server-only**. They are declared in [`src/env.ts`](src/env.ts) without
the `public` flag, so SvelteKit refuses to let them into a client bundle.

Multichain payments also need `mint-router` deployed, because 1Click validates
the NEAR recipient before it will quote — and reports a missing account as an
opaque `400 Internal server error`. `/api/intents/quote` checks the account
first and says so plainly instead. See [contracts/README.md](contracts/README.md)
for the deploy commands.

## Commands

```bash
pnpm dev       # dev server, reachable on the LAN
pnpm build     # production build (adapter-vercel)
pnpm preview   # serve the production build
pnpm check     # svelte-check: types, a11y, unused props
```

```bash
cd contracts
cargo test     # 49 unit tests across both contracts
./build.sh     # wasm artifacts into contracts/res/
```

## Layout

```
src/
├── env.ts                  typed, validated, documented env vars
├── app.css                 Tailwind 4 theme + the pixel-art helpers
├── lib/nav.ts              one nav definition, shared by the bar and the dock
├── lib/
│   ├── config.ts           contract ids, gas presets, settlement asset
│   ├── near/
│   │   ├── wallet.svelte.ts  NEAR Connect in a Svelte 5 rune store
│   │   ├── rpc.ts            view calls over plain fetch — SSR-safe, no polyfills
│   │   ├── tx.ts             signAndSendTransaction helpers
│   │   └── amount.ts         BigInt fixed-point, so no near-api-js in the bundle
│   ├── intents/            1Click types, asset catalogue, browser client
│   ├── economy/            tokenomics parameters, staking client, AI actions
│   ├── ai/                 pet personalities and prompts
│   ├── game/pets.ts        the existing pet contract, typed
│   ├── server/             1Click client + on-chain receipt verification
│   ├── platform.ts         haptics, external links, ?ref= referrals
│   ├── stores/             player state, toasts
│   └── components/
│       ├── AppShell.svelte   header + content container + mobile dock
│       └── …
└── routes/
    ├── +page.svelte        home: pet, stats, AI feed, item shop
    ├── stake/              the AI Lab
    ├── market/             drops, inventory and the item market
    ├── mint/               mint · top up · settings
    ├── battle/ mining/ mission/ space/
    └── api/
        ├── intents/{tokens,quote,status,deposit}
        ├── ai/chat         metered NEAR AI proxy
        └── credits

contracts/
├── pet-stake/    the credit ledger: staking, items, drops, market, battles
└── mint-router/  receives wNEAR from 1Click and mints the pet
```

## Responsive layout

Every page renders through
[`AppShell.svelte`](src/lib/components/AppShell.svelte), so width and navigation
are decided in one place:

| Width | Navigation | Content |
|---|---|---|
| `< 768px` | pixel-art dock fixed to the bottom | single column |
| `≥ 768px` | links in the header bar | two columns on the pet and AI Lab screens, up to 720px |
| `≥ 1280px` | same | up to 1040px |

The app used to be pinned to a 400px column by hand-written CSS
(`.screen`, `.fix-header`), which is gone.

## Design notes

**No `near-api-js`.** View calls are plain `fetch` against the JSON-RPC, and
amounts are BigInt fixed-point in
[`src/lib/near/amount.ts`](src/lib/near/amount.ts). That removes the Buffer and
process polyfills the React build needed, and it means pet data can be read
during SSR.

**The RPC has to allow CORS.** The browser reads contract state directly, and
`rpc.mainnet.near.org` sends no `Access-Control-Allow-Origin` — every view call
from a page fails and balances read as zero. `PUBLIC_NEAR_RPC_URL` therefore
defaults to FastNEAR (`free.rpc.fastnear.com`, `test.rpc.fastnear.com`), chosen
by network. Any replacement must send CORS headers.

**The browser never talks to 1Click.** Every call goes through
`/api/intents/*`. The quote endpoint is purpose-driven, not a proxy: the client
sends *which coin* and *which refund address*, and the server decides the
recipient, the amount and the message. A tampered request cannot redirect funds.

**AI spending is metered on-chain.** `/api/ai/chat` will not run inference
without the hash of a successful, unused `spend_credits` transaction signed by
the account asking. See the "Why the AI key is safe" section of
[docs/ECONOMY.md](docs/ECONOMY.md).

**`#lib` instead of `$lib`.** SvelteKit 3 removed `$lib` in favour of Node
subpath imports. The mapping lives in `package.json#imports`, and `tsconfig.json`
repeats it under `paths` because TypeScript does not substitute file extensions
through an imports map.

## Migrating from the React app

The previous Vite + React client is in git history at `f1fea1f`. Everything it
could do is here, with these changes:

| Before | Now |
|---|---|
| `@here-wallet/core` | `@hot-labs/near-connect` (Meteor, HOT, MyNearWallet + 8 more) |
| `localStorage` as app state | Svelte 5 rune stores, read from chain |
| `Swap.tsx` — a mock, "Coming soon" | real 1Click swaps for mint and top-up |
| `near-api-js` + node polyfills | plain fetch + BigInt |
| react-router with a lazy `useRoutes` | SvelteKit file routes |
| alert()/status strings per page | one toast store |
| Telegram Mini App, 400px column | a responsive website: desktop, tablet, phone |
| `@twa-dev/sdk`, Telegram haptics | Vibration API, no platform SDK |
| referral via Telegram `start_param` | `?ref=` on the URL |
| "1 JOY" labels with no token behind them | NEAR in, AI credit out — two real resources |
| — | the AI Lab, AI feeding, item drops, an item market |
