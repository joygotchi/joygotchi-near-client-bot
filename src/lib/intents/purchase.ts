import type { QuoteResponse } from './types';

export type PurchasePurpose = 'mint_pet' | 'buy_credits';

/** What the browser sends to `POST /api/intents/quote`. */
export type PurchaseQuoteRequest = {
  purpose: PurchasePurpose;
  /** 1Click asset id of the coin the player pays with (must be in `PAY_ASSETS`). */
  originAsset: string;
  /** Address on the origin chain that receives a refund if the swap fails. */
  refundTo: string;
  /** NEAR account that receives the pet or the credits. */
  accountId: string;
  /** Number of pets. Ignored when buying credits. Defaults to 1. */
  quantity?: number;
  /** Pet species for `mint_pet`, e.g. `blackdragon`. */
  petCategory?: string;
  petName?: string;
  /** `true` to price the swap without reserving a deposit address. */
  dry?: boolean;
};

/** What `POST /api/intents/quote` answers. */
export type PurchaseQuoteResponse = {
  purpose: PurchasePurpose;
  quote: QuoteResponse;
  /** Human amount of wNEAR the game will receive. */
  settlementAmount: string;
  /** `msg` the mint router will receive through `ft_on_transfer`. */
  routerMsg: string;
  /** `false` when ONECLICK_JWT is unset — the swap then costs 0.25% more. */
  authenticated: boolean;
};

export type MintRouterMsg = {
  action: 'mint_pet';
  account_id: string;
  category: string;
  name: string;
  quantity: number;
};

export type BuyCreditsMsg = {
  action: 'buy_credits';
  account_id: string;
};
