import { runAiAction, type AiResult } from '#lib/ai/client';
import type { PetContext } from '#lib/ai/prompts';
import { evolutionPhase, secondsUntilStarving, type Pet } from '#lib/game/pets';
import { wallet } from '#lib/near/wallet.svelte';
import { refreshPlayer } from '#lib/stores/player.svelte';
import { SINKS, type CreditAction } from './params';
import { claimAndSpendCredits, spendCredits } from './stake';

export const toPetContext = (pet: Pet): PetContext => ({
  petId: pet.pet_id,
  name: pet.name,
  category: pet.category,
  level: pet.level,
  star: pet.star,
  status: pet.status,
  evolutionPhase: evolutionPhase(pet),
  timeUntilStarving: secondsUntilStarving(pet)
});

export class NotEnoughCreditsError extends Error {
  constructor(needed: number, have: number) {
    super(`Needs ${needed} AI credit, you have ${have.toFixed(3)}`);
    this.name = 'NotEnoughCreditsError';
  }
}

/**
 * Pay for an AI action on-chain, then run it.
 *
 * Two steps on purpose: the credits leave the staking contract first, and the
 * server will only run inference once it can see that transaction. That is what
 * stops the NEAR AI key from being drained by anyone who can call our endpoint.
 */
export async function performAiAction<T extends AiResult>(
  action: CreditAction,
  pet: Pet,
  options: { message?: string; creditBalance: number; pendingCredits?: number }
): Promise<T> {
  if (!wallet.accountId) throw new Error('Connect a wallet first');

  const cost = SINKS[action].credits;
  const pending = options.pendingCredits ?? 0;
  if (options.creditBalance + pending < cost) {
    throw new NotEnoughCreditsError(cost, options.creditBalance + pending);
  }

  // Credits sitting in `pending` are earned but unspendable until claimed, so
  // claim them in the same transaction rather than refusing the action.
  const outcome =
    options.creditBalance < cost
      ? await claimAndSpendCredits(action, pet.pet_id)
      : await spendCredits(action, pet.pet_id);
  const txHash = (outcome as { transaction?: { hash?: string } })?.transaction?.hash;
  if (!txHash) throw new Error('Could not read the payment transaction hash');

  try {
    return await runAiAction<T>({
      accountId: wallet.accountId,
      txHash,
      action,
      pet: toPetContext(pet),
      message: options.message
    });
  } finally {
    // The credits are gone either way, so keep the header honest.
    void refreshPlayer(true);
  }
}
