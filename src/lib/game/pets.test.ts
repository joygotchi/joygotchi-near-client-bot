import { describe, expect, it } from 'vitest';
import {
  attackBlockedReason,
  evolutionPhase,
  isAlive,
  PET_SPECIES,
  petImage,
  petStatus,
  secondsUntilStarving,
  starProgress,
  starvesAtMs,
  WINS_PER_STAR,
  type Pet
} from '#lib/game/pets';

const NOW = Date.UTC(2026, 10, 5, 12, 0, 0);
const ns = (ms: number) => ms * 1e6;

function pet(over: Partial<Pet> = {}): Pet {
  return {
    pet_id: 1,
    owner_id: 'alice.near',
    name: 'SMAUG',
    category: 'blackdragon',
    level: 1,
    star: 0,
    status: 'HAPPY',
    reward_debt: 0,
    starves_at: String(ns(NOW + 86_400_000)),
    evolution_phase: 1,
    ...over
  };
}

describe('starving deadline', () => {
  // The bug this pins: `time_until_starving` is an absolute nanosecond
  // timestamp, and the old client divided it by 1e7 and called the result
  // seconds remaining — which rendered a countdown of about 5,460 years.
  it('reads the field as a deadline, not a duration', () => {
    const p = pet({ starves_at: undefined, time_until_starving: ns(NOW + 3_600_000) });
    expect(secondsUntilStarving(p, NOW)).toBe(3600);
  });

  it('accepts either contract version of the field', () => {
    const v1 = pet({ starves_at: undefined, time_until_starving: ns(NOW + 7_200_000) });
    const v2 = pet({ starves_at: String(ns(NOW + 7_200_000)) });
    expect(secondsUntilStarving(v1, NOW)).toBe(secondsUntilStarving(v2, NOW));
    expect(starvesAtMs(v1)).toBe(starvesAtMs(v2));
  });

  it('never reports negative time once the deadline has passed', () => {
    expect(secondsUntilStarving(pet({ starves_at: String(ns(NOW - 86_400_000)) }), NOW)).toBe(0);
  });
});

describe('petStatus', () => {
  // v1 stored the status and only rewrote it when something touched the pet:
  // 14 of the 20 pets on mainnet still said HAPPY while starved since 2024.
  it('is derived from the deadline, ignoring the stored field', () => {
    const starved = pet({ status: 'HAPPY', starves_at: String(ns(NOW - 1000)) });
    expect(petStatus(starved, NOW)).toBe('DYING');
    expect(isAlive(starved, NOW)).toBe(false);
  });

  it('warns before it is too late', () => {
    expect(petStatus(pet({ starves_at: String(ns(NOW + 2 * 3_600_000)) }), NOW)).toBe('HUNGRY');
    expect(petStatus(pet({ starves_at: String(ns(NOW + 10 * 3_600_000)) }), NOW)).toBe('HAPPY');
  });
});

describe('PET_SPECIES', () => {
  // These strings are sent straight to `create_pet`, and the contract's
  // CATEGORIES list rejects anything else with "Unknown pet category".
  it('only names categories the contract accepts', () => {
    expect(PET_SPECIES.map((s) => s.category).sort()).toEqual(['blackdragon', 'greendragon']);
  });

  it('points every species at a sprite that follows the path convention', () => {
    for (const species of PET_SPECIES) {
      expect(species.preview).toBe(`/assets/animation/${species.category}/1.gif`);
    }
  });
});

describe('evolutionPhase', () => {
  it('reads either contract version, defaulting to the first phase', () => {
    expect(evolutionPhase(pet({ evolution_phase: 3 }))).toBe(3);
    expect(evolutionPhase(pet({ evolution_phase: undefined, pet_evolution_phase: 2 }))).toBe(2);
    expect(evolutionPhase(pet({ evolution_phase: undefined }))).toBe(1);
  });

  it('builds the sprite path from the phase it found', () => {
    expect(petImage(pet({ evolution_phase: undefined, pet_evolution_phase: 4 }))).toBe(
      '/assets/animation/blackdragon/4.gif'
    );
  });
});

describe('starProgress', () => {
  it('counts down to the next star', () => {
    expect(starProgress(pet({ win_streak: 1, wins_to_next_star: 2 }))).toEqual({
      streak: 1,
      needed: 2
    });
  });

  it('is null at the cap, and null on a contract that does not report streaks', () => {
    expect(starProgress(pet({ win_streak: 0, wins_to_next_star: null }))).toBeNull();
    expect(starProgress(pet())).toBeNull();
  });

  it('agrees with the contract on how many wins a star costs', () => {
    expect(WINS_PER_STAR).toBe(3);
  });
});

describe('attackBlockedReason', () => {
  const mine = pet({ pet_id: 1, level: 1 });
  const target = pet({ pet_id: 2, owner_id: 'bob.near', level: 2 });

  it('allows an ordinary matchup', () => {
    expect(attackBlockedReason(mine, target, { nowMs: NOW })).toBeNull();
  });

  it('needs both sides picked', () => {
    expect(attackBlockedReason(undefined, target, { nowMs: NOW })).toMatch(/your pets/i);
    expect(attackBlockedReason(mine, undefined, { nowMs: NOW })).toMatch(/opponent/i);
  });

  it('uses derived status, so a stale HAPPY does not let a starved pet fight', () => {
    const starved = pet({ status: 'HAPPY', starves_at: String(ns(NOW - 1)) });
    expect(attackBlockedReason(starved, target, { nowMs: NOW })).toMatch(/dying/i);
  });

  it('blocks a staked pet in both directions', () => {
    expect(attackBlockedReason(mine, target, { nowMs: NOW, mineStaked: true })).toMatch(/AI Lab/);
    expect(attackBlockedReason(mine, target, { nowMs: NOW, targetProtected: true })).toMatch(
      /protected/i
    );
  });

  it('keeps the level rule and the attack cooldown', () => {
    const weaker = pet({ pet_id: 2, owner_id: 'bob.near', level: 0 });
    expect(attackBlockedReason(mine, weaker, { nowMs: NOW })).toMatch(/level or above/);

    const justHit = pet({
      pet_id: 2,
      owner_id: 'bob.near',
      level: 2,
      last_attacked: ns(NOW - 60_000)
    });
    expect(attackBlockedReason(mine, justHit, { nowMs: NOW })).toMatch(/less than an hour/);
  });
});
