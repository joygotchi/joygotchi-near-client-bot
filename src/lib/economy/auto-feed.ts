/**
 * Which pets to auto-feed, and how many feeds each player can afford.
 *
 * Pure on purpose. The cron route around it does the I/O — read the opted-in
 * accounts, read their pets, sign one call per task — and this decides *what*
 * to do, which is the part with arithmetic in it and therefore the part worth
 * testing. An hourly job that quietly feeds the wrong pet, or feeds one pet
 * eleven times because the budget divided badly, is not something a deployment
 * notices.
 *
 * None of it is a security boundary. `pet-stake::auto_feed` re-checks the
 * budget, the balance and the daily window, and `game::feed_from_stake`
 * re-checks that the pet is the account's and still alive. This is here so the
 * cron does not waste gas on calls the chain would refuse — getting it wrong
 * costs an allowance, not a player's credits.
 */

import { AUTO_FEED_WHEN_UNDER_SECONDS } from './params';

/** An account that has opted in, as `get_auto_feed_accounts` reports it. */
export type AutoFeedAccount = {
  accountId: string;
  /** Micro-credits auto-feed could still spend today: budget left, capped by balance. */
  available: bigint;
};

/** A pet the account owns, as `game::get_pet` reports it. */
export type AutoFeedPet = {
  petId: number;
  ownerId: string;
  /** Absolute nanosecond timestamp at which the pet starves. */
  starvesAt: bigint;
};

export type AutoFeedTask = {
  accountId: string;
  petId: number;
  /** Seconds until this pet starves, at planning time. Logged, not sent. */
  secondsLeft: number;
};

export type AutoFeedPlanInput = {
  accounts: AutoFeedAccount[];
  /** Pets of those accounts. Pets whose owner is not in `accounts` are ignored. */
  pets: AutoFeedPet[];
  /** Now, in milliseconds. */
  nowMs: number;
  /** Price of one auto-feed, in micro-credits. */
  price: bigint;
  /** Only feed pets with less than this many seconds left. */
  whenUnderSeconds?: number;
};

/** Seconds until `starvesAt`, negative once the pet has starved. */
export const secondsUntilStarving = (starvesAt: bigint, nowMs: number): number =>
  Number(starvesAt / 1_000_000n) / 1000 - nowMs / 1000;

/**
 * How many auto-feeds an account can pay for right now.
 *
 * Integer division, floored: a budget of 0.02 ACR buys one 0.015 ACR feed and
 * not two, and the leftover 0.005 is simply not spent. Rounding up here would
 * mean every run ends with one call the contract refuses.
 */
export const affordableFeeds = (available: bigint, price: bigint): number => {
  if (price <= 0n || available < price) return 0;
  return Number(available / price);
};

/**
 * One feed per pet per run, nearest deadline first.
 *
 * Nearest-first is what makes a budget too small for every pet degrade the way
 * a player would want: with three hungry pets and two feeds left, the two
 * closest to dying eat. Feeding the same pet twice in one run would only bank
 * food it cannot use before the next run, so the plan never does.
 */
export function planAutoFeeds(input: AutoFeedPlanInput): AutoFeedTask[] {
  const { accounts, pets, nowMs, price } = input;
  const whenUnder = input.whenUnderSeconds ?? AUTO_FEED_WHEN_UNDER_SECONDS;

  const byOwner = new Map<string, AutoFeedPet[]>();
  for (const pet of pets) {
    const list = byOwner.get(pet.ownerId);
    if (list) list.push(pet);
    else byOwner.set(pet.ownerId, [pet]);
  }

  const tasks: AutoFeedTask[] = [];

  for (const account of accounts) {
    const feeds = affordableFeeds(account.available, price);
    if (feeds === 0) continue;

    const hungry = (byOwner.get(account.accountId) ?? [])
      .map((pet) => ({ pet, secondsLeft: secondsUntilStarving(pet.starvesAt, nowMs) }))
      // A pet that has already starved needs a revival item, not food: the game
      // contract refuses it, so planning the call would just burn gas.
      .filter(({ secondsLeft }) => secondsLeft > 0 && secondsLeft < whenUnder)
      .sort((a, b) => a.secondsLeft - b.secondsLeft || a.pet.petId - b.pet.petId);

    for (const { pet, secondsLeft } of hungry.slice(0, feeds)) {
      tasks.push({
        accountId: account.accountId,
        petId: pet.petId,
        secondsLeft: Math.round(secondsLeft)
      });
    }
  }

  return tasks;
}
