/**
 * Writing a feed's XP to the chain.
 *
 * `spend_credits("ai_feed")` debits the credits, the inference decides how good
 * the meal was, and this closes the loop: `pet-stake::award_feed_xp` forwards
 * the XP to `game::award_xp`, which levels the pet and re-syncs its staking
 * weight. Without it a pet's XP stays 0 however much it is fed — no levels, no
 * evolve, no star gates, and no reason to feed at all.
 *
 * Never fatal. By the time this runs the player's credits are already spent and
 * the inference has already been paid for, so a failure here degrades to "the
 * XP did not land" and says so in the response, the same way the JSON-parse
 * fallback does.
 */

import { NEAR_ORACLE_ACCOUNT_ID, NEAR_ORACLE_PRIVATE_KEY } from '$app/env/private';
import { GAS, RPC_URL, STAKE_CONTRACT } from '#lib/config';
import { MAX_FEED_XP } from '#lib/economy/params';
import { signAndSendFunctionCall } from './near-signer';

/** Whether this deployment can write XP at all. */
export const feedXpConfigured = () =>
  NEAR_ORACLE_ACCOUNT_ID.length > 0 && NEAR_ORACLE_PRIVATE_KEY.length > 0;

export type FeedXpResult =
  | { awarded: true; xp: number; txHash: string }
  | { awarded: false; xp: number; reason: string };

/**
 * Award XP for one feed.
 *
 * Clamped here as well as in the contract. The contract's clamp is the one
 * that protects the player — this one keeps the number the API reports and the
 * number the chain stores from disagreeing when something upstream goes wrong.
 */
export async function awardFeedXp(petId: number, xp: number): Promise<FeedXpResult> {
  const capped = Math.max(0, Math.min(MAX_FEED_XP, Math.round(xp)));

  if (capped === 0) {
    return { awarded: false, xp: 0, reason: 'The meal was worth no XP' };
  }
  if (!feedXpConfigured()) {
    return {
      awarded: false,
      xp: capped,
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
        methodName: 'award_feed_xp',
        args: { pet_id: petId, xp: capped },
        // Two hops: this contract calls the game contract, which calls back
        // here to re-sync weight when the pet levels.
        gas: BigInt(GAS.heavy),
        // A function-call access key cannot attach a deposit, and this method
        // does not want one.
        deposit: 0n
      }
    );
    return { awarded: true, xp: capped, txHash };
  } catch (err) {
    return {
      awarded: false,
      xp: capped,
      reason: err instanceof Error ? err.message : String(err)
    };
  }
}
