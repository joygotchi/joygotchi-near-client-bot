import { GAME_CONTRACT, GAS } from '#lib/config';
import { viewFunction } from '#lib/near/rpc';
import { callAndDecode, callFunction } from '#lib/near/tx';

/** One owned item instance, as it appears inside `Pet.items`. */
export type PetItem = {
  item_id: number;
  name: string;
  points: number;
  price: number;
  price_delta: number;
  stock: number;
  shield: number;
  time_extension: number;
  is_revival: boolean;
};

/**
 * Pet metadata from `get_all_pet_metadata`.
 *
 * Both contract versions are accepted, because the v2 contract renamed two
 * fields for good reasons and the v1 one is still what is deployed:
 *
 * | v1 | v2 | why |
 * |---|---|---|
 * | `time_until_starving` | `starves_at` | it was always an absolute timestamp, and the old name had the client rendering a 5,460-year countdown |
 * | `pet_evolution_phase` | `evolution_phase` | the prefix was noise |
 * | `status` (stored, stale) | `status` (derived) | v1 only rewrote it when something touched the pet |
 *
 * Read through the helpers below rather than touching these directly.
 */
export type Pet = {
  pet_id: number;
  owner_id: string;
  name: string;
  category: string;
  /** v2 only. */
  rarity?: 'common' | 'rare' | 'epic' | 'legendary';
  pet_species?: number;
  /** v1 name. */
  pet_evolution_phase?: number;
  /** v2 name. */
  evolution_phase?: number;
  level: number;
  star: number;
  /** v2 only. */
  xp?: number;
  score?: number;
  pet_shield?: number;
  /** v2 only: blocks one attack. */
  shield?: number;
  /** v2 only: consecutive wins since the last defeat. */
  win_streak?: number;
  /** v2 only: wins still needed for the next star, `null` at the cap. */
  wins_to_next_star?: number | null;
  /** v2 only: true while the pet is staked. */
  locked?: boolean;
  /**
   * Last status the contract wrote. Stale: it only changes when somebody
   * touches the pet, so 14 of the 20 pets on mainnet still say `HAPPY` while
   * their starving deadline passed in 2024. Use `petStatus()` instead.
   */
  status: string;
  reward_debt: string | number;
  /** v1: an absolute nanosecond timestamp, despite the name. */
  time_until_starving?: number;
  /** v2: the same value, named for what it is. */
  starves_at?: string | number;
  time_pet_born?: number;
  items?: PetItem[];
  pet_need_evolution_item?: boolean;
  pet_has_evolution_item?: boolean;
  pet_evolution_item_id?: number;
  last_attack_used?: number | string;
  last_attacked?: number | string;
};

export const PET_SPECIES = [
  {
    category: 'blackdragon',
    label: 'BLACK DRAGON',
    atk: 100,
    def: 100,
    preview: '/assets/animation/blackdragon/1.gif'
  },
  {
    category: 'greendragon',
    label: 'GREEN DRAGON',
    atk: 100,
    def: 100,
    preview: '/assets/animation/greendragon/1.gif'
  }
] as const;

/*
 * The game contract has its own item shop, priced in a token that turned out
 * never to exist on-chain (`faucet.joyv1.near` is not a NEP-141 and exposes no
 * balance view at all). Items are credit items in `pet-stake` instead — see
 * `CREDIT_ITEMS` in `#lib/economy/params`. `buyItem` stays for whenever the
 * game contract gets NEAR pricing.
 */

/** Evolution phase, under either contract's field name. */
export const evolutionPhase = (pet: Pet) => pet.evolution_phase ?? pet.pet_evolution_phase ?? 1;

export const petImage = (pet: Pet) =>
  `/assets/animation/${pet.category}/${evolutionPhase(pet)}.gif`;

/** The starving deadline in epoch milliseconds, under either field name. */
export function starvesAtMs(pet: Pet): number {
  const raw = pet.starves_at ?? pet.time_until_starving ?? 0;
  return Number(raw) / 1e6;
}

export type PetStatus = 'HAPPY' | 'HUNGRY' | 'DYING';

/** Consecutive wins that earn a star. Mirrors `WINS_PER_STAR` in the contract. */
export const WINS_PER_STAR = 3;

/**
 * How close a pet is to its next star, or `null` when it is capped or the
 * contract does not report streaks (v1).
 */
export function starProgress(pet: Pet): { streak: number; needed: number } | null {
  if (pet.win_streak === undefined) return null;
  if (pet.wins_to_next_star === null) return null;
  return { streak: pet.win_streak, needed: pet.wins_to_next_star ?? WINS_PER_STAR };
}

/**
 * Seconds left before the pet starves.
 *
 * `time_until_starving` is an absolute nanosecond timestamp. The old client
 * divided it by 1e7 and treated the result as a remaining duration, which made
 * the countdown read about 5,460 years.
 */
export function secondsUntilStarving(pet: Pet, nowMs = Date.now()): number {
  return Math.max(0, Math.round((starvesAtMs(pet) - nowMs) / 1000));
}

/**
 * Status derived from the deadline rather than read from `Pet.status`, which
 * the contract only refreshes when something touches the pet.
 */
export function petStatus(pet: Pet, nowMs = Date.now()): PetStatus {
  const left = secondsUntilStarving(pet, nowMs);
  if (left <= 0) return 'DYING';
  if (left < 3 * 3600) return 'HUNGRY';
  return 'HAPPY';
}

export const isAlive = (pet: Pet, nowMs = Date.now()) => secondsUntilStarving(pet, nowMs) > 0;

// ── Views ────────────────────────────────────────────────────────────────────

export async function getAllPets(fetchImpl: typeof fetch = fetch): Promise<Pet[]> {
  try {
    return (await viewFunction<Pet[]>(GAME_CONTRACT, 'get_all_pet_metadata', {}, fetchImpl)) ?? [];
  } catch {
    return [];
  }
}

export async function getPets(
  accountId: string | null,
  fetchImpl: typeof fetch = fetch
): Promise<{ mine: Pet[]; opponents: Pet[] }> {
  const all = await getAllPets(fetchImpl);
  if (!accountId) return { mine: [], opponents: all };
  return {
    mine: all.filter((pet) => pet.owner_id === accountId),
    opponents: all.filter((pet) => pet.owner_id !== accountId)
  };
}

export const isPetAlive = (petId: number, fetchImpl: typeof fetch = fetch) =>
  viewFunction<boolean>(GAME_CONTRACT, 'is_pet_alive', { pet_id: petId }, fetchImpl);

// ── Calls ────────────────────────────────────────────────────────────────────

export const createPet = (name: string, metadata: { title: string; description: string; media: string }) =>
  callFunction(GAME_CONTRACT, 'create_pet', { name, metadata }, { gas: GAS.heavy });

export const renamePet = (petId: number, name: string) =>
  callFunction(GAME_CONTRACT, 'change_name_pet', { pet_id: petId, name });

/** The game contract's own shop. Not surfaced until it is priced in NEAR. */
export const buyItem = (petId: number, itemId: number) =>
  callFunction(GAME_CONTRACT, 'buy_item', { pet_id: petId, item_id: itemId });

export const attack = (petId: number, fromId: number, toId: number) =>
  callFunction(GAME_CONTRACT, 'attack', { pet_id: petId, from_id: fromId, to_id: toId }, { gas: GAS.heavy });

export const checkAlive = (petId: number) =>
  callAndDecode<boolean>(GAME_CONTRACT, 'is_pet_alive', { pet_id: petId });

/**
 * Can `mine` attack `target` right now? Mirrors the rules the old React client
 * checked before sending the transaction, so players get an explanation instead
 * of a failed transaction.
 */
export function attackBlockedReason(
  mine: Pet | undefined,
  target: Pet | undefined,
  options: { nowMs?: number; mineStaked?: boolean; targetProtected?: boolean } = {}
): string | null {
  const nowMs = options.nowMs ?? Date.now();

  if (!mine) return 'Pick one of your pets first';
  if (!target) return 'Pick an opponent';

  // Statuses are derived, because `Pet.status` lags behind reality.
  if (petStatus(mine, nowMs) === 'DYING') return 'Your pet is dying — feed it first';
  if (petStatus(target, nowMs) === 'DYING') return 'That opponent is already dying';

  if (options.mineStaked) return 'This pet is working in the AI Lab — unstake it to fight';
  if (options.targetProtected) return 'That opponent is protected right now';

  if (mine.level > target.level) return 'You can only attack pets at your level or above';

  const lastAttacked = Number(target.last_attacked ?? 0);
  if (lastAttacked > 0) {
    const hoursSince = (nowMs - lastAttacked / 1e6) / 3_600_000;
    if (hoursSince < 1) return 'That opponent was attacked less than an hour ago';
  }
  return null;
}
