/**
 * Types for the NEAR Intents 1Click API (https://1click.chaindefuser.com/docs/v0/openapi.yaml).
 * Only the fields JOYGOTCHI uses are modelled.
 */

export type SwapType = 'EXACT_INPUT' | 'EXACT_OUTPUT' | 'FLEX_INPUT' | 'ANY_INPUT';
export type DepositType = 'ORIGIN_CHAIN' | 'INTENTS' | 'CONFIDENTIAL_INTENTS';
export type RefundType = DepositType;
export type RecipientType = 'DESTINATION_CHAIN' | 'INTENTS' | 'CONFIDENTIAL_INTENTS';
export type DepositMode = 'SIMPLE' | 'MEMO';

export type SwapStatus =
  | 'PENDING_DEPOSIT'
  | 'KNOWN_DEPOSIT_TX'
  | 'INCOMPLETE_DEPOSIT'
  | 'PROCESSING'
  | 'SUCCESS'
  | 'REFUNDED'
  | 'FAILED';

export type TokenResponse = {
  assetId: string;
  decimals: number;
  blockchain: string;
  symbol: string;
  price: number;
  priceUpdatedAt: string;
  contractAddress?: string;
  coingeckoId?: string;
};

export type QuoteRequest = {
  dry: boolean;
  swapType: SwapType;
  slippageTolerance: number;
  originAsset: string;
  depositType: DepositType;
  destinationAsset: string;
  amount: string;
  refundTo: string;
  refundType: RefundType;
  recipient: string;
  recipientType: RecipientType;
  deadline: string;
  depositMode?: DepositMode;
  customRecipientMsg?: string;
  referral?: string;
  appFees?: Array<{ recipient: string; fee: number }>;
  quoteWaitingTimeMs?: number;
};

export type Quote = {
  depositAddress?: string;
  depositMemo?: string;
  amountIn: string;
  amountInFormatted: string;
  amountInUsd: string;
  minAmountIn: string;
  amountOut: string;
  amountOutFormatted: string;
  amountOutUsd: string;
  minAmountOut: string;
  deadline?: string;
  timeWhenInactive?: string;
  timeEstimate: number;
  refundFee?: string;
  withdrawFee?: string;
};

export type QuoteResponse = {
  correlationId: string;
  timestamp: string;
  /** Keep this together with the quote — it is the proof for any dispute. */
  signature: string;
  quoteRequest: QuoteRequest;
  quote: Quote;
};

export type TransactionDetails = { hash: string; explorerUrl: string };

export type SwapDetails = {
  intentHashes?: string[];
  nearTxHashes?: string[];
  amountIn?: string;
  amountInFormatted?: string;
  amountOut?: string;
  amountOutFormatted?: string;
  originChainTxHashes?: TransactionDetails[];
  destinationChainTxHashes?: TransactionDetails[];
  refundedAmount?: string;
  refundedAmountFormatted?: string;
};

export type ExecutionStatus = {
  correlationId: string;
  quoteResponse: QuoteResponse;
  status: SwapStatus;
  updatedAt: string;
  swapDetails: SwapDetails;
};

export const TERMINAL_STATUSES: SwapStatus[] = ['SUCCESS', 'REFUNDED', 'FAILED'];

export const STATUS_LABEL: Record<SwapStatus, string> = {
  PENDING_DEPOSIT: 'Waiting for your deposit',
  KNOWN_DEPOSIT_TX: 'Deposit seen on-chain',
  INCOMPLETE_DEPOSIT: 'Deposit is below the quoted amount',
  PROCESSING: 'Swapping across chains',
  SUCCESS: 'Done',
  REFUNDED: 'Refunded',
  FAILED: 'Failed'
};
