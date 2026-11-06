import { describe, expect, it } from 'vitest';
import {
  CREDIT_ITEM_LIST,
  DROP_BASE_BPS,
  DROP_MAX_BPS,
  SINKS,
  creditItemById,
  creditsPerDay,
  creditsPerSecond,
  dropChanceBps,
  droppableItems,
  feedXp,
  formatCredits,
  petWeight,
  xpForLevel,
  EPOCH_SECONDS,
  LENS_BONUS,
  MAX_FEED_XP,
  MINING_COOLDOWN_SECONDS
} from '#lib/economy/params';

describe('petWeight', () => {
  it('starts a common pet at the rarity base', () => {
    expect(petWeight({ rarity: 'common', level: 0, star: 0, evolutionPhase: 1 })).toBe(100);
  });

  it('grows sub-linearly, so one maxed pet cannot crowd out a hundred casual ones', () => {
    const common = petWeight({ rarity: 'common', level: 0, star: 0, evolutionPhase: 1 });
    const maxed = petWeight({ rarity: 'legendary', level: 20, star: 5, evolutionPhase: 4 });

    expect(maxed).toBe(2240);
    expect(maxed).toBeLessThan(common * 30);
  });

  it('counts each of rarity, level, star and evolution', () => {
    const base = { rarity: 'common' as const, level: 0, star: 0, evolutionPhase: 1 };
    expect(petWeight({ ...base, rarity: 'rare' })).toBeGreaterThan(petWeight(base));
    expect(petWeight({ ...base, level: 10 })).toBeGreaterThan(petWeight(base));
    expect(petWeight({ ...base, star: 2 })).toBeGreaterThan(petWeight(base));
    expect(petWeight({ ...base, evolutionPhase: 4 })).toBeGreaterThan(petWeight(base));
  });

  it('defaults a pet with nothing known to the common base', () => {
    expect(petWeight({})).toBe(100);
  });
});

describe('drop odds', () => {
  // The bug this pins: the first curve used weight/20, which squeezed the
  // whole reachable range between 2.05% and 3.1% and left the ceiling
  // unreachable. "Stronger pets find more" was true only on paper.
  it('spans the weights pets actually reach', () => {
    expect(dropChanceBps(100)).toBe(300);
    expect(dropChanceBps(2240)).toBe(2440);
  });

  it('has a floor and a ceiling', () => {
    expect(dropChanceBps(0)).toBe(0);
    expect(dropChanceBps(1)).toBe(DROP_BASE_BPS + 1);
    expect(dropChanceBps(100_000)).toBe(DROP_MAX_BPS);
  });

  it('unlocks better items with weight, and nothing at all without a pet', () => {
    expect(droppableItems(0)).toEqual([]);
    expect(droppableItems(100)).toEqual(['tonic']);
    expect(droppableItems(2240)).toEqual(['tonic', 'charm', 'lens', 'ward']);
  });

  it('agrees with the contract on which items can drop', () => {
    const keys = CREDIT_ITEM_LIST.map((i) => i.key).sort();
    expect(droppableItems(2240).sort()).toEqual(keys);
  });
});

describe('credit stream', () => {
  it('pays the whole epoch budget to a sole staker', () => {
    expect(creditsPerSecond(100, 100, 500) * EPOCH_SECONDS).toBeCloseTo(500, 6);
  });

  it('splits strictly by weight share', () => {
    const a = creditsPerSecond(100, 260, 500);
    const b = creditsPerSecond(160, 260, 500);
    expect(b / a).toBeCloseTo(1.6, 6);
  });

  it('pays nothing without a stake, and never divides by zero', () => {
    expect(creditsPerSecond(0, 100, 500)).toBe(0);
    expect(creditsPerSecond(100, 0, 500)).toBe(0);
  });

  it('reports a day as 86,400 seconds of the same rate', () => {
    expect(creditsPerDay(100, 100, 500)).toBeCloseTo(creditsPerSecond(100, 100, 500) * 86_400, 9);
  });
});

describe('levelling', () => {
  it('costs 100 XP for the first level, then 18% more each time', () => {
    expect(xpForLevel(0)).toBe(100);
    expect(xpForLevel(1)).toBe(118);
    expect(xpForLevel(2)).toBe(139);
  });

  it('never gets cheaper', () => {
    for (let level = 1; level < 20; level++) {
      expect(xpForLevel(level)).toBeGreaterThan(xpForLevel(level - 1));
    }
  });
});

describe('feedXp', () => {
  it('pays 1.0x for a miss and 1.8x for a perfect match', () => {
    expect(feedXp(0)).toBe(35);
    expect(feedXp(1)).toBe(63);
  });

  it('adds the lens bonus on top', () => {
    expect(feedXp(1, true)).toBe(Math.round(35 * (1.8 + LENS_BONUS)));
    expect(feedXp(1, true)).toBeGreaterThan(feedXp(1));
  });

  it('clamps a taste match the model reports out of range', () => {
    expect(feedXp(-5)).toBe(feedXp(0));
    expect(feedXp(99)).toBe(feedXp(1));
  });

  // The XP award is signed by a server key, so the cap is what stands between
  // a compromised backend and a maxed pet. `MAX_FEED_XP` in
  // contracts/pet-stake/src/types.rs has to be this same number, and the
  // contract clamps to it — this side only keeps the two honest.
  it('never exceeds the cap the staking contract enforces', () => {
    expect(MAX_FEED_XP).toBe(72);
    expect(feedXp(1, true)).toBe(MAX_FEED_XP);

    for (const match of [0, 0.01, 0.25, 0.5, 0.75, 0.99, 1, 5, -1]) {
      expect(feedXp(match, true)).toBeLessThanOrEqual(MAX_FEED_XP);
      expect(feedXp(match, false)).toBeLessThanOrEqual(MAX_FEED_XP);
    }
  });

  it('leaves the cap well under the game contract\u2019s own clamp of 500', () => {
    // 500 is seven feeds' worth, which is five levels in five calls.
    expect(MAX_FEED_XP * 6).toBeLessThan(500);
  });
});

describe('sinks and items', () => {
  it('prices every sink above zero', () => {
    for (const sink of Object.values(SINKS)) {
      expect(sink.credits).toBeGreaterThan(0);
    }
  });

  it('keeps a feed affordable against what a lone staker earns in a day', () => {
    // Under one feed a day was the problem with the original 0.02 pricing.
    const daily = creditsPerDay(100, 5_000, 500);
    expect(daily / SINKS.ai_feed.credits).toBeGreaterThan(5);
  });

  it('resolves every credit item by its id', () => {
    for (const item of CREDIT_ITEM_LIST) {
      expect(creditItemById(item.id)).toBe(item);
    }
    expect(creditItemById(999)).toBeUndefined();
  });

  it('gives mining a cooldown measured in hours, not minutes', () => {
    expect(MINING_COOLDOWN_SECONDS).toBe(6 * 3600);
  });
});

describe('formatCredits', () => {
  it('trims to three decimals by default and survives rubbish input', () => {
    expect(formatCredits(1.23456)).toBe('1.235');
    expect(formatCredits(0)).toBe('0');
    expect(formatCredits(Number.NaN)).toBe('0');
  });
});
