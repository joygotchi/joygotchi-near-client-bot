import { RPC_URL } from '#lib/config';

type RpcError = { code?: number; message?: string; data?: unknown; name?: string; cause?: unknown };

function toBase64(json: unknown): string {
  const text = JSON.stringify(json ?? {});
  const bytes = new TextEncoder().encode(text);
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

async function rpc<T>(method: string, params: unknown, fetchImpl: typeof fetch = fetch): Promise<T> {
  const response = await fetchImpl(RPC_URL, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 'joygotchi', method, params })
  });

  if (!response.ok) {
    throw new Error(`NEAR RPC ${method} failed: HTTP ${response.status}`);
  }

  const body = (await response.json()) as { result?: T; error?: RpcError };
  if (body.error) {
    const detail = body.error.cause ?? body.error.data ?? body.error.message;
    throw new Error(`NEAR RPC ${method} failed: ${JSON.stringify(detail)}`);
  }
  return body.result as T;
}

/**
 * Read-only contract call. Works on the server and in the browser — it never
 * touches the wallet, so pages can render pet data during SSR.
 */
export async function viewFunction<T = unknown>(
  contractId: string,
  methodName: string,
  args: Record<string, unknown> = {},
  fetchImpl: typeof fetch = fetch
): Promise<T> {
  const result = await rpc<{ result: number[]; logs: string[] }>(
    'query',
    {
      request_type: 'call_function',
      finality: 'optimistic',
      account_id: contractId,
      method_name: methodName,
      args_base64: toBase64(args)
    },
    fetchImpl
  );

  const text = new TextDecoder().decode(new Uint8Array(result.result));
  return (text === '' ? null : JSON.parse(text)) as T;
}

/** Native NEAR balance in yoctoNEAR. Returns "0" for accounts that don't exist. */
export async function getAccountBalance(
  accountId: string,
  fetchImpl: typeof fetch = fetch
): Promise<string> {
  try {
    const account = await rpc<{ amount: string }>(
      'query',
      { request_type: 'view_account', finality: 'optimistic', account_id: accountId },
      fetchImpl
    );
    return account.amount;
  } catch {
    return '0';
  }
}

/**
 * Does this account exist on-chain?
 *
 * Worth checking before asking 1Click for a quote: when the NEAR recipient does
 * not exist, 1Click answers `400 Internal server error`, which tells nobody
 * anything. Results are cached because contract ids do not come and go.
 */
const existsCache = new Map<string, boolean>();

export async function accountExists(
  accountId: string,
  fetchImpl: typeof fetch = fetch
): Promise<boolean> {
  const cached = existsCache.get(accountId);
  if (cached !== undefined) return cached;

  try {
    await rpc<{ amount: string }>(
      'query',
      { request_type: 'view_account', finality: 'optimistic', account_id: accountId },
      fetchImpl
    );
    existsCache.set(accountId, true);
    return true;
  } catch (error) {
    // Only cache a definite "no". A network blip must not poison the cache.
    const message = error instanceof Error ? error.message : '';
    if (message.includes('UNKNOWN_ACCOUNT') || message.includes('does not exist')) {
      existsCache.set(accountId, false);
      return false;
    }
    return true;
  }
}

/** NEP-141 balance in the token's smallest unit. */
export async function ftBalanceOf(
  tokenContract: string,
  accountId: string,
  fetchImpl: typeof fetch = fetch
): Promise<string> {
  try {
    const balance = await viewFunction<string>(
      tokenContract,
      'ft_balance_of',
      { account_id: accountId },
      fetchImpl
    );
    return balance ?? '0';
  } catch {
    return '0';
  }
}

/** Decode the `SuccessValue` of a FunctionCall outcome into JSON. */
export function decodeResult<T = unknown>(outcome: unknown): T | null {
  const status = (outcome as { status?: { SuccessValue?: string } } | undefined)?.status;
  if (!status || typeof status.SuccessValue !== 'string') return null;
  if (status.SuccessValue === '') return null;

  const binary = atob(status.SuccessValue);
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  const text = new TextDecoder().decode(bytes);
  try {
    return JSON.parse(text) as T;
  } catch {
    return text as unknown as T;
  }
}
