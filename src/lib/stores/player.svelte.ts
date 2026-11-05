import { browser } from '$app/env';
import { JOY_CONTRACT } from '#lib/config';
import { CREDIT_DECIMALS, JOY_DECIMALS, SECONDS_PER_DAY } from '#lib/economy/params';
import {
  EMPTY_PET_STATE,
  getPetStates,
  getStakeAccount,
  getStakePool,
  type PetStateView
} from '#lib/economy/stake';
import { getPets, type Pet } from '#lib/game/pets';
import { formatAmount } from '#lib/near/amount';
import { ftBalanceOf } from '#lib/near/rpc';
import { wallet } from '#lib/near/wallet.svelte';

export const player = $state({
  loading: false,
  loaded: false,
  pets: [] as Pet[],
  opponents: [] as Pet[],
  joy: 0,
  /** Credits already claimed and spendable. */
  credits: 0,
  /** Credits accrued since the last claim, at the moment of the last refresh. */
  pendingCredits: 0,
  creditsPerSecond: 0,
  stakedPetIds: [] as number[],
  weight: 0,
  totalWeight: 0,
  autoFeedBudget: 0,
  /** Item and battle state, keyed by pet id. */
  petStates: {} as Record<number, PetStateView>,
  /** Wall-clock ms of the last refresh, used to extrapolate pending credits. */
  refreshedAt: 0
});

export const petState = (petId: number): PetStateView =>
  player.petStates[petId] ?? EMPTY_PET_STATE(petId);

/**
 * Credits accrued right now: the on-chain pending amount plus the stream that
 * has run since we last read it. Lets the UI tick up smoothly without polling.
 */
export function liveCredits(nowMs: number): number {
  if (player.creditsPerSecond <= 0 || player.refreshedAt === 0) return player.pendingCredits;
  const elapsed = Math.max(0, (nowMs - player.refreshedAt) / 1000);
  return player.pendingCredits + player.creditsPerSecond * elapsed;
}

export const creditsPerDay = () => player.creditsPerSecond * SECONDS_PER_DAY;

export async function refreshPlayer(force = false): Promise<void> {
  if (!browser) return;
  const accountId = wallet.accountId;

  if (!accountId) {
    const { opponents } = await getPets(null);
    player.pets = [];
    player.opponents = opponents;
    player.loaded = true;
    return;
  }

  if (player.loading && !force) return;
  player.loading = true;

  try {
    const [{ mine, opponents }, joy, stake, pool] = await Promise.all([
      getPets(accountId),
      ftBalanceOf(JOY_CONTRACT, accountId),
      getStakeAccount(accountId),
      getStakePool()
    ]);

    player.pets = mine;
    player.opponents = opponents;
    player.joy = Number(formatAmount(joy, JOY_DECIMALS, 4));
    player.credits = Number(formatAmount(stake.credit_balance, CREDIT_DECIMALS));
    player.pendingCredits = Number(formatAmount(stake.pending_credits, CREDIT_DECIMALS));
    player.autoFeedBudget = Number(formatAmount(stake.auto_feed_budget, CREDIT_DECIMALS));
    player.stakedPetIds = stake.staked_pets ?? [];
    player.weight = Number(stake.weight || '0');
    player.totalWeight = Number(pool?.total_weight || '0');

    // Opponents' states matter too: the battle screen has to know who is
    // warded or wounded before it offers an attack.
    const ids = [...mine, ...opponents].map((pet) => pet.pet_id);
    player.petStates = await getPetStates(ids);

    const epochBudget = Number(pool?.epoch_budget || '0') / 10 ** CREDIT_DECIMALS;
    player.creditsPerSecond =
      player.totalWeight > 0 && player.weight > 0
        ? (epochBudget / (30 * SECONDS_PER_DAY)) * (player.weight / player.totalWeight)
        : 0;

    player.refreshedAt = Date.now();
    player.loaded = true;
  } finally {
    player.loading = false;
  }
}

export const isStaked = (petId: number) => player.stakedPetIds.includes(petId);
