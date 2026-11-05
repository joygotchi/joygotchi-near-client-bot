import { dev } from '$app/env';
import {
  ALLOW_INSECURE_REPLAY_GUARD,
  KV_REST_API_TOKEN,
  KV_REST_API_URL
} from '$app/env/private';

/**
 * One-shot claims on AI payment receipts.
 *
 * `/api/ai/chat` runs no inference without the hash of a `spend_credits`
 * transaction, and each hash may be used exactly once. "Exactly once" is the
 * whole security property: without it, one payment buys unlimited inference on
 * the server's NEAR AI key.
 *
 * Enforcing it needs an atomic set-if-absent in storage shared by every
 * instance. A `Map` in module scope is neither — serverless instances do not
 * share memory, and two concurrent requests on one instance can both pass a
 * read-then-write.
 */

/** Longer than the RPC keeps a transaction queryable, so nothing expires early. */
const CLAIM_TTL_SECONDS = 24 * 60 * 60;

export type ReceiptStore = {
  /** `true` if this call claimed the hash; `false` if somebody already had it. */
  claim(txHash: string): Promise<boolean>;
  /** Give a claim back, for when the receipt could not be checked at all. */
  release(txHash: string): Promise<void>;
  readonly kind: 'kv' | 'memory';
};

const key = (txHash: string) => `joygotchi:receipt:${txHash}`;

/**
 * Upstash / Vercel KV over REST.
 *
 * `SET … NX EX` is one Redis command, so the claim is atomic across every
 * instance. The REST API answers `{"result":"OK"}` when the key was set and
 * `{"result":null}` when it already existed.
 */
function kvStore(url: string, token: string): ReceiptStore {
  const call = async (command: unknown[]): Promise<unknown> => {
    const response = await fetch(url.replace(/\/+$/, ''), {
      method: 'POST',
      headers: {
        authorization: `Bearer ${token}`,
        'content-type': 'application/json'
      },
      body: JSON.stringify(command)
    });

    if (!response.ok) {
      throw new Error(`KV command failed with HTTP ${response.status}`);
    }
    const body = (await response.json()) as { result?: unknown; error?: string };
    if (body.error) throw new Error(`KV error: ${body.error}`);
    return body.result;
  };

  return {
    kind: 'kv',
    async claim(txHash) {
      const result = await call(['SET', key(txHash), '1', 'NX', 'EX', CLAIM_TTL_SECONDS]);
      return result === 'OK';
    },
    async release(txHash) {
      await call(['DEL', key(txHash)]).catch(() => {
        // A stuck claim only costs the player one retry; losing the release is
        // not worth failing the request over.
      });
    }
  };
}

/**
 * Development-only fallback.
 *
 * Correct within a single process and useless beyond it, which is why
 * production refuses to start on it unless the operator opts in explicitly.
 */
function memoryStore(): ReceiptStore {
  const seen = new Map<string, number>();

  return {
    kind: 'memory',
    async claim(txHash) {
      const now = Date.now();
      for (const [hash, at] of seen) {
        if (now - at > CLAIM_TTL_SECONDS * 1000) seen.delete(hash);
      }
      if (seen.has(txHash)) return false;
      seen.set(txHash, now);
      return true;
    },
    async release(txHash) {
      seen.delete(txHash);
    }
  };
}

export class ReplayGuardUnavailableError extends Error {
  constructor() {
    super(
      'The AI payment replay guard is not configured. Set KV_REST_API_URL and ' +
        'KV_REST_API_TOKEN, or set ALLOW_INSECURE_REPLAY_GUARD=true to accept ' +
        'that one payment can buy more than one inference.'
    );
    this.name = 'ReplayGuardUnavailableError';
  }
}

let store: ReceiptStore | null = null;

/**
 * The store to use, decided once.
 *
 * Outside development this throws rather than quietly degrading: a bypassable
 * guard that looks like a working one is worse than an endpoint that refuses
 * to run, because only one of the two tells you about it.
 */
export function receiptStore(): ReceiptStore {
  if (store) return store;

  if (KV_REST_API_URL && KV_REST_API_TOKEN) {
    store = kvStore(KV_REST_API_URL, KV_REST_API_TOKEN);
    return store;
  }

  if (!dev && ALLOW_INSECURE_REPLAY_GUARD !== 'true') {
    throw new ReplayGuardUnavailableError();
  }

  if (!dev) {
    console.warn(
      '[joygotchi] ALLOW_INSECURE_REPLAY_GUARD is set: AI payment receipts are ' +
        'only deduplicated per instance, so one payment can buy more than one ' +
        'inference. Configure KV_REST_API_URL and KV_REST_API_TOKEN.'
    );
  }

  store = memoryStore();
  return store;
}
