# JOYGOTCHI

An on-chain pet game that runs as a Telegram Mini App on NEAR. Pets live in a
NEAR contract, you pay for them in whatever coin you already hold, and they earn
real AI compute credit while they work in the AI Lab.

Built with **SvelteKit 3 + Svelte 5 + Tailwind 4**, **NEAR Connect** (HOT Wallet
and Meteor Wallet), **NEAR Intents 1Click** for multichain payments, and
**NEAR AI Cloud** for the parts where the pet actually thinks.

## What it does

- **Connect** with HOT Wallet or Meteor Wallet through
  [`@hot-labs/near-connect`](https://github.com/hot-dao/near-selector) — one tap
  inside Telegram, a wallet picker everywhere else.
- **Mint a pet with ZEC, SOL, BTC or USDC.** The app quotes the swap through
  NEAR Intents, shows a deposit address and QR, and watches the swap until the
  pet lands on NEAR. No bridging, no NEAR balance needed first.
- **Feed with AI.** Your pet decides what it wants to eat; a good taste match is
  worth up to 1.8× the XP. The call really does run on NEAR AI Cloud.
- **Stake pets in the AI Lab** to earn AI credit every second, proportional to
  pet weight. A staked pet cannot battle — that is the trade.
- **Battle, mine and run missions** as before.

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

### Telegram

The Mini App needs HTTPS, so point a tunnel at the dev server:

```bash
cloudflared tunnel --url http://localhost:5173
```

`vite.config.ts` already sets `allowedHosts: true` for this.

## Commands

```bash
pnpm dev       # dev server, reachable on the LAN
pnpm build     # production build (adapter-vercel)
pnpm preview   # serve the production build
pnpm check     # svelte-check: types, a11y, unused props
```

```bash
cd contracts
cargo test     # unit tests for all three contracts
./build.sh     # wasm artifacts into contracts/res/
```

## Layout

```
src/
├── env.ts                  typed, validated, documented env vars
├── app.css                 Tailwind 4 theme + the pixel-art helpers
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
│   ├── stores/             player state, toasts
│   └── components/
└── routes/
    ├── +page.svelte        home: pet, stats, AI feed, item shop
    ├── stake/              the AI Lab
    ├── mint/               mint · top up · settings
    ├── battle/ mining/ mission/ space/
    └── api/
        ├── intents/{tokens,quote,status,deposit}
        ├── ai/chat         metered NEAR AI proxy
        └── credits

contracts/
├── joy-token/    NEP-141 $JOY with a minter allow-list
├── pet-stake/    pet staking → AI credit stream
└── mint-router/  receives wNEAR from 1Click and mints the pet
```

## Design notes

**No `near-api-js`.** View calls are plain `fetch` against the JSON-RPC, and
amounts are BigInt fixed-point in
[`src/lib/near/amount.ts`](src/lib/near/amount.ts). That removes the Buffer and
process polyfills the React build needed, and it means pet data can be read
during SSR.

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
| `@here-wallet/core` | `@hot-labs/near-connect` (HOT + Meteor + 8 more) |
| `localStorage` as app state | Svelte 5 rune stores, read from chain |
| `Swap.tsx` — a mock, "Coming soon" | real 1Click swaps for mint and top-up |
| `near-api-js` + node polyfills | plain fetch + BigInt |
| react-router with a lazy `useRoutes` | SvelteKit file routes |
| alert()/status strings per page | one toast store |
| — | the AI Lab, AI feeding, $JOY, three contracts |
