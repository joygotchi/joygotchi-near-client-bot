import { WNEAR_CONTRACT } from '#lib/config';

export type PayAsset = {
  /** 1Click asset id. */
  assetId: string;
  symbol: string;
  /** Human-readable chain name. */
  chain: string;
  /** 1Click `blockchain` value. */
  blockchain: string;
  decimals: number;
  icon: string;
  /** How a refund address on this chain looks, shown as input placeholder. */
  addressHint: string;
};

/**
 * Assets a player can pay a mint with. Asset ids come from
 * `GET https://1click.chaindefuser.com/v0/tokens`; the live token list is
 * fetched at runtime so prices and decimals are never stale.
 */
export const PAY_ASSETS: PayAsset[] = [
  {
    assetId: 'nep141:zec.omft.near',
    symbol: 'ZEC',
    chain: 'Zcash',
    blockchain: 'zec',
    decimals: 8,
    icon: '/assets/icon/bronze.svg',
    addressHint: 't1… or u1… (transparent or unified address)'
  },
  {
    assetId: 'nep141:sol.omft.near',
    symbol: 'SOL',
    chain: 'Solana',
    blockchain: 'sol',
    decimals: 9,
    icon: '/assets/icon/planet.svg',
    addressHint: 'Base58 Solana address'
  },
  {
    assetId: 'nep141:sol-5ce3bf3a31af18be40ba30f721101b4341690186.omft.near',
    symbol: 'USDC',
    chain: 'Solana',
    blockchain: 'sol',
    decimals: 6,
    icon: '/assets/icon/gold.svg',
    addressHint: 'Base58 Solana address'
  },
  {
    assetId: 'nep141:btc.omft.near',
    symbol: 'BTC',
    chain: 'Bitcoin',
    blockchain: 'btc',
    decimals: 8,
    icon: '/assets/icon/diamond.svg',
    addressHint: 'bc1… address'
  }
];

/** Everything settles in wNEAR on NEAR, which is what the game contracts speak. */
export const SETTLEMENT_ASSET = `nep141:${WNEAR_CONTRACT}`;

export const findPayAsset = (assetId: string) =>
  PAY_ASSETS.find((asset) => asset.assetId === assetId);
