import { WNEAR_CONTRACT } from '#lib/config';

/**
 * Where a player can pay from, and with what.
 *
 * Payment is a deposit to an address 1Click hands out, so any wallet on a
 * supported chain works — the list below is the chains, not the wallets. It
 * started as four assets (ZEC, SOL, Solana USDC, BTC), which left out every
 * MetaMask, TronLink and TON user. The asset ids and decimals come from
 * `GET https://1click.chaindefuser.com/v0/tokens`; a quote for an id that is
 * not there fails, so `assets.test.ts` keeps them honest against the shape.
 */

/** How a chain's addresses are written, for validating the refund address. */
export type AddressFamily = 'evm' | 'solana' | 'bitcoin' | 'zcash' | 'tron' | 'ton' | 'doge' | 'xrp' | 'sui' | 'aptos' | 'litecoin';

export type PayChain = {
  /** 1Click's `blockchain` value. */
  blockchain: string;
  name: string;
  family: AddressFamily;
  /** Wallets players are likely to have for it — a hint, not a whitelist. */
  wallets: string;
  addressHint: string;
};

export type PayAsset = {
  assetId: string;
  symbol: string;
  /** The chain's display name. */
  chain: string;
  blockchain: string;
  decimals: number;
  icon: string;
  addressHint: string;
};

const EVM = { family: 'evm' as const, addressHint: '0x… address', wallets: 'MetaMask, Rabby, Coinbase Wallet' };

export const PAY_CHAINS: PayChain[] = [
  { blockchain: 'btc', name: 'Bitcoin', family: 'bitcoin', wallets: 'Any Bitcoin wallet', addressHint: 'bc1…, 1… or 3… address' },
  { blockchain: 'eth', name: 'Ethereum', ...EVM },
  { blockchain: 'base', name: 'Base', ...EVM },
  { blockchain: 'arb', name: 'Arbitrum', ...EVM },
  { blockchain: 'op', name: 'Optimism', ...EVM },
  { blockchain: 'bsc', name: 'BNB Chain', ...EVM, wallets: 'MetaMask, Trust Wallet, Binance Web3' },
  { blockchain: 'pol', name: 'Polygon', ...EVM },
  { blockchain: 'avax', name: 'Avalanche', ...EVM, wallets: 'MetaMask, Core' },
  { blockchain: 'sol', name: 'Solana', family: 'solana', wallets: 'Phantom, Solflare, Backpack', addressHint: 'Base58 Solana address' },
  { blockchain: 'tron', name: 'Tron', family: 'tron', wallets: 'TronLink, Trust Wallet', addressHint: 'T… address' },
  { blockchain: 'ton', name: 'TON', family: 'ton', wallets: 'Tonkeeper, Telegram Wallet', addressHint: 'UQ… or EQ… address' },
  { blockchain: 'zec', name: 'Zcash', family: 'zcash', wallets: 'Zashi, YWallet', addressHint: 't1… or u1… (transparent or unified address)' },
  { blockchain: 'doge', name: 'Dogecoin', family: 'doge', wallets: 'Any Dogecoin wallet', addressHint: 'D… address' },
  { blockchain: 'ltc', name: 'Litecoin', family: 'litecoin', wallets: 'Any Litecoin wallet', addressHint: 'ltc1…, L… or M… address' },
  { blockchain: 'xrp', name: 'XRP Ledger', family: 'xrp', wallets: 'Xaman, Ledger', addressHint: 'r… address' },
  { blockchain: 'sui', name: 'Sui', family: 'sui', wallets: 'Sui Wallet, Suiet', addressHint: '0x… 64-hex address' },
  { blockchain: 'aptos', name: 'Aptos', family: 'aptos', wallets: 'Petra, Pontem', addressHint: '0x… 64-hex address' }
];

const COIN = '/assets/icon/bronze.svg';
const STABLE = '/assets/icon/gold.svg';
const GAS = '/assets/icon/planet.svg';
const BTCISH = '/assets/icon/diamond.svg';

/** [blockchain, symbol, assetId, decimals, icon] as 1Click lists them. */
const ROWS: [string, string, string, number, string][] = [
  ['btc', 'BTC', 'nep141:btc.omft.near', 8, BTCISH],
  ['eth', 'ETH', 'nep141:eth.omft.near', 18, GAS],
  ['eth', 'USDC', 'nep141:eth-0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48.omft.near', 6, STABLE],
  ['eth', 'USDT', 'nep141:eth-0xdac17f958d2ee523a2206206994597c13d831ec7.omft.near', 6, STABLE],
  ['base', 'ETH', 'nep141:base.omft.near', 18, GAS],
  ['base', 'USDC', 'nep141:base-0x833589fcd6edb6e08f4c7c32d4f71b54bda02913.omft.near', 6, STABLE],
  ['arb', 'ETH', 'nep141:arb.omft.near', 18, GAS],
  ['arb', 'USDC', 'nep141:arb-0xaf88d065e77c8cc2239327c5edb3a432268e5831.omft.near', 6, STABLE],
  ['op', 'ETH', 'nep245:v2_1.omni.hot.tg:10_11111111111111111111', 18, GAS],
  ['bsc', 'BNB', 'nep141:bnb.omdep.near', 18, GAS],
  ['bsc', 'USDT', 'nep141:bnb-0x55d398326f99059ff775485246999027b3197955.omdep.near', 18, STABLE],
  ['bsc', 'USDC', 'nep141:bnb-0x8ac76a51cc950d9822d68b83fe1ad97b32cd580d.omdep.near', 18, STABLE],
  ['pol', 'POL', 'nep141:pol.omdep.near', 18, GAS],
  ['pol', 'USDC', 'nep141:pol-0x3c499c542cef5e3811e1192ce70d8cc03d5c3359.omdep.near', 6, STABLE],
  ['avax', 'AVAX', 'nep141:avax.omdep.near', 18, GAS],
  ['sol', 'SOL', 'nep141:sol.omft.near', 9, GAS],
  ['sol', 'USDC', 'nep141:sol-5ce3bf3a31af18be40ba30f721101b4341690186.omft.near', 6, STABLE],
  ['sol', 'USDT', 'nep141:sol-c800a4bd850783ccb82c2b2c7e84175443606352.omft.near', 6, STABLE],
  ['tron', 'TRX', 'nep141:tron.omft.near', 6, GAS],
  ['tron', 'USDT', 'nep141:tron-d28a265909efecdcee7c5028585214ea0b96f015.omft.near', 6, STABLE],
  // 1Click lists native TON under its old name, GRAM.
  ['ton', 'TON', 'nep245:v2_1.omni.hot.tg:1117_', 9, GAS],
  ['ton', 'USDT', 'nep245:v2_1.omni.hot.tg:1117_3tsdfyziyc7EJbP2aULWSKU4toBaAcN4FdTgfm5W1mC4ouR', 6, STABLE],
  ['zec', 'ZEC', 'nep141:zec.omft.near', 8, COIN],
  ['doge', 'DOGE', 'nep141:doge.omft.near', 8, COIN],
  ['ltc', 'LTC', 'nep141:ltc.omft.near', 8, COIN],
  ['xrp', 'XRP', 'nep141:xrp.omft.near', 6, COIN],
  ['sui', 'SUI', 'nep141:sui.omft.near', 9, GAS],
  ['aptos', 'APT', 'nep141:aptos.omft.near', 8, GAS]
];

export const findPayChain = (blockchain: string) =>
  PAY_CHAINS.find((chain) => chain.blockchain === blockchain);

export const PAY_ASSETS: PayAsset[] = ROWS.map(([blockchain, symbol, assetId, decimals, icon]) => {
  const chain = findPayChain(blockchain)!;
  return { assetId, symbol, chain: chain.name, blockchain, decimals, icon, addressHint: chain.addressHint };
});

export const assetsOn = (blockchain: string) =>
  PAY_ASSETS.filter((asset) => asset.blockchain === blockchain);

export const SETTLEMENT_ASSET = `nep141:${WNEAR_CONTRACT}`;

export const findPayAsset = (assetId: string) =>
  PAY_ASSETS.find((asset) => asset.assetId === assetId);

const PATTERNS: Record<AddressFamily, RegExp> = {
  evm: /^0x[0-9a-fA-F]{40}$/,
  solana: /^[1-9A-HJ-NP-Za-km-z]{32,44}$/,
  bitcoin: /^(bc1[02-9ac-hj-np-z]{11,71}|[13][1-9A-HJ-NP-Za-km-z]{25,34})$/,
  zcash: /^(t1|t3)[1-9A-HJ-NP-Za-km-z]{33}$|^u1[02-9ac-hj-np-z]{100,}$/,
  tron: /^T[1-9A-HJ-NP-Za-km-z]{33}$/,
  ton: /^(EQ|UQ|kQ|0Q)[A-Za-z0-9_-]{46}$|^-?[0-9]:[0-9a-fA-F]{64}$/,
  doge: /^[DA9][1-9A-HJ-NP-Za-km-z]{25,34}$/,
  litecoin: /^(ltc1[02-9ac-hj-np-z]{11,71}|[LM3][1-9A-HJ-NP-Za-km-z]{26,33})$/,
  xrp: /^r[1-9A-HJ-NP-Za-km-z]{24,34}$/,
  sui: /^0x[0-9a-fA-F]{64}$/,
  aptos: /^0x[0-9a-fA-F]{1,64}$/
};

/**
 * Why a refund address is wrong for its chain, or null if it looks right.
 *
 * Checked before the quote because a refund to a mistyped address is money
 * nobody can get back — the old check only asked for eight characters.
 */
export function refundAddressError(blockchain: string, address: string): string | null {
  const chain = findPayChain(blockchain);
  if (!chain) return 'That chain is not accepted';
  const trimmed = address.trim();
  if (trimmed === '') return `Enter your ${chain.name} address for refunds`;
  return PATTERNS[chain.family].test(trimmed)
    ? null
    : `That does not look like a ${chain.name} address (${chain.addressHint})`;
}
