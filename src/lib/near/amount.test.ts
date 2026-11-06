import { describe, expect, it } from 'vitest';
import {
  formatAmount,
  formatNear,
  parseAmount,
  parseNear,
  truncateAccount
} from '#lib/near/amount';

describe('parseAmount', () => {
  it('scales a decimal string to base units', () => {
    expect(parseAmount('1.5', 24)).toBe('1500000000000000000000000');
    expect(parseAmount('2', 24)).toBe('2000000000000000000000000');
    expect(parseAmount('0.000001', 6)).toBe('1');
  });

  it('truncates rather than rounds past the last decimal', () => {
    // Rounding up here would mean charging a player more than they typed.
    expect(parseAmount('0.0000005', 6)).toBe('0');
    expect(parseAmount('1.9999999', 6)).toBe('1999999');
  });

  it('treats empty and bare separators as zero', () => {
    expect(parseAmount('', 24)).toBe('0');
    expect(parseAmount('0', 24)).toBe('0');
    expect(parseAmount('.5', 6)).toBe('500000');
  });

  it('refuses anything that is not a positive decimal', () => {
    expect(() => parseAmount('-1', 24)).toThrow();
    expect(() => parseAmount('1e24', 24)).toThrow();
    expect(() => parseAmount('abc', 24)).toThrow();
  });

  it('keeps full precision on amounts no float could hold', () => {
    // 2^53 is where Number starts losing integers; NEAR amounts are far past it.
    expect(parseAmount('9007199254740993.000001', 6)).toBe('9007199254740993000001');
  });
});

describe('formatAmount', () => {
  it('is the inverse of parseAmount', () => {
    // All already in canonical form: no trailing fractional zeros to lose.
    for (const value of ['1.5', '0.001', '123.456789', '1000000']) {
      expect(formatAmount(parseAmount(value, 24), 24, 6)).toBe(value);
    }
  });

  it('drops trailing zeros and the point with them', () => {
    expect(formatAmount('1500000000000000000000000', 24)).toBe('1.5');
    expect(formatAmount('2000000000000000000000000', 24)).toBe('2');
  });

  it('honours the fraction limit', () => {
    expect(formatAmount('1234567', 6, 2)).toBe('1.23');
    expect(formatAmount('1234567', 6, 6)).toBe('1.234567');
  });

  it('handles zero and accepts a bigint', () => {
    expect(formatAmount('0', 24)).toBe('0');
    expect(formatAmount(1500000000000000000000000n, 24)).toBe('1.5');
  });
});

describe('near helpers', () => {
  it('round-trip through 24 decimals', () => {
    expect(formatNear(parseNear('2.5'))).toBe('2.5');
  });
});

describe('truncateAccount', () => {
  it('keeps short names whole', () => {
    expect(truncateAccount('alice.tg')).toBe('alice.tg');
  });

  it('shortens the middle of a long name and keeps the suffix', () => {
    expect(truncateAccount('huunhanz-hot.tg')).toBe('hu...ot.tg');
  });

  it('handles an implicit account, which has no suffix at all', () => {
    const implicit = 'a'.repeat(64);
    expect(truncateAccount(implicit)).toBe('aa...aa');
  });
});
