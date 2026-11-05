import {
  PUBLIC_CONTRACT_GAME,
  PUBLIC_CONTRACT_MINT_ROUTER,
  PUBLIC_CONTRACT_STAKE,
  PUBLIC_MINT_PRICE_WNEAR,
  PUBLIC_NEAR_NETWORK,
  PUBLIC_NEAR_RPC_URL,
  PUBLIC_TREASURY_ACCOUNT
} from '$app/env/public';

export const NETWORK = PUBLIC_NEAR_NETWORK;

/**
 * JSON-RPC endpoint. Defaults to FastNEAR because the browser calls it
 * directly and it has to allow cross-origin requests — `rpc.mainnet.near.org`
 * sends no `Access-Control-Allow-Origin`, so every view call from a page fails
 * and balances read as zero.
 */
export const RPC_URL =
  PUBLIC_NEAR_RPC_URL ||
  (NETWORK === 'mainnet' ? 'https://free.rpc.fastnear.com' : 'https://test.rpc.fastnear.com');

export const GAME_CONTRACT = PUBLIC_CONTRACT_GAME;
export const STAKE_CONTRACT = PUBLIC_CONTRACT_STAKE;
export const MINT_ROUTER_CONTRACT = PUBLIC_CONTRACT_MINT_ROUTER;
export const TREASURY_ACCOUNT = PUBLIC_TREASURY_ACCOUNT;

export const MINT_PRICE_WNEAR = PUBLIC_MINT_PRICE_WNEAR;

/** wNEAR is the settlement asset for every multichain payment. */
export const WNEAR_CONTRACT = NETWORK === 'mainnet' ? 'wrap.near' : 'wrap.testnet';

export const EXPLORER_URL =
  NETWORK === 'mainnet' ? 'https://nearblocks.io' : 'https://testnet.nearblocks.io';

/** Gas presets, in gas units. */
export const GAS = {
  view: '30000000000000',
  call: '30000000000000',
  heavy: '100000000000000',
  /** ft_transfer_call needs room for the receiver's callback. */
  transferCall: '100000000000000'
} as const;

/** 1 yoctoNEAR, required by NEP-141 transfer methods. */
export const ONE_YOCTO = '1';
