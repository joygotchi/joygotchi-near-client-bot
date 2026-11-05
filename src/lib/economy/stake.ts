import { GAS, JOY_CONTRACT, STAKE_CONTRACT } from '#lib/config';
import { parseAmount } from '#lib/near/amount';
import { viewFunction } from '#lib/near/rpc';
import { callFunction, ftTransferCall } from '#lib/near/tx';
import {
  CREDIT_DECIMALS,
  CREDIT_ITEMS,
  JOY_DECIMALS,
  SINKS,
  type CreditAction,
  type CreditItemKey,
  type Rarity
} from './params';

/** Shape returned by `pet-stake::get_account` (see contracts/pet-stake). */
export type StakeAccount = {
  account_id: string;
  /** Pet ids currently staked. */
  staked_pets: number[];
  /** Sum of the weights of those pets. */
  weight: string;
  /** Credits already claimed and not yet spent, in micro-credits. */
  credit_balance: string;
  /** Credits accrued but not yet claimed, in micro-credits. */
  pending_credits: string;
  /** Auto-feed budget per day, in micro-credits. 0 = off. */
  auto_feed_budget: string;
  last_claim_at: string;
};

export type StakePool = {
  total_weight: string;
  /** Credits streamed per 30-day epoch, in micro-credits. */
  epoch_budget: string;
  /** Micro-credits per second, derived from `epoch_budget`. */
  rate_per_second: string;
  staked_pets: number;
  updated_at: string;
};

/** Per-pet item and battle state, from `pet-stake::get_pet_state`. */
export type PetStateView = {
  pet_id: number;
  /** Nanosecond timestamp; 0 means not wounded. */
  wounded_until: string;
  /** Nanosecond timestamp; 0 means no ward. */
  shielded_until: string;
  lens_feeds_left: number;
  star_guard: boolean;
  staked: boolean;
  /** False while staked, warded or wounded. */
  attackable: boolean;
};

export const EMPTY_PET_STATE = (petId: number): PetStateView => ({
  pet_id: petId,
  wounded_until: '0',
  shielded_until: '0',
  lens_feeds_left: 0,
  star_guard: false,
  staked: false,
  attackable: true
});

/** Is a nanosecond deadline still in the future? */
export const activeUntil = (nanos: string, nowMs = Date.now()) =>
  Number(nanos || '0') / 1e6 > nowMs;

export type StakedPetView = {
  pet_id: number;
  owner_id: string;
  weight: string;
  rarity: Rarity;
  level: number;
  star: number;
  evolution_phase: number;
  staked_at: string;
};

const EMPTY_ACCOUNT = (accountId: string): StakeAccount => ({
  account_id: accountId,
  staked_pets: [],
  weight: '0',
  credit_balance: '0',
  pending_credits: '0',
  auto_feed_budget: '0',
  last_claim_at: '0'
});

// ── Views ────────────────────────────────────────────────────────────────────

export async function getStakePool(fetchImpl: typeof fetch = fetch): Promise<StakePool | null> {
  try {
    return await viewFunction<StakePool>(STAKE_CONTRACT, 'get_pool', {}, fetchImpl);
  } catch {
    return null;
  }
}

export async function getStakeAccount(
  accountId: string,
  fetchImpl: typeof fetch = fetch
): Promise<StakeAccount> {
  try {
    const account = await viewFunction<StakeAccount | null>(
      STAKE_CONTRACT,
      'get_account',
      { account_id: accountId },
      fetchImpl
    );
    return account ?? EMPTY_ACCOUNT(accountId);
  } catch {
    return EMPTY_ACCOUNT(accountId);
  }
}

export async function getStakedPets(
  accountId: string,
  fetchImpl: typeof fetch = fetch
): Promise<StakedPetView[]> {
  try {
    return (
      (await viewFunction<StakedPetView[]>(
        STAKE_CONTRACT,
        'get_staked_pets',
        { account_id: accountId },
        fetchImpl
      )) ?? []
    );
  } catch {
    return [];
  }
}

export async function getPetStates(
  petIds: number[],
  fetchImpl: typeof fetch = fetch
): Promise<Record<number, PetStateView>> {
  if (petIds.length === 0) return {};
  try {
    const states = await viewFunction<PetStateView[]>(
      STAKE_CONTRACT,
      'get_pet_states',
      { pet_ids: petIds },
      fetchImpl
    );
    return Object.fromEntries((states ?? []).map((state) => [state.pet_id, state]));
  } catch {
    return Object.fromEntries(petIds.map((id) => [id, EMPTY_PET_STATE(id)]));
  }
}

/** Credit item ids and prices as the contract has them. */
export async function getCreditItemPrices(
  fetchImpl: typeof fetch = fetch
): Promise<Record<number, string>> {
  try {
    const items = await viewFunction<Array<[number, string]>>(
      STAKE_CONTRACT,
      'get_items',
      {},
      fetchImpl
    );
    return Object.fromEntries(items ?? []);
  } catch {
    return {};
  }
}

// ── Calls ────────────────────────────────────────────────────────────────────

/** Put a pet to work in the AI lab. It stops being able to battle. */
export const stakePet = (petId: number) =>
  callFunction(STAKE_CONTRACT, 'stake_pet', { pet_id: petId }, { gas: GAS.heavy, deposit: '1' });

export const unstakePet = (petId: number) =>
  callFunction(STAKE_CONTRACT, 'unstake_pet', { pet_id: petId }, { gas: GAS.heavy, deposit: '1' });

/** Move `pending_credits` into the spendable `credit_balance`. */
export const claimCredits = () =>
  callFunction(STAKE_CONTRACT, 'claim_credits', {}, { gas: GAS.heavy });

/**
 * Spend credits on one action. The contract debits the balance and emits a
 * `credit_spent` event; `/api/ai/chat` verifies that event before it will run
 * any inference, so credits can never be spent twice.
 */
export const spendCredits = (action: CreditAction, petId: number | null = null) =>
  callFunction(
    STAKE_CONTRACT,
    'spend_credits',
    {
      action,
      pet_id: petId,
      amount: parseAmount(SINKS[action].credits.toString(), CREDIT_DECIMALS)
    },
    { gas: GAS.heavy }
  );

export const setAutoFeedBudget = (creditsPerDay: number) =>
  callFunction(STAKE_CONTRACT, 'set_auto_feed_budget', {
    budget: parseAmount(creditsPerDay.toString(), CREDIT_DECIMALS)
  });

/**
 * Buy a credit item for a pet. Effects are pet state with a deadline or a
 * counter — none of them touch staking weight, which is what keeps the credit
 * accumulator exact.
 */
export const buyCreditItem = (key: CreditItemKey, petId: number) =>
  callFunction(
    STAKE_CONTRACT,
    'buy_credit_item',
    { item_id: CREDIT_ITEMS[key].id, pet_id: petId },
    { gas: GAS.heavy }
  );

/** One-way $JOY -> credits. Deliberately a poor rate; see docs/ECONOMY.md. */
export const buyCreditsWithJoy = (joyAmount: number) =>
  ftTransferCall(
    JOY_CONTRACT,
    STAKE_CONTRACT,
    parseAmount(joyAmount.toString(), JOY_DECIMALS),
    JSON.stringify({ action: 'buy_credits' })
  );

/** One-off sponsored credits so new players can try AI feed before owning a pet. */
export const claimFaucetCredits = () =>
  callFunction(STAKE_CONTRACT, 'claim_faucet', {}, { gas: GAS.call });
