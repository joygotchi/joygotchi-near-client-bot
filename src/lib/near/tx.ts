import { GAS, ONE_YOCTO } from '#lib/config';
import { decodeResult } from './rpc';
import { getActiveWallet, wallet } from './wallet.svelte';

/**
 * The part of a NEAR `FinalExecutionOutcome` we actually read. The wallet
 * returns the full structure; typing just this keeps `@near-js/*` out of the
 * dependency tree.
 */
export type ExecutionStatus = {
  SuccessValue?: string;
  SuccessReceiptId?: string;
  Failure?: {
    ActionError?: {
      kind?: { FunctionCallError?: { ExecutionError?: string } } | Record<string, unknown>;
    };
  };
};

export type TxOutcome = {
  transaction: { hash: string; signer_id: string; receiver_id: string };
  status: ExecutionStatus & Record<string, unknown>;
  receipts_outcome?: Array<{ outcome: { logs: string[]; status?: ExecutionStatus } }>;
};

export class WalletNotConnectedError extends Error {
  constructor() {
    super('Connect a wallet first');
    this.name = 'WalletNotConnectedError';
  }
}

/** A call that the chain accepted, but whose work failed further down. */
export class ReceiptFailedError extends Error {
  constructor(
    message: string,
    readonly txHash: string
  ) {
    super(message);
    this.name = 'ReceiptFailedError';
  }
}

function panicMessage(status: ExecutionStatus | undefined): string | null {
  const kind = status?.Failure?.ActionError?.kind;
  if (!kind) return null;
  const execution = (kind as { FunctionCallError?: { ExecutionError?: string } }).FunctionCallError
    ?.ExecutionError;
  if (typeof execution === 'string') {
    // "Smart contract panicked: <what the contract actually said>"
    return execution.replace(/^Smart contract panicked:\s*/, '');
  }
  return Object.keys(kind)[0] ?? 'the call failed';
}

/**
 * Throw if any receipt in the transaction failed.
 *
 * A NEAR transaction reports success as soon as its first receipt is accepted,
 * so everything a contract does in a cross-contract callback fails *silently*
 * from the client's point of view. On testnet, staking somebody else's pet came
 * back as a clean success: `stake_pet` only schedules the call, and the
 * ownership check lives in the callback. The UI said "staked".
 *
 * Checked everywhere rather than at call sites, because the calls that cross
 * contracts are not the ones that look like they do.
 */
export function assertReceiptsSucceeded(outcome: TxOutcome): TxOutcome {
  const statuses = [outcome.status, ...(outcome.receipts_outcome ?? []).map((r) => r.outcome.status)];
  for (const status of statuses) {
    const message = panicMessage(status);
    if (message) throw new ReceiptFailedError(message, outcome.transaction?.hash ?? '');
  }
  return outcome;
}

export type FunctionCall = {
  methodName: string;
  args?: Record<string, unknown>;
  /** Gas units. Defaults to `GAS.call`. */
  gas?: string;
  /** Deposit in yoctoNEAR. Defaults to `"0"`. */
  deposit?: string;
};

function toAction(call: FunctionCall) {
  return {
    type: 'FunctionCall' as const,
    params: {
      methodName: call.methodName,
      args: call.args ?? {},
      gas: call.gas ?? GAS.call,
      deposit: call.deposit ?? '0'
    }
  };
}

/** Sign and send a single function call. */
export async function callFunction(
  contractId: string,
  methodName: string,
  args: Record<string, unknown> = {},
  options: { gas?: string; deposit?: string } = {}
): Promise<TxOutcome> {
  const active = await getActiveWallet();
  if (!active || !wallet.accountId) throw new WalletNotConnectedError();

  const outcome = await active.signAndSendTransaction({
    signerId: wallet.accountId,
    receiverId: contractId,
    actions: [toAction({ methodName, args, ...options })]
  });
  return assertReceiptsSucceeded(outcome);
}

/** Sign and send several calls to the same contract as one transaction. */
export async function callFunctions(
  contractId: string,
  calls: FunctionCall[]
): Promise<TxOutcome> {
  const active = await getActiveWallet();
  if (!active || !wallet.accountId) throw new WalletNotConnectedError();

  const outcome = await active.signAndSendTransaction({
    signerId: wallet.accountId,
    receiverId: contractId,
    actions: calls.map(toAction)
  });
  return assertReceiptsSucceeded(outcome);
}

/** Sign and send a call, then decode its return value. */
export async function callAndDecode<T>(
  contractId: string,
  methodName: string,
  args: Record<string, unknown> = {},
  options: { gas?: string; deposit?: string } = {}
): Promise<T | null> {
  const outcome = await callFunction(contractId, methodName, args, options);
  return decodeResult<T>(outcome);
}

/**
 * Read the log line a callback left when it reported failure by *returning*
 * rather than panicking.
 *
 * `on_pet_attrs` logs why and returns `false`, which keeps the pet's state
 * clean but is indistinguishable from success in the transaction status.
 */
export function failureLog(outcome: TxOutcome, patterns: string[]): string | null {
  for (const receipt of outcome.receipts_outcome ?? []) {
    for (const line of receipt.outcome.logs) {
      if (patterns.some((p) => line.includes(p))) return line;
    }
  }
  return null;
}

/**
 * NEP-141 `ft_transfer_call`: move tokens to a contract and let it react.
 * Used for buying items with $JOY and for paying a pet mint.
 */
export async function ftTransferCall(
  tokenContract: string,
  receiverId: string,
  amount: string,
  msg: string
): Promise<TxOutcome> {
  return callFunction(
    tokenContract,
    'ft_transfer_call',
    { receiver_id: receiverId, amount, msg },
    { gas: GAS.transferCall, deposit: ONE_YOCTO }
  );
}

/**
 * Sign several transactions at once, each to a different contract.
 *
 * The wallet shows them together and they are signed in one approval, but they
 * are separate transactions: a later one can land even if an earlier one
 * fails, so only use this where that is acceptable.
 */
export async function callAcrossContracts(
  transactions: Array<{ receiverId: string; calls: FunctionCall[] }>
): Promise<TxOutcome[]> {
  const active = await getActiveWallet();
  if (!active || !wallet.accountId) throw new WalletNotConnectedError();

  const outcomes = await active.signAndSendTransactions({
    transactions: transactions.map((tx) => ({
      receiverId: tx.receiverId,
      actions: tx.calls.map(toAction)
    }))
  });
  outcomes.forEach(assertReceiptsSucceeded);
  return outcomes;
}

/** NEP-141 `storage_deposit`, needed once before an account can hold a token. */
export async function storageDeposit(
  tokenContract: string,
  accountId: string,
  depositYocto: string
): Promise<TxOutcome> {
  return callFunction(
    tokenContract,
    'storage_deposit',
    { account_id: accountId, registration_only: true },
    { deposit: depositYocto }
  );
}
