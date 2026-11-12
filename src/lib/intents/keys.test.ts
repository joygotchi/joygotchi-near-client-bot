import { describe, expect, it } from 'vitest';
import { isQuoteKey, keyType } from './keys';

/*
 * Tokens shaped like the ones the partners portal issues: only the payload
 * matters here, since the server never verifies the signature — 1Click does.
 */
const jwt = (payload: object) =>
  [
    'eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9',
    Buffer.from(JSON.stringify(payload)).toString('base64url'),
    'signature'
  ].join('.');

const DISTRIBUTION = jwt({ v: 1, key_type: 'distribution_channel', partner_id: 'someone' });
const PARTNER = jwt({ v: 1, key_type: 'partner', partner_id: 'someone' });
const EXPLORER = jwt({ v: 1, key_type: 'explorer', partner_id: 'someone' });

describe('keyType', () => {
  it('reads the claim the portal puts in every key', () => {
    expect(keyType(DISTRIBUTION)).toBe('distribution_channel');
    expect(keyType(EXPLORER)).toBe('explorer');
  });

  it('is null for something that is not a JWT', () => {
    expect(keyType('plain-api-key')).toBeNull();
    expect(keyType('a.!!!.c')).toBeNull();
    expect(keyType(jwt({ v: 1 }))).toBeNull();
  });
});

describe('isQuoteKey', () => {
  it('takes the distribution-channel key the portal issues today', () => {
    // The regression: this key was treated as no key at all.
    expect(isQuoteKey(DISTRIBUTION)).toBe(true);
  });

  it('still takes a key issued under the older `partner` name', () => {
    expect(isQuoteKey(PARTNER)).toBe(true);
  });

  it('refuses an explorer key, which authenticates nothing for quotes', () => {
    expect(isQuoteKey(EXPLORER)).toBe(false);
  });

  it('sends a key whose type it cannot read rather than guessing it away', () => {
    expect(isQuoteKey('plain-api-key')).toBe(true);
  });

  it('has nothing to send when nothing is configured', () => {
    expect(isQuoteKey('')).toBe(false);
  });
});
