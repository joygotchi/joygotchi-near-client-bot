import type { PurchaseQuoteRequest, PurchaseQuoteResponse } from './purchase';
import { TERMINAL_STATUSES, type ExecutionStatus, type TokenResponse } from './types';

/**
 * Our handlers report problems two ways: `json({ error })` for an upstream
 * failure we want to pass through, and SvelteKit's `error()` — which serialises
 * to `{ message }` — for a bad request. Read both so the player always sees the
 * real reason.
 */
async function readJson<T>(response: Response): Promise<T> {
  const body = (await response.json().catch(() => null)) as
    | (T & { error?: string; message?: string })
    | null;

  if (!response.ok) {
    throw new Error(
      body?.error ?? body?.message ?? `Request failed with HTTP ${response.status}`
    );
  }
  return body as T;
}

export type PayToken = TokenResponse;

export async function fetchPayTokens(fetchImpl: typeof fetch = fetch): Promise<PayToken[]> {
  const response = await fetchImpl('/api/intents/tokens');
  const body = await readJson<{ tokens: PayToken[] }>(response);
  return body.tokens;
}

export type QuoteResult = PurchaseQuoteResponse & { originPriceUsd: number | null };

export async function requestQuote(
  input: PurchaseQuoteRequest,
  fetchImpl: typeof fetch = fetch
): Promise<QuoteResult> {
  const response = await fetchImpl('/api/intents/quote', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input)
  });
  return readJson<QuoteResult>(response);
}

export async function fetchStatus(
  depositAddress: string,
  depositMemo?: string,
  fetchImpl: typeof fetch = fetch
): Promise<ExecutionStatus> {
  const params = new URLSearchParams({ depositAddress });
  if (depositMemo) params.set('depositMemo', depositMemo);
  return readJson<ExecutionStatus>(await fetchImpl(`/api/intents/status?${params}`));
}

export async function submitDeposit(
  input: { depositAddress: string; txHash: string; nearSenderAccount?: string; memo?: string },
  fetchImpl: typeof fetch = fetch
): Promise<ExecutionStatus> {
  const response = await fetchImpl('/api/intents/deposit', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input)
  });
  return readJson<ExecutionStatus>(response);
}

/**
 * Poll a swap until it reaches a terminal state. Returns a function that stops
 * the polling — call it from `onDestroy`.
 */
export function watchSwap(
  depositAddress: string,
  depositMemo: string | undefined,
  handlers: {
    onUpdate: (status: ExecutionStatus) => void;
    onError?: (error: Error) => void;
    intervalMs?: number;
  }
): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const interval = handlers.intervalMs ?? 4000;

  const tick = async () => {
    if (stopped) return;
    try {
      const status = await fetchStatus(depositAddress, depositMemo);
      if (stopped) return;
      handlers.onUpdate(status);
      if (TERMINAL_STATUSES.includes(status.status)) return;
    } catch (error) {
      handlers.onError?.(error as Error);
    }
    timer = setTimeout(tick, interval);
  };

  void tick();

  return () => {
    stopped = true;
    if (timer) clearTimeout(timer);
  };
}
