import { describe, expect, it } from 'vitest';
import { PAY_ASSETS, PAY_CHAINS, assetsOn, findPayChain, refundAddressError } from './assets';

describe('the pay list', () => {
  it('has one entry per asset id', () => {
    expect(new Set(PAY_ASSETS.map((asset) => asset.assetId)).size).toBe(PAY_ASSETS.length);
  });

  it('puts every asset on a chain it declares, and every chain has something to pay with', () => {
    for (const asset of PAY_ASSETS) expect(findPayChain(asset.blockchain)).toBeTruthy();
    for (const chain of PAY_CHAINS) expect(assetsOn(chain.blockchain).length).toBeGreaterThan(0);
  });

  it('covers the wallets people actually hold', () => {
    const chains = PAY_CHAINS.map((chain) => chain.blockchain);
    for (const needed of ['btc', 'eth', 'base', 'bsc', 'sol', 'tron', 'ton', 'zec']) {
      expect(chains).toContain(needed);
    }
  });
});

describe('refundAddressError', () => {
  // Real-format addresses for each family; none is anyone's in particular.
  const good: [string, string][] = [
    ['eth', '0x52908400098527886E0F7030069857D2E4169EE7'],
    ['bsc', '0x52908400098527886E0F7030069857D2E4169EE7'],
    ['sol', '9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM'],
    ['btc', 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq'],
    ['btc', '1BoatSLRHtKNngkdXEeobR76b53LETtpyT'],
    ['tron', 'TLa2f6VPqDgRE67v1736s7bJ8Ray5wYjU7'],
    ['ton', 'UQBvW8Z5huBkMJYdnfAEM5JqTNkuWX3diqYENkWsIL0XggGG'],
    ['zec', 't1Rv4exT7bqhZqi2j7xz8bUHDMxwosrjADU'],
    ['doge', 'DH5yaieqoZN36fDVciNyRueRGvGLR3mr7L'],
    ['xrp', 'rEb8TK3gBgk5auZkwc6sHnwrGVJH8DuaLh'],
    ['sui', '0x' + 'a'.repeat(64)]
  ];

  it.each(good)('accepts a %s address', (chain, address) => {
    expect(refundAddressError(chain, address)).toBeNull();
  });

  it('refuses an address from the wrong chain', () => {
    expect(refundAddressError('sol', '0x52908400098527886E0F7030069857D2E4169EE7')).toMatch(/Solana/);
    expect(refundAddressError('eth', '9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM')).toMatch(/Ethereum/);
    expect(refundAddressError('tron', 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq')).toMatch(/Tron/);
  });

  it('asks for an address when there is none, and refuses a chain it does not know', () => {
    expect(refundAddressError('eth', '  ')).toMatch(/Enter/);
    expect(refundAddressError('nowhere', 'x')).toMatch(/not accepted/);
  });
});
