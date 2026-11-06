import { describe, expect, it } from 'vitest';
import {
  affordableFeeds,
  planAutoFeeds,
  secondsUntilStarving,
  type AutoFeedAccount,
  type AutoFeedPet
} from './auto-feed';
import { AUTO_FEED_WHEN_UNDER_SECONDS, SINKS } from './params';

const NOW_MS = 1_770_000_000_000;
/** One auto-feed, in micro-credits. */
const PRICE = BigInt(Math.round(SINKS.auto_feed.credits * 1e6));

/** A pet with `seconds` of food left, as the game contract would report it. */
function pet(petId: number, ownerId: string, seconds: number): AutoFeedPet {
  return {
    petId,
    ownerId,
    starvesAt: BigInt(Math.round((NOW_MS / 1000 + seconds) * 1e9))
  };
}

const account = (accountId: string, available: bigint): AutoFeedAccount => ({
  accountId,
  available
});

const plan = (accounts: AutoFeedAccount[], pets: AutoFeedPet[], whenUnderSeconds?: number) =>
  planAutoFeeds({ accounts, pets, nowMs: NOW_MS, price: PRICE, whenUnderSeconds });

describe('the auto-feed price', () => {
  // The contract charges its own configured price and this is the number the
  // planner divides the budget by. If they drift, every run plans one call too
  // many or leaves an affordable feed unspent.
  it('is the 15,000 micro-credits the contract is deployed with', () => {
    expect(PRICE).toBe(15_000n);
  });
});

describe('affordableFeeds', () => {
  it('floors, so a leftover under one feed is simply not spent', () => {
    expect(affordableFeeds(20_000n, PRICE)).toBe(1);
    expect(affordableFeeds(30_000n, PRICE)).toBe(2);
    expect(affordableFeeds(44_999n, PRICE)).toBe(2);
  });

  it('is zero for a budget that cannot afford a single feed', () => {
    expect(affordableFeeds(0n, PRICE)).toBe(0);
    expect(affordableFeeds(14_999n, PRICE)).toBe(0);
  });

  it('is zero rather than infinite when the price is missing', () => {
    expect(affordableFeeds(1_000_000n, 0n)).toBe(0);
  });
});

describe('secondsUntilStarving', () => {
  it('reads the contract’s absolute nanosecond deadline', () => {
    expect(secondsUntilStarving(pet(1, 'a.near', 3_600).starvesAt, NOW_MS)).toBeCloseTo(3_600, 0);
  });

  it('goes negative once the pet has starved', () => {
    expect(secondsUntilStarving(pet(1, 'a.near', -60).starvesAt, NOW_MS)).toBeLessThan(0);
  });
});

describe('planAutoFeeds', () => {
  it('feeds a hungry pet for an account that can afford it', () => {
    const tasks = plan([account('alice.near', 100_000n)], [pet(1, 'alice.near', 3_600)]);
    expect(tasks).toEqual([{ accountId: 'alice.near', petId: 1, secondsLeft: 3_600 }]);
  });

  // Not an optimisation: feeding a pet with 20 hours left spends the player's
  // budget to bank food, and the contract caps banked food anyway.
  it('leaves a pet that is not hungry yet alone', () => {
    expect(plan([account('alice.near', 100_000n)], [pet(1, 'alice.near', 20 * 3_600)])).toEqual([]);
  });

  it('uses six hours as the default threshold', () => {
    const justInside = AUTO_FEED_WHEN_UNDER_SECONDS - 60;
    const justOutside = AUTO_FEED_WHEN_UNDER_SECONDS + 60;
    const tasks = plan(
      [account('alice.near', 1_000_000n)],
      [pet(1, 'alice.near', justInside), pet(2, 'alice.near', justOutside)]
    );
    expect(tasks.map((t) => t.petId)).toEqual([1]);
  });

  // A starved pet needs a revival item, which auto-feed is not. The game
  // contract refuses it, so planning the call would only burn gas.
  it('skips a pet that has already starved', () => {
    expect(plan([account('alice.near', 100_000n)], [pet(1, 'alice.near', -600)])).toEqual([]);
  });

  it('spends nothing for an account whose allowance is used up', () => {
    expect(plan([account('alice.near', 0n)], [pet(1, 'alice.near', 600)])).toEqual([]);
  });

  it('spends nothing when what is left cannot buy one feed', () => {
    expect(plan([account('alice.near', 14_999n)], [pet(1, 'alice.near', 600)])).toEqual([]);
  });

  // The behaviour a player with several pets and a small budget would want:
  // whoever is closest to dying eats.
  it('feeds the nearest deadlines first when the budget runs short', () => {
    const tasks = plan(
      [account('alice.near', PRICE * 2n)],
      [
        pet(1, 'alice.near', 5 * 3_600),
        pet(2, 'alice.near', 600),
        pet(3, 'alice.near', 2 * 3_600)
      ]
    );
    expect(tasks.map((t) => t.petId)).toEqual([2, 3]);
  });

  it('never feeds the same pet twice in one run', () => {
    const tasks = plan([account('alice.near', PRICE * 5n)], [pet(1, 'alice.near', 600)]);
    expect(tasks).toHaveLength(1);
  });

  it('breaks a deadline tie on pet id, so a run is deterministic', () => {
    const tasks = plan(
      [account('alice.near', PRICE)],
      [pet(9, 'alice.near', 600), pet(4, 'alice.near', 600)]
    );
    expect(tasks.map((t) => t.petId)).toEqual([4]);
  });

  it('keeps each account to its own budget and its own pets', () => {
    const tasks = plan(
      [account('alice.near', PRICE), account('bob.near', PRICE * 2n)],
      [
        pet(1, 'alice.near', 600),
        pet(2, 'alice.near', 1_200),
        pet(3, 'bob.near', 600),
        pet(4, 'bob.near', 1_200)
      ]
    );
    expect(tasks).toEqual([
      { accountId: 'alice.near', petId: 1, secondsLeft: 600 },
      { accountId: 'bob.near', petId: 3, secondsLeft: 600 },
      { accountId: 'bob.near', petId: 4, secondsLeft: 1_200 }
    ]);
  });

  // The pets list is read per account, but a pet sold between the two reads
  // would arrive under its new owner. Grouping by owner rather than trusting
  // the order of the reads is what keeps that from charging the wrong player.
  it('ignores a pet whose owner did not opt in', () => {
    expect(plan([account('alice.near', PRICE)], [pet(1, 'bob.near', 600)])).toEqual([]);
  });

  it('does nothing when nobody opted in', () => {
    expect(plan([], [pet(1, 'alice.near', 600)])).toEqual([]);
  });
});
