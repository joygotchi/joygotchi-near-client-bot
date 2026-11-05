import { GAS, ONE_YOCTO } from '#lib/config';
import { decodeResult } from './rpc';
import { getActiveWallet, wallet } from './wallet.svelte';

/**
 * The part of a NEAR `FinalExecutionOutcome` we actually read. The wallet
 * returns the full structure; typing just this keeps `@near-js/*` out of the
 * dependency tree.
 */
export type TxOutcome = {
  transaction: { hash: string; signer_id: string; receiver_id: string };
  status: Record<string, unknown>;
  receipts_outcome?: Array<{ outcome: { logs: string[] } }>;
};

export class WalletNotConnectedError extends Error {
  constructor() {
    super('Connect a wallet first');
    this.name = 'WalletNotConnectedError';
  }
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

  return active.signAndSendTransaction({
    signerId: wallet.accountId,
    receiverId: contractId,
    actions: [toAction({ methodName, args, ...options })]
  });
}

/** Sign and send several calls to the same contract as one transaction. */
export async function callFunctions(
  contractId: string,
  calls: FunctionCall[]
): Promise<TxOutcome> {
  const active = await getActiveWallet();
  if (!active || !wallet.accountId) throw new WalletNotConnectedError();

  return active.signAndSendTransaction({
    signerId: wallet.accountId,
    receiverId: contractId,
    actions: calls.map(toAction)
  });
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

  return active.signAndSendTransactions({
    transactions: transactions.map((tx) => ({
      receiverId: tx.receiverId,
      actions: tx.calls.map(toAction)
    }))
  });
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
