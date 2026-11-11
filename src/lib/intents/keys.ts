/**
 * Which NEAR Intents keys can authenticate a 1Click quote.
 *
 * The partners portal (https://partners.near-intents.org/home) issues more
 * than one kind of JWT and says which in the token's own `key_type` claim:
 *
 * - `distribution_channel`: what the portal issues to an app that routes
 *   swaps for its users, which is what this app does. 1Click takes it as
 *   `X-API-Key`.
 * - `partner`: the older name for the same role. Still accepted, so a key
 *   issued before the rename keeps working.
 * - `explorer`: reads swap history from explorer.near-intents.org and does
 *   nothing for quoting.
 *
 * Only the first two are sent. An earlier version accepted `partner` alone,
 * which turned the key the portal actually hands out into "no key".
 */
export const QUOTE_KEY_TYPES: readonly string[] = ['distribution_channel', 'partner'];

/** The `key_type` claim of a JWT, or null if it has none or is not a JWT. */
export function keyType(token: string): string | null {
  const payload = token.split('.')[1];
  if (!payload) return null;
  try {
    const padded = payload.padEnd(payload.length + ((4 - (payload.length % 4)) % 4), '=');
    const json = atob(padded.replace(/-/g, '+').replace(/_/g, '/'));
    return (JSON.parse(json) as { key_type?: string }).key_type ?? null;
  } catch {
    return null;
  }
}

/**
 * Can this token authenticate a quote?
 *
 * A token whose type cannot be read is given the benefit of the doubt: 1Click
 * also issues plain API keys, and refusing to send one because it is not a
 * JWT would break a working setup.
 */
export function isQuoteKey(token: string): boolean {
  if (token.length === 0) return false;
  const kind = keyType(token);
  return kind === null || QUOTE_KEY_TYPES.includes(kind);
}
