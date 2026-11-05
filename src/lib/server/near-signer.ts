/**
 * Signs and sends a NEAR function call from a server key.
 *
 * Everything the browser signs goes through the wallet (`#lib/near/tx`). This
 * is the other half: two things have to happen *for* a player, with no wallet
 * in the loop.
 *
 *  - Feed XP. `game::award_xp` accepts only the staking contract or the game
 *    owner, and the taste multiplier is only known after NEAR AI has judged
 *    the meal — so the backend has to call `pet-stake::award_feed_xp` itself.
 *  - Auto-feed (issue #8), which needs the same shape: a key the server holds,
 *    scoped to one method on one contract.
 *
 * Written by hand rather than with `near-api-js` on purpose. The dependency is
 * ~1 MB of browser-oriented code for a transaction this file describes in 60
 * lines, it drags in its own crypto, and this runs in a serverless function
 * where cold-start size is the thing you pay for. Borsh is a fixed wire format;
 * the pieces below are unit-tested against known vectors in `near-signer.test.ts`.
 *
 * The key this is used with must be a *function-call* access key, limited to
 * the one method it needs. Nothing here checks that — the chain does, and that
 * is the point: if the key leaks, the chain refuses everything else.
 */

import { createPrivateKey, createHash, sign as nodeSign } from 'node:crypto';

const BASE58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

export function base58Decode(input: string): Uint8Array {
  if (input.length === 0) return new Uint8Array();

  const bytes: number[] = [];
  for (const char of input) {
    let carry = BASE58.indexOf(char);
    if (carry < 0) throw new Error(`"${char}" is not a base58 character`);

    for (let i = 0; i < bytes.length; i++) {
      carry += bytes[i] * 58;
      bytes[i] = carry & 0xff;
      carry >>= 8;
    }
    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }

  // Each leading "1" is a leading zero byte, which the maths above drops.
  for (const char of input) {
    if (char !== '1') break;
    bytes.push(0);
  }

  return new Uint8Array(bytes.reverse());
}

export function base58Encode(input: Uint8Array): string {
  if (input.length === 0) return '';

  const digits: number[] = [];
  for (const byte of input) {
    let carry = byte;
    for (let i = 0; i < digits.length; i++) {
      carry += digits[i] << 8;
      digits[i] = carry % 58;
      carry = (carry / 58) | 0;
    }
    while (carry > 0) {
      digits.push(carry % 58);
      carry = (carry / 58) | 0;
    }
  }

  let leading = '';
  for (const byte of input) {
    if (byte !== 0) break;
    leading += '1';
  }

  return leading + digits.reverse().map((d) => BASE58[d]).join('');
}

export type KeyPair = {
  /** The 32-byte ed25519 seed. */
  seed: Uint8Array;
  publicKey: Uint8Array;
  /** `ed25519:…`, the form the RPC and the chain use. */
  publicKeyString: string;
};

/**
 * Read a NEAR secret key.
 *
 * NEAR writes ed25519 secret keys as 64 bytes: the 32-byte seed followed by
 * the public key. Node's crypto wants them apart, and the public half is also
 * what identifies the access key on chain, so both are returned here rather
 * than derived twice.
 */
export function parseSecretKey(raw: string): KeyPair {
  const trimmed = raw.trim();
  const [prefix, encoded] = trimmed.includes(':') ? trimmed.split(':') : ['ed25519', trimmed];
  if (prefix !== 'ed25519') {
    throw new Error(`only ed25519 keys are supported, got "${prefix}"`);
  }

  const bytes = base58Decode(encoded);
  if (bytes.length !== 64) {
    throw new Error(`an ed25519 secret key is 64 bytes, got ${bytes.length}`);
  }

  const publicKey = bytes.slice(32);
  return {
    seed: bytes.slice(0, 32),
    publicKey,
    publicKeyString: `ed25519:${base58Encode(publicKey)}`
  };
}

// ── Borsh ────────────────────────────────────────────────────────────────────

class Writer {
  private parts: Uint8Array[] = [];

  u8(value: number) {
    this.parts.push(new Uint8Array([value]));
    return this;
  }

  u32(value: number) {
    const buf = new Uint8Array(4);
    new DataView(buf.buffer).setUint32(0, value, true);
    this.parts.push(buf);
    return this;
  }

  u64(value: bigint) {
    const buf = new Uint8Array(8);
    new DataView(buf.buffer).setBigUint64(0, value, true);
    this.parts.push(buf);
    return this;
  }

  /** Little-endian, 16 bytes — NEAR deposits are u128. */
  u128(value: bigint) {
    const buf = new Uint8Array(16);
    const view = new DataView(buf.buffer);
    view.setBigUint64(0, value & 0xffffffffffffffffn, true);
    view.setBigUint64(8, value >> 64n, true);
    this.parts.push(buf);
    return this;
  }

  /** Borsh string: a u32 byte length, then UTF-8. */
  string(value: string) {
    const bytes = new TextEncoder().encode(value);
    return this.u32(bytes.length).raw(bytes);
  }

  raw(bytes: Uint8Array) {
    this.parts.push(bytes);
    return this;
  }

  finish(): Uint8Array {
    const total = this.parts.reduce((n, part) => n + part.length, 0);
    const out = new Uint8Array(total);
    let offset = 0;
    for (const part of this.parts) {
      out.set(part, offset);
      offset += part.length;
    }
    return out;
  }
}

export type FunctionCallAction = {
  methodName: string;
  args: Record<string, unknown>;
  /** Gas units. */
  gas: bigint;
  /** yoctoNEAR. */
  deposit: bigint;
};

export type TransactionFields = {
  signerId: string;
  publicKey: Uint8Array;
  nonce: bigint;
  receiverId: string;
  /** The 32-byte block hash, already decoded. */
  blockHash: Uint8Array;
  actions: FunctionCallAction[];
};

/** Enum discriminants in NEAR's `Action`. FunctionCall is the third. */
const ACTION_FUNCTION_CALL = 2;
/** Enum discriminant in `PublicKey` / `Signature`. */
const KEY_TYPE_ED25519 = 0;

export function serializeTransaction(tx: TransactionFields): Uint8Array {
  const w = new Writer();
  w.string(tx.signerId);
  w.u8(KEY_TYPE_ED25519).raw(tx.publicKey);
  w.u64(tx.nonce);
  w.string(tx.receiverId);
  w.raw(tx.blockHash);

  w.u32(tx.actions.length);
  for (const action of tx.actions) {
    const args = new TextEncoder().encode(JSON.stringify(action.args));
    w.u8(ACTION_FUNCTION_CALL)
      .string(action.methodName)
      .u32(args.length)
      .raw(args)
      .u64(action.gas)
      .u128(action.deposit);
  }

  return w.finish();
}

/** A `SignedTransaction` is the transaction followed by the signature. */
export function serializeSignedTransaction(
  transaction: Uint8Array,
  signature: Uint8Array
): Uint8Array {
  return new Writer()
    .raw(transaction)
    .u8(KEY_TYPE_ED25519)
    .raw(signature)
    .finish();
}

const base64url = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64url');

/**
 * Sign the SHA-256 of a serialized transaction, which is what NEAR signs.
 *
 * Node takes an ed25519 key as a JWK, so the seed and public half go in
 * directly — no DER to assemble.
 */
export function signTransaction(transaction: Uint8Array, key: KeyPair): Uint8Array {
  const hash = createHash('sha256').update(transaction).digest();
  const privateKey = createPrivateKey({
    format: 'jwk',
    key: {
      kty: 'OKP',
      crv: 'Ed25519',
      d: base64url(key.seed),
      x: base64url(key.publicKey)
    }
  });
  return new Uint8Array(nodeSign(null, hash, privateKey));
}

// ── RPC ──────────────────────────────────────────────────────────────────────

type ExecutionStatus = {
  SuccessValue?: string;
  SuccessReceiptId?: string;
  Failure?: {
    ActionError?: { kind?: { FunctionCallError?: { ExecutionError?: string } } };
  };
};

export type SendOutcome = {
  transaction?: { hash?: string };
  status?: ExecutionStatus;
  receipts_outcome?: Array<{ outcome: { status?: ExecutionStatus } }>;
};

/**
 * The message of the first receipt that failed, or null.
 *
 * Needed for the same reason `assertReceiptsSucceeded` is needed on the
 * client: `award_feed_xp` returns a promise, so the transaction reports success
 * the moment its first receipt is accepted and the actual `award_xp` call
 * fails one receipt later.
 */
export function receiptFailure(outcome: SendOutcome): string | null {
  const statuses = [outcome.status, ...(outcome.receipts_outcome ?? []).map((r) => r.outcome.status)];
  for (const status of statuses) {
    const error = status?.Failure?.ActionError?.kind?.FunctionCallError?.ExecutionError;
    if (typeof error === 'string') {
      return error.replace(/^Smart contract panicked:\s*/, '');
    }
    if (status?.Failure) return 'the call failed';
  }
  return null;
}

async function rpc<T>(url: string, method: string, params: unknown): Promise<T> {
  const response = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 'signer', method, params })
  });

  const body = (await response.json()) as {
    result?: T;
    error?: { message?: string; data?: unknown; cause?: { name?: string } };
  };

  if (body.error || body.result === undefined) {
    // `cause.name` is the coarse label ("INVALID_TRANSACTION") and `data`
    // carries what was actually wrong — a key not permitted for this method,
    // a stale nonce, an exhausted allowance. Reporting only the label turns
    // every one of those into the same unhelpful line, and this message is
    // what ends up in the response's `xpError`.
    const data = body.error?.data;
    const detail = typeof data === 'string' ? data : data ? JSON.stringify(data) : null;
    const label = body.error?.cause?.name ?? body.error?.message ?? 'unknown RPC error';
    throw new Error(`${method} failed: ${detail ? `${label}: ${detail}` : label}`);
  }

  return body.result;
}

/**
 * Highest nonce this instance has used, per public key.
 *
 * The nonce has to be strictly greater than the access key's stored one, and
 * two requests that read it in the same block would otherwise pick the same
 * number and one would be rejected. This does not help across instances, which
 * is what `retries` below is for.
 */
const lastUsedNonce = new Map<string, bigint>();

export type SignerConfig = {
  rpcUrl: string;
  accountId: string;
  /** `ed25519:…`, the 64-byte secret key form. */
  secretKey: string;
};

/**
 * Sign one function call with a server key and wait for it to execute.
 *
 * Throws on anything that went wrong, including a receipt that failed under a
 * transaction the chain called a success — callers decide whether that is
 * fatal.
 */
export async function signAndSendFunctionCall(
  config: SignerConfig,
  receiverId: string,
  action: FunctionCallAction,
  retries = 1
): Promise<{ txHash: string }> {
  const key = parseSecretKey(config.secretKey);

  // One request, two answers: the key's nonce and a recent block hash. The
  // block hash only has to be from the last ~24 hours, and the one the access
  // key was read at always is.
  const accessKey = await rpc<{ nonce: number; block_hash: string }>(config.rpcUrl, 'query', {
    request_type: 'view_access_key',
    finality: 'final',
    account_id: config.accountId,
    public_key: key.publicKeyString
  });

  const onChain = BigInt(accessKey.nonce);
  const previous = lastUsedNonce.get(key.publicKeyString) ?? 0n;
  const nonce = (onChain > previous ? onChain : previous) + 1n;
  lastUsedNonce.set(key.publicKeyString, nonce);

  const transaction = serializeTransaction({
    signerId: config.accountId,
    publicKey: key.publicKey,
    nonce,
    receiverId,
    blockHash: base58Decode(accessKey.block_hash),
    actions: [action]
  });

  const signed = serializeSignedTransaction(transaction, signTransaction(transaction, key));

  let outcome: SendOutcome;
  try {
    outcome = await rpc<SendOutcome>(config.rpcUrl, 'send_tx', {
      signed_tx_base64: Buffer.from(signed).toString('base64'),
      wait_until: 'EXECUTED_OPTIMISTIC'
    });
  } catch (err) {
    // Another instance used this nonce first. Re-reading it is the fix, and it
    // is worth one retry: the player has already paid for this feed.
    const message = err instanceof Error ? err.message : String(err);
    if (retries > 0 && /nonce/i.test(message)) {
      lastUsedNonce.set(key.publicKeyString, nonce);
      return signAndSendFunctionCall(config, receiverId, action, retries - 1);
    }
    throw err;
  }

  const failure = receiptFailure(outcome);
  if (failure) throw new Error(failure);

  return { txHash: outcome.transaction?.hash ?? '' };
}
