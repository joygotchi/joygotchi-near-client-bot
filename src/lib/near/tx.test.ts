import { describe, expect, it } from 'vitest';
import { ReceiptFailedError, assertReceiptsSucceeded, failureLog, type TxOutcome } from './tx';

const panic = (message: string) => ({
  Failure: { ActionError: { kind: { FunctionCallError: { ExecutionError: message } } } }
});

function outcome(parts: Partial<TxOutcome>): TxOutcome {
  return {
    transaction: { hash: 'HASH', signer_id: 'alice.testnet', receiver_id: 'stake.testnet' },
    status: { SuccessValue: '' },
    ...parts
  } as TxOutcome;
}

describe('assertReceiptsSucceeded', () => {
  // The whole point: on testnet, staking a pet owned by somebody else came back
  // as a clean success because the check runs in a cross-contract callback.
  it('throws on a receipt that failed under a successful transaction', () => {
    const tx = outcome({
      status: { SuccessValue: 'dHJ1ZQ==' },
      receipts_outcome: [
        { outcome: { logs: [], status: { SuccessValue: '' } } },
        { outcome: { logs: [], status: panic('Smart contract panicked: Pet is already staked') } }
      ]
    });

    expect(() => assertReceiptsSucceeded(tx)).toThrow(ReceiptFailedError);
    expect(() => assertReceiptsSucceeded(tx)).toThrow('Pet is already staked');
  });

  it('strips the boilerplate and keeps what the contract said', () => {
    const tx = outcome({ status: panic('Smart contract panicked: Attach 1 yoctoNEAR') });
    try {
      assertReceiptsSucceeded(tx);
      expect.unreachable('should have thrown');
    } catch (error) {
      expect((error as ReceiptFailedError).message).toBe('Attach 1 yoctoNEAR');
      expect((error as ReceiptFailedError).txHash).toBe('HASH');
    }
  });

  it('names the kind when the failure is not a contract panic', () => {
    const tx = outcome({ status: { Failure: { ActionError: { kind: { AccountDoesNotExist: {} } } } } });
    expect(() => assertReceiptsSucceeded(tx)).toThrow('AccountDoesNotExist');
  });

  it('passes a transaction whose every receipt succeeded, and one with no receipts', () => {
    const ok = outcome({
      receipts_outcome: [{ outcome: { logs: ['EVENT_JSON:{}'], status: { SuccessValue: '' } } }]
    });
    expect(assertReceiptsSucceeded(ok)).toBe(ok);
    expect(assertReceiptsSucceeded(outcome({}))).toBeTruthy();
  });
});

describe('failureLog', () => {
  // `on_pet_attrs` reports refusal by logging and returning false, so there is
  // no failed receipt to find — only the log line.
  const refused = outcome({
    status: { SuccessValue: 'ZmFsc2U=' },
    receipts_outcome: [
      { outcome: { logs: ['That pet belongs to someone else'], status: { SuccessValue: '' } } }
    ]
  });

  it('finds the line a callback left behind', () => {
    expect(failureLog(refused, ['That pet belongs to someone else'])).toBe(
      'That pet belongs to someone else'
    );
  });

  it('returns null when none of the patterns appear', () => {
    expect(failureLog(refused, ['Could not read the pet'])).toBeNull();
    expect(failureLog(outcome({}), ['anything'])).toBeNull();
  });
});
