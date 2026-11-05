import { defineEnvVars } from '@sveltejs/kit/env';

/** A string variable with a fallback, so a missing `.env` never breaks the build. */
const withDefault = (fallback: string) => (value: string | undefined) =>
  value && value.trim().length > 0 ? value.trim() : fallback;

/** A variable that is allowed to be empty (empty string means "not configured"). */
const optional = () => (value: string | undefined) => (value ?? '').trim();

const integer = (fallback: number) => (value: string | undefined) => {
  if (value === undefined || value.trim() === '') return fallback;
  const n = Number(value);
  if (!Number.isInteger(n)) throw new Error(`expected an integer, got "${value}"`);
  return n;
};

const decimal = (fallback: string) => (value: string | undefined) => {
  const raw = value?.trim() || fallback;
  if (!/^\d+(\.\d+)?$/.test(raw)) throw new Error(`expected a decimal number, got "${raw}"`);
  return raw;
};

export const variables = defineEnvVars({
  // ── Public: NEAR ────────────────────────────────────────────────────────────
  PUBLIC_NEAR_NETWORK: {
    public: true,
    schema: (value: string | undefined) => {
      const v = value?.trim() || 'mainnet';
      if (v !== 'mainnet' && v !== 'testnet') throw new Error('must be "mainnet" or "testnet"');
      return v as 'mainnet' | 'testnet';
    },
    description: 'NEAR network the app talks to.'
  },
  PUBLIC_NEAR_RPC_URL: {
    public: true,
    schema: withDefault('https://rpc.mainnet.near.org'),
    description: 'JSON-RPC endpoint used for view calls.'
  },

  // ── Public: contracts ───────────────────────────────────────────────────────
  PUBLIC_CONTRACT_GAME: {
    public: true,
    schema: withDefault('game.joyv1.near'),
    description: 'Pet / game contract (create_pet, attack, buy_item, …).'
  },
  PUBLIC_CONTRACT_FT: {
    public: true,
    schema: withDefault('faucet.joyv1.near'),
    description: 'Legacy faucet contract that hands out JOY (get_joychi).'
  },
  PUBLIC_CONTRACT_JOY: {
    public: true,
    schema: withDefault('joy.joyv1.near'),
    description: '$JOY NEP-141 token contract.'
  },
  PUBLIC_CONTRACT_STAKE: {
    public: true,
    schema: withDefault('stake.joyv1.near'),
    description: 'Pet-staking contract that accrues NEAR AI credits.'
  },
  PUBLIC_CONTRACT_MINT_ROUTER: {
    public: true,
    schema: withDefault('mint.joyv1.near'),
    description: 'Receives wNEAR from 1Click and mints a pet (ft_on_transfer).'
  },
  PUBLIC_TREASURY_ACCOUNT: {
    public: true,
    schema: withDefault('treasury.joyv1.near'),
    description: 'Fallback destination for multichain mint payments.'
  },
  PUBLIC_MINT_PRICE_WNEAR: {
    public: true,
    schema: decimal('2'),
    description: 'Price of one pet, in wNEAR (human units).'
  },

  // ── Public: Telegram ────────────────────────────────────────────────────────
  PUBLIC_TG_BOT_ID: {
    public: true,
    schema: withDefault('joygotchi_bot/game'),
    description: 'Telegram Mini App id, used by the in-Telegram HOT wallet flow.'
  },
  PUBLIC_TG_WALLET_ID: {
    public: true,
    schema: withDefault('herewalletbot/app'),
    description: 'Telegram wallet app id for HOT wallet.'
  },

  // ── Private: NEAR Intents 1Click ────────────────────────────────────────────
  ONECLICK_JWT: {
    schema: optional(),
    description:
      'NEAR Intents 1Click partner JWT from https://partners.near-intents.org/home. ' +
      'Server-only. Without it quotes still work but carry an extra 0.25% fee.'
  },
  ONECLICK_BASE_URL: {
    schema: withDefault('https://1click.chaindefuser.com'),
    description: '1Click API base URL (without the /v0 suffix).'
  },
  ONECLICK_REFERRAL: {
    schema: withDefault('joygotchi'),
    description: 'Distribution-channel identifier sent with every quote (lowercase).'
  },
  ONECLICK_FEE_BPS: {
    schema: integer(0),
    description: 'App fee on every swap, in basis points (100 = 1%).'
  },
  ONECLICK_FEE_RECIPIENT: {
    schema: optional(),
    description: 'Intents account that receives ONECLICK_FEE_BPS. Required if the fee is > 0.'
  },

  // ── Private: NEAR AI Cloud ──────────────────────────────────────────────────
  NEAR_AI_API_KEY: {
    schema: optional(),
    description: 'NEAR AI Cloud API key from https://cloud.near.ai. Server-only.'
  },
  NEAR_AI_BASE_URL: {
    schema: withDefault('https://cloud-api.near.ai/v1'),
    description: 'NEAR AI Cloud OpenAI-compatible gateway.'
  },
  NEAR_AI_MODEL: {
    schema: withDefault('zai-org/GLM-5.1-FP8'),
    description: 'Default model used for pet AI interactions.'
  }
});
