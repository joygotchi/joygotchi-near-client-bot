/**
 * Spending the on-chain auto-feed budget (issue #8).
 *
 * The budget has been stored per account since the staking contract shipped,
 * and the AI Lab has a slider that writes it, but nothing ever spent it — a
 * pet whose owner was away starved with a full allowance sitting unused.
 *
 * The call that spends it is `pet-stake::auto_feed(account_id, pet_id)`, which
 * debits *the player's* balance against *the player's* budget. The obvious
 * alternative — a server key scoped to `spend_credits` — cannot work:
 * `spend_credits` debits `env::predecessor_account_id()`, so a server key would
 * spend the server's own credits, and the oracle account has no credit balance
 * and must never have one.
 *
 * So this reuses the feed-XP oracle key as it stands. The key is a
 * *function-call* access key limited to `award_feed_xp,auto_feed` on the
 * staking contract: a leak can top up a stranger's pet inside that stranger's
 * own daily budget and nothing else.
 */

import { NEAR_ORACLE_ACCOUNT_ID, NEAR_ORACLE_PRIVATE_KEY } from '$app/env/private';
import { GAS, RPC_URL, STAKE_CONTRACT } from '#lib/config';
import { signAndSendFunctionCall } from './near-signer';

/** Whether this deployment holds a key that can auto-feed at all. */
export const autoFeedConfigured = () =>
  NEAR_ORACLE_ACCOUNT_ID.length > 0 && NEAR_ORACLE_PRIVATE_KEY.length > 0;

export type AutoFeedOutcome =
  | { fed: true; accountId: string; petId: number; txHash: string }
  | { fed: false; accountId: string; petId: number; reason: string };

/**
 * Auto-feed one pet.
 *
 * Never throws. One account failing — a budget spent by an earlier run, a pet
 * sold between the read and the call, a pet that starved before the cron got to
 * it — must not stop the rest of the run, so every failure comes back as data.
 */
export async function autoFeedPet(accountId: string, petId: number): Promise<AutoFeedOutcome> {
  if (!autoFeedConfigured()) {
    return {
      fed: false,
      accountId,
      petId,
      reason: 'No oracle key on this deployment (set NEAR_ORACLE_ACCOUNT_ID and NEAR_ORACLE_PRIVATE_KEY)'
    };
  }

  try {
    const { txHash } = await signAndSendFunctionCall(
      {
        rpcUrl: RPC_URL,
        accountId: NEAR_ORACLE_ACCOUNT_ID,
        secretKey: NEAR_ORACLE_PRIVATE_KEY
      },
      STAKE_CONTRACT,
      {
        methodName: 'auto_feed',
        args: { account_id: accountId, pet_id: petId },
        // Two hops: the staking contract calls the game contract and then its
        // own callback, which refunds the credits if the feed was refused.
        gas: BigInt(GAS.heavy),
        // A function-call access key cannot attach a deposit.
        deposit: 0n
      }
    );
    return { fed: true, accountId, petId, txHash };
  } catch (err) {
    return {
      fed: false,
      accountId,
      petId,
      reason: err instanceof Error ? err.message : String(err)
    };
  }
}
