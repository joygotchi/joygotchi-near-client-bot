import { GAS, MINT_ROUTER_CONTRACT, STAKE_CONTRACT, WNEAR_CONTRACT } from '#lib/config';
import { parseAmount } from '#lib/near/amount';
import { viewFunction } from '#lib/near/rpc';
import {
  ReceiptFailedError,
  callAcrossContracts,
  callFunction,
  callFunctions,
  failureLog
} from '#lib/near/tx';
import {
  CREDIT_DECIMALS,
  CREDIT_ITEMS,
  NEAR_DECIMALS,
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
  /** Of that budget, what auto-feed has already spent today. */
  auto_feed_spent: string;
  last_claim_at: string;
  /** Unused items, as [item_id, count]. */
  items: Array<[number, number]>;
};

/** One item offered for sale, from `pet-stake::get_listings`. */
export type Listing = {
  id: string;
  seller_id: string;
  item_id: number;
  quantity: number;
  /** Price of one item, in micro-credits. */
  price_each: string;
  created_at: string;
};

export type DropOdds = {
  weight: string;
  chance_bps: number;
  best_item: number;
  can_drop: number[];
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
  auto_feed_spent: '0',
  last_claim_at: '0',
  items: []
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

export async function getListings(
  fetchImpl: typeof fetch = fetch
): Promise<Listing[]> {
  try {
    return (
      (await viewFunction<Listing[]>(
        STAKE_CONTRACT,
        'get_listings',
        { from_index: 0, limit: 100 },
        fetchImpl
      )) ?? []
    );
  } catch {
    return [];
  }
}

export async function getDropOdds(
  accountId: string,
  fetchImpl: typeof fetch = fetch
): Promise<DropOdds | null> {
  try {
    return await viewFunction<DropOdds>(
      STAKE_CONTRACT,
      'get_drop_odds',
      { account_id: accountId },
      fetchImpl
    );
  } catch {
    return null;
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

/**
 * Put a pet to work in the AI lab. It stops being able to battle.
 *
 * The ownership check happens in a callback on the contract, which logs and
 * returns `false` instead of panicking — so the transaction succeeds either
 * way. Staking a pet somebody else owns looked like a clean success on testnet
 * until the pool was read back.
 */
export async function stakePet(petId: number) {
  const outcome = await callFunction(
    STAKE_CONTRACT,
    'stake_pet',
    { pet_id: petId },
    { gas: GAS.heavy, deposit: '1' }
  );

  const refused = failureLog(outcome, [
    'That pet belongs to someone else',
    'Could not read the pet from the game contract'
  ]);
  if (refused) throw new ReceiptFailedError(refused, outcome.transaction.hash);

  return outcome;
}

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
  callFunction(STAKE_CONTRACT, 'spend_credits', spendArgs(action, petId), { gas: GAS.heavy });

function spendArgs(action: CreditAction, petId: number | null) {
  return {
    action,
    pet_id: petId,
    amount: parseAmount(SINKS[action].credits.toString(), CREDIT_DECIMALS)
  };
}

/**
 * Claim and spend in one transaction.
 *
 * `harvest` folds earnings into `pending_credits`, and only `claim_credits`
 * moves those into the `credit_balance` that every spend checks. A player who
 * has been staking all week and has never pressed Claim has, as far as
 * `spend_credits` is concerned, nothing. Two actions in one transaction spare
 * them from finding out.
 *
 * Not unconditional: `claim_credits` panics with "Nothing to claim yet" when
 * `pending` is empty, which would take the spend down with it.
 */
export const claimAndSpendCredits = (action: CreditAction, petId: number | null = null) =>
  callFunctions(STAKE_CONTRACT, [
    { methodName: 'claim_credits', args: {}, gas: GAS.call },
    { methodName: 'spend_credits', args: spendArgs(action, petId), gas: GAS.call }
  ]);

export const setAutoFeedBudget = (creditsPerDay: number) =>
  callFunction(STAKE_CONTRACT, 'set_auto_feed_budget', {
    budget: parseAmount(creditsPerDay.toString(), CREDIT_DECIMALS)
  });

/**
 * Buy a credit item for a pet. Effects are pet state with a deadline or a
 * counter — none of them touch staking weight, which is what keeps the credit
 * accumulator exact.
 */
/** Buy items into your inventory. `useItem` applies one to a pet. */
export const buyCreditItem = (key: CreditItemKey, quantity = 1) =>
  callFunction(
    STAKE_CONTRACT,
    'buy_credit_item',
    { item_id: CREDIT_ITEMS[key].id, quantity },
    { gas: GAS.heavy }
  );

export const useItem = (key: CreditItemKey, petId: number) =>
  callFunction(
    STAKE_CONTRACT,
    'use_item',
    { item_id: CREDIT_ITEMS[key].id, pet_id: petId },
    { gas: GAS.heavy }
  );

/** Buy and apply in one transaction, which is what the shop button does. */
export const buyAndUseItem = (key: CreditItemKey, petId: number) =>
  callFunctions(STAKE_CONTRACT, [
    {
      methodName: 'buy_credit_item',
      args: { item_id: CREDIT_ITEMS[key].id, quantity: 1 },
      gas: GAS.call
    },
    {
      methodName: 'use_item',
      args: { item_id: CREDIT_ITEMS[key].id, pet_id: petId },
      gas: GAS.call
    }
  ]);

// ── Item market ──────────────────────────────────────────────────────────────

export const listItem = (key: CreditItemKey, quantity: number, priceEachCredits: number) =>
  callFunction(
    STAKE_CONTRACT,
    'list_item',
    {
      item_id: CREDIT_ITEMS[key].id,
      quantity,
      price_each: parseAmount(priceEachCredits.toString(), CREDIT_DECIMALS)
    },
    { gas: GAS.heavy }
  );

export const buyListing = (listingId: string, quantity: number) =>
  callFunction(
    STAKE_CONTRACT,
    'buy_listing',
    { listing_id: listingId, quantity },
    { gas: GAS.heavy }
  );

export const cancelListing = (listingId: string) =>
  callFunction(STAKE_CONTRACT, 'cancel_listing', { listing_id: listingId }, { gas: GAS.heavy });

/** Mining pays credits and requires a pet at work, so it cannot be farmed. */
export const claimMining = () =>
  callFunction(STAKE_CONTRACT, 'claim_mining', {}, { gas: GAS.heavy });

/**
 * Buy credits with NEAR, one way.
 *
 * Two steps in one signature: wrap the NEAR, then hand the wNEAR to the mint
 * router. The staking contract deliberately never touches tokens — the router
 * is the only account in the system that custodies funds — so this is the path
 * a player with NEAR takes, and it is the same path a ZEC payment ends on.
 */
export async function buyCreditsWithNear(nearAmount: number, accountId: string) {
  const deposit = parseAmount(nearAmount.toString(), NEAR_DECIMALS);

  return callAcrossContracts([
    {
      receiverId: WNEAR_CONTRACT,
      calls: [{ methodName: 'near_deposit', args: {}, deposit, gas: GAS.call }]
    },
    {
      receiverId: WNEAR_CONTRACT,
      calls: [
        {
          methodName: 'ft_transfer_call',
          args: {
            receiver_id: MINT_ROUTER_CONTRACT,
            amount: deposit,
            msg: JSON.stringify({ action: 'buy_credits', account_id: accountId })
          },
          deposit: '1',
          gas: GAS.transferCall
        }
      ]
    }
  ]);
}

/** One-off sponsored credits so new players can try AI feed before owning a pet. */
export const claimFaucetCredits = () =>
  callFunction(STAKE_CONTRACT, 'claim_faucet', {}, { gas: GAS.call });
