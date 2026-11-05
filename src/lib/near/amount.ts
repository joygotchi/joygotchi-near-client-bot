/**
 * Fixed-point helpers for token amounts. Everything is BigInt-based so we never
 * need `near-api-js` (and therefore no Buffer / process polyfills in the bundle).
 */

export const NEAR_DECIMALS = 24;

/** "1.5" + 24 decimals -> "1500000000000000000000000" */
export function parseAmount(value: string, decimals: number): string {
  const trimmed = (value ?? '').trim();
  if (trimmed === '') return '0';
  if (!/^\d*(\.\d*)?$/.test(trimmed)) {
    throw new Error(`"${value}" is not a positive decimal number`);
  }

  const [whole = '0', fraction = ''] = trimmed.split('.');
  const padded = (fraction + '0'.repeat(decimals)).slice(0, decimals);
  const digits = `${whole}${padded}`.replace(/^0+(?=\d)/, '');
  return digits === '' ? '0' : digits;
}

/** "1500000000000000000000000" + 24 decimals -> "1.5" */
export function formatAmount(value: string | bigint, decimals: number, maxFraction = 6): string {
  const raw = typeof value === 'bigint' ? value : BigInt(value || '0');
  const negative = raw < 0n;
  const abs = negative ? -raw : raw;

  const base = 10n ** BigInt(decimals);
  const whole = abs / base;
  const fraction = abs % base;

  let fractionStr = fraction.toString().padStart(decimals, '0').slice(0, maxFraction);
  fractionStr = fractionStr.replace(/0+$/, '');

  const sign = negative ? '-' : '';
  return fractionStr ? `${sign}${whole}.${fractionStr}` : `${sign}${whole}`;
}

export const parseNear = (value: string) => parseAmount(value, NEAR_DECIMALS);
export const formatNear = (value: string | bigint, maxFraction = 4) =>
  formatAmount(value, NEAR_DECIMALS, maxFraction);

/** Shorten `alice-long-name.tg` to `al...me.tg` for the header chip. */
export function truncateAccount(accountId: string): string {
  const [name, ...rest] = accountId.split('.');
  const suffix = rest.length ? `.${rest.join('.')}` : '';
  if (name.length <= 6) return `${name}${suffix}`;
  return `${name.slice(0, 2)}...${name.slice(-2)}${suffix}`;
}
