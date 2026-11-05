import { RPC_URL, STAKE_CONTRACT } from '#lib/config';

/**
 * Verifies on-chain that a player really paid for an AI action, so the NEAR AI
 * key on this server can never be used for free.
 *
 * The browser first sends `spend_credits` to the staking contract, then hands us
 * the transaction hash. We look the transaction up on the RPC and check that it
 *  - succeeded,
 *  - was signed by the account that is asking,
 *  - called `spend_credits` on our staking contract,
 *  - carried the action we are about to perform,
 *  - has not been presented before.
 */

export type SpendReceipt = {
  accountId: string;
  action: string;
  petId: number | null;
  amount: string;
  /**
   * Whether a Focus Lens was consumed by this spend. Read from the contract's
   * own event log, never from the client — otherwise anyone could claim the
   * bonus by sending `lensActive: true`.
   */
  lensApplied: boolean;
};

/**
 * Replay guard. In-memory is enough for a single serverless instance and for
 * local development; point this at Redis / Vercel KV before going to
 * production, or an attacker with two concurrent requests can reuse one receipt.
 */
const seen = new Map<string, number>();
const SEEN_TTL_MS = 30 * 60_000;

function remember(txHash: string): boolean {
  const now = Date.now();
  for (const [hash, at] of seen) if (now - at > SEEN_TTL_MS) seen.delete(hash);
  if (seen.has(txHash)) return false;
  seen.set(txHash, now);
  return true;
}

type TxStatus = {
  transaction: { signer_id: string; receiver_id: string; actions: unknown[] };
  status: Record<string, unknown>;
  receipts_outcome: Array<{ outcome: { logs: string[]; status: Record<string, unknown> } }>;
};

type CreditSpentEvent = {
  event?: string;
  data?: Array<Record<string, unknown>>;
};

/** Pull the contract's `credit_spent` event out of a transaction's logs. */
function findCreditSpent(tx: TxStatus): Record<string, unknown> | null {
  for (const receipt of tx.receipts_outcome ?? []) {
    for (const log of receipt.outcome?.logs ?? []) {
      if (!log.startsWith('EVENT_JSON:')) continue;
      try {
        const event = JSON.parse(log.slice('EVENT_JSON:'.length)) as CreditSpentEvent;
        if (event.event === 'credit_spent' && event.data?.[0]) {
          return event.data[0];
        }
      } catch {
        // Not our event; keep looking.
      }
    }
  }
  return null;
}

function decodeArgs(argsBase64: string): Record<string, unknown> {
  const binary = atob(argsBase64);
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  try {
    return JSON.parse(new TextDecoder().decode(bytes)) as Record<string, unknown>;
  } catch {
    return {};
  }
}

export class ReceiptError extends Error {}

export async function verifySpendReceipt(input: {
  txHash: string;
  accountId: string;
  action: string;
}): Promise<SpendReceipt> {
  const { txHash, accountId, action } = input;

  if (!remember(txHash)) {
    throw new ReceiptError('This payment receipt was already used');
  }

  const response = await fetch(RPC_URL, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: 'verify',
      method: 'tx',
      params: { tx_hash: txHash, sender_account_id: accountId, wait_until: 'EXECUTED_OPTIMISTIC' }
    })
  });

  const body = (await response.json()) as { result?: TxStatus; error?: { message?: string } };
  if (body.error || !body.result) {
    seen.delete(txHash);
    throw new ReceiptError(`Could not find transaction ${txHash}`);
  }

  const tx = body.result;

  if (tx.transaction.signer_id !== accountId) {
    throw new ReceiptError('Transaction was signed by a different account');
  }
  if (tx.transaction.receiver_id !== STAKE_CONTRACT) {
    throw new ReceiptError('Transaction did not go to the staking contract');
  }
  if (!('SuccessValue' in tx.status) && !('SuccessReceiptId' in tx.status)) {
    throw new ReceiptError('Transaction did not succeed');
  }

  const call = tx.transaction.actions.find(
    (entry): entry is { FunctionCall: { method_name: string; args: string } } =>
      typeof entry === 'object' && entry !== null && 'FunctionCall' in entry
  );

  if (!call || call.FunctionCall.method_name !== 'spend_credits') {
    throw new ReceiptError('Transaction is not a spend_credits call');
  }

  const args = decodeArgs(call.FunctionCall.args);
  if (args.action !== action) {
    throw new ReceiptError(`Receipt paid for "${String(args.action)}", not "${action}"`);
  }

  // The event is what the contract actually did, as opposed to what the call
  // asked for, so bonuses are read from there.
  const event = findCreditSpent(tx);

  return {
    accountId,
    action,
    petId: typeof args.pet_id === 'number' ? args.pet_id : null,
    amount: typeof event?.amount === 'string' ? event.amount : '0',
    lensApplied: event?.lens_applied === true
  };
}
