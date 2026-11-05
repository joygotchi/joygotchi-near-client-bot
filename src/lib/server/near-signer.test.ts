import { generateKeyPairSync, verify } from 'node:crypto';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  base58Decode,
  base58Encode,
  parseSecretKey,
  receiptFailure,
  serializeSignedTransaction,
  serializeTransaction,
  signTransaction,
  type SendOutcome
} from './near-signer';

/**
 * The signer is hand-written Borsh and hand-written base58, so this is where
 * the wire format is pinned down. A transaction that serialises wrong is
 * rejected by the RPC with "invalid transaction" and nothing says which field
 * was wrong, which is a bad afternoon.
 */

describe('base58', () => {
  // The vector every base58 implementation is checked against.
  it('matches the reference vector', () => {
    expect(base58Encode(new TextEncoder().encode('Hello World!'))).toBe('2NEpo7TZRRrLZSi2U');
    expect(new TextDecoder().decode(base58Decode('2NEpo7TZRRrLZSi2U'))).toBe('Hello World!');
  });

  // Leading zeros fall out of the arithmetic and have to be put back by hand.
  // A NEAR public key beginning with a zero byte is not rare, and dropping it
  // produces a 31-byte key that looks fine until the chain says the access key
  // does not exist.
  it('keeps leading zero bytes', () => {
    expect(base58Encode(new Uint8Array([0, 0, 1]))).toBe('112');
    expect([...base58Decode('112')]).toEqual([0, 0, 1]);
    expect([...base58Decode('1')]).toEqual([0]);
  });

  it('round-trips a 32-byte hash, including one that starts with zero', () => {
    const bytes = new Uint8Array(32).map((_, i) => (i * 7 + 3) % 256);
    bytes[0] = 0;
    expect([...base58Decode(base58Encode(bytes))]).toEqual([...bytes]);
  });

  it('is empty for empty', () => {
    expect(base58Encode(new Uint8Array())).toBe('');
    expect(base58Decode('')).toHaveLength(0);
  });

  it('refuses a character that is not base58', () => {
    // 0, O, I and l are left out of the alphabet precisely so a typo fails
    // here rather than decoding to different bytes.
    expect(() => base58Decode('0OIl')).toThrow('not a base58 character');
  });
});

describe('parseSecretKey', () => {
  const sixtyFour = new Uint8Array(64).map((_, i) => i + 1);

  it('splits NEAR’s 64-byte key into seed and public half', () => {
    const key = parseSecretKey(`ed25519:${base58Encode(sixtyFour)}`);

    expect([...key.seed]).toEqual([...sixtyFour.slice(0, 32)]);
    expect([...key.publicKey]).toEqual([...sixtyFour.slice(32)]);
    expect(key.publicKeyString).toBe(`ed25519:${base58Encode(sixtyFour.slice(32))}`);
  });

  it('tolerates a bare key and surrounding whitespace', () => {
    const bare = parseSecretKey(`  ${base58Encode(sixtyFour)}\n`);
    expect([...bare.seed]).toEqual([...sixtyFour.slice(0, 32)]);
  });

  // The failure this prevents: pasting the *public* key into the env var. It
  // decodes cleanly, so without a length check it would only fail at signing.
  it('refuses a key of the wrong length', () => {
    expect(() => parseSecretKey(`ed25519:${base58Encode(sixtyFour.slice(0, 32))}`)).toThrow(
      'is 64 bytes, got 32'
    );
  });

  it('refuses a curve it cannot sign with', () => {
    expect(() => parseSecretKey('secp256k1:abc')).toThrow('only ed25519');
  });
});

describe('serializeTransaction', () => {
  const key = parseSecretKey(`ed25519:${base58Encode(new Uint8Array(64).map((_, i) => i + 1))}`);
  const blockHash = new Uint8Array(32).fill(9);

  const tx = () =>
    serializeTransaction({
      signerId: 'oracle.testnet',
      publicKey: key.publicKey,
      nonce: 42n,
      receiverId: 'jg-stake.testnet',
      blockHash,
      actions: [
        {
          methodName: 'award_feed_xp',
          args: { pet_id: 3, xp: 63 },
          gas: 100_000_000_000_000n,
          deposit: 0n
        }
      ]
    });

  it('lays the fields out in Borsh order', () => {
    const bytes = tx();
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    let at = 0;

    const string = () => {
      const length = view.getUint32(at, true);
      at += 4;
      const value = new TextDecoder().decode(bytes.subarray(at, at + length));
      at += length;
      return value;
    };

    expect(string()).toBe('oracle.testnet');
    expect(bytes[at]).toBe(0); // ed25519 key type
    at += 1;
    expect([...bytes.subarray(at, at + 32)]).toEqual([...key.publicKey]);
    at += 32;
    expect(view.getBigUint64(at, true)).toBe(42n);
    at += 8;
    expect(string()).toBe('jg-stake.testnet');
    expect([...bytes.subarray(at, at + 32)]).toEqual([...blockHash]);
    at += 32;
    expect(view.getUint32(at, true)).toBe(1); // one action
    at += 4;
    expect(bytes[at]).toBe(2); // FunctionCall is the third Action variant
    at += 1;
    expect(string()).toBe('award_feed_xp');

    const argsLength = view.getUint32(at, true);
    at += 4;
    expect(JSON.parse(new TextDecoder().decode(bytes.subarray(at, at + argsLength)))).toEqual({
      pet_id: 3,
      xp: 63
    });
    at += argsLength;
    expect(view.getBigUint64(at, true)).toBe(100_000_000_000_000n);
    at += 8;
    // A deposit is u128: sixteen little-endian bytes, not eight.
    expect([...bytes.subarray(at, at + 16)]).toEqual(Array(16).fill(0));
    at += 16;
    expect(at).toBe(bytes.length);
  });

  it('writes a u128 deposit across both halves', () => {
    const big = serializeTransaction({
      signerId: 'a.near',
      publicKey: key.publicKey,
      nonce: 1n,
      receiverId: 'b.near',
      blockHash,
      actions: [{ methodName: 'm', args: {}, gas: 1n, deposit: 2n ** 80n + 5n }]
    });

    const deposit = big.subarray(big.length - 16);
    const view = new DataView(deposit.buffer, deposit.byteOffset, 16);
    expect(view.getBigUint64(0, true)).toBe(5n);
    expect(view.getBigUint64(8, true)).toBe(2n ** 16n);
  });

  it('appends the signature after the transaction, with its key type', () => {
    const transaction = tx();
    const signature = new Uint8Array(64).fill(7);
    const signed = serializeSignedTransaction(transaction, signature);

    expect(signed.length).toBe(transaction.length + 1 + 64);
    expect([...signed.subarray(0, transaction.length)]).toEqual([...transaction]);
    expect(signed[transaction.length]).toBe(0);
  });
});

describe('signTransaction', () => {
  // NEAR signs the SHA-256 of the serialized transaction, not the bytes
  // themselves. Signing the wrong thing produces a signature that verifies
  // locally and is rejected on chain, so this checks against node's own
  // verifier rather than against itself.
  it('produces an ed25519 signature over the transaction hash', () => {
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const jwk = privateKey.export({ format: 'jwk' }) as { d: string; x: string };
    const key = {
      seed: new Uint8Array(Buffer.from(jwk.d, 'base64url')),
      publicKey: new Uint8Array(Buffer.from(jwk.x, 'base64url')),
      publicKeyString: 'ed25519:unused'
    };

    const transaction = serializeTransaction({
      signerId: 'oracle.testnet',
      publicKey: key.publicKey,
      nonce: 1n,
      receiverId: 'jg-stake.testnet',
      blockHash: new Uint8Array(32),
      actions: [{ methodName: 'award_feed_xp', args: { pet_id: 1, xp: 1 }, gas: 1n, deposit: 0n }]
    });

    const signature = signTransaction(transaction, key);
    expect(signature).toHaveLength(64);

    const hash = createHash('sha256').update(transaction).digest();
    expect(verify(null, hash, publicKey, signature)).toBe(true);
    // And not over the raw bytes.
    expect(verify(null, transaction, publicKey, signature)).toBe(false);
  });
});

describe('receiptFailure', () => {
  const panic = (message: string) => ({
    Failure: { ActionError: { kind: { FunctionCallError: { ExecutionError: message } } } }
  });

  // `award_feed_xp` returns a promise, so the transaction succeeds the moment
  // its first receipt is accepted and the real `award_xp` call fails a receipt
  // later. Reading only `status` would call every failure a success.
  it('finds a failure in a later receipt under a successful transaction', () => {
    const outcome: SendOutcome = {
      status: { SuccessReceiptId: 'R' },
      receipts_outcome: [
        { outcome: { status: { SuccessValue: '' } } },
        { outcome: { status: panic('Smart contract panicked: No such pet') } }
      ]
    };

    expect(receiptFailure(outcome)).toBe('No such pet');
  });

  it('says nothing when every receipt succeeded', () => {
    expect(
      receiptFailure({
        status: { SuccessValue: '' },
        receipts_outcome: [{ outcome: { status: { SuccessValue: '' } } }]
      })
    ).toBeNull();
  });

  it('still reports a failure it cannot name', () => {
    expect(receiptFailure({ status: { Failure: {} } })).toBe('the call failed');
  });
});
