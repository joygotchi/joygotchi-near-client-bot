import {
  ONECLICK_BASE_URL,
  ONECLICK_FEE_BPS,
  ONECLICK_FEE_RECIPIENT,
  ONECLICK_JWT,
  ONECLICK_REFERRAL
} from '$app/env/private';
import type { ExecutionStatus, QuoteRequest, QuoteResponse, TokenResponse } from '#lib/intents/types';

const API = `${ONECLICK_BASE_URL.replace(/\/+$/, '')}/v0`;

/**
 * NEAR Intents issues more than one kind of JWT, and they are not
 * interchangeable. A partner key authenticates 1Click quotes; an `explorer`
 * key reads swap history from `explorer.near-intents.org` and does nothing
 * here.
 *
 * The claim is in the token's own payload, so checking it costs nothing and
 * stops the UI from reporting quotes as authenticated when they are not —
 * which is what happens if the wrong key is pasted into `ONECLICK_JWT`.
 */
function keyType(token: string): string | null {
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

let warnedAboutKeyType = false;

/**
 * Is a usable 1Click partner key configured?
 *
 * A token of the wrong type counts as no key: quotes still work, they just
 * carry the unauthenticated surcharge, and saying so is better than claiming
 * an authentication that is not happening.
 */
export function hasPartnerKey(): boolean {
  if (ONECLICK_JWT.length === 0) return false;

  const kind = keyType(ONECLICK_JWT);
  if (kind && kind !== 'partner') {
    if (!warnedAboutKeyType) {
      warnedAboutKeyType = true;
      console.warn(
        `[joygotchi] ONECLICK_JWT is a "${kind}" key, not a 1Click partner key. ` +
          'Quotes will carry the 0.25% unauthenticated fee. Get a partner key from ' +
          'https://partners.near-intents.org/home.'
      );
    }
    return false;
  }
  return true;
}

function headers(): HeadersInit {
  const base: Record<string, string> = { 'content-type': 'application/json' };
  // Only send a key 1Click can actually use. Sending an explorer key here
  // authenticates nothing and risks being rejected outright.
  if (hasPartnerKey()) base['X-API-Key'] = ONECLICK_JWT;
  return base;
}

export class OneClickError extends Error {
  constructor(
    readonly status: number,
    message: string
  ) {
    super(message);
    this.name = 'OneClickError';
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API}${path}`, { ...init, headers: headers() });
  const text = await response.text();

  if (!response.ok) {
    let message = text || response.statusText;
    try {
      const parsed = JSON.parse(text) as { message?: string | string[] };
      if (parsed.message) {
        message = Array.isArray(parsed.message) ? parsed.message.join('; ') : parsed.message;
      }
    } catch {
      // keep the raw body
    }
    throw new OneClickError(response.status, message);
  }

  return (text ? JSON.parse(text) : null) as T;
}

/** The token list changes rarely but carries live prices, so cache it briefly. */
let tokenCache: { at: number; tokens: TokenResponse[] } | null = null;
const TOKEN_TTL_MS = 30_000;

export async function getTokens(): Promise<TokenResponse[]> {
  if (tokenCache && Date.now() - tokenCache.at < TOKEN_TTL_MS) return tokenCache.tokens;
  const tokens = await request<TokenResponse[]>('/tokens');
  tokenCache = { at: Date.now(), tokens };
  return tokens;
}

export async function findToken(assetId: string): Promise<TokenResponse | undefined> {
  const tokens = await getTokens();
  return tokens.find((token) => token.assetId === assetId);
}

/** App-level fee, attached to every quote when configured. */
function appFees(): QuoteRequest['appFees'] {
  if (ONECLICK_FEE_BPS <= 0) return undefined;
  if (!ONECLICK_FEE_RECIPIENT) {
    throw new Error('ONECLICK_FEE_BPS is set but ONECLICK_FEE_RECIPIENT is empty');
  }
  return [{ recipient: ONECLICK_FEE_RECIPIENT, fee: ONECLICK_FEE_BPS }];
}

export async function getQuote(
  quote: Omit<QuoteRequest, 'referral' | 'appFees'>
): Promise<QuoteResponse> {
  const body: QuoteRequest = {
    ...quote,
    referral: ONECLICK_REFERRAL,
    appFees: appFees()
  };

  return request<QuoteResponse>('/quote', {
    method: 'POST',
    body: JSON.stringify(body)
  });
}

export async function getExecutionStatus(
  depositAddress: string,
  depositMemo?: string
): Promise<ExecutionStatus> {
  const params = new URLSearchParams({ depositAddress });
  if (depositMemo) params.set('depositMemo', depositMemo);
  return request<ExecutionStatus>(`/status?${params}`);
}

export async function submitDepositTx(input: {
  depositAddress: string;
  txHash: string;
  nearSenderAccount?: string;
  memo?: string;
}): Promise<ExecutionStatus> {
  return request<ExecutionStatus>('/deposit/submit', {
    method: 'POST',
    body: JSON.stringify(input)
  });
}
