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

/** Pet metadata as returned by `game.joyv1.near::get_all_pet_metadata`. */
export type Pet = {
  pet_id: number;
  owner_id: string;
  name: string;
  category: string;
  pet_species: number;
  pet_evolution_phase: number;
  level: number;
  star: number;
  score: number;
  pet_shield: number;
  /**
   * Last status the contract wrote. Stale: it only changes when somebody
   * touches the pet, so 14 of the 20 pets on mainnet still say `HAPPY` while
   * their starving deadline passed in 2024. Use `petStatus()` instead.
   */
  status: string;
  reward_debt: string | number;
  /**
   * Absolute timestamp, in nanoseconds, at which the pet starves — NOT a
   * remaining duration, despite the name.
   */
  time_until_starving: number;
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

export const petImage = (pet: Pet) =>
  `/assets/animation/${pet.category}/${pet.pet_evolution_phase}.gif`;

export type PetStatus = 'HAPPY' | 'HUNGRY' | 'DYING';

/**
 * Seconds left before the pet starves.
 *
 * `time_until_starving` is an absolute nanosecond timestamp. The old client
 * divided it by 1e7 and treated the result as a remaining duration, which made
 * the countdown read about 5,460 years.
 */
export function secondsUntilStarving(pet: Pet, nowMs = Date.now()): number {
  const deadlineMs = pet.time_until_starving / 1e6;
  return Math.max(0, Math.round((deadlineMs - nowMs) / 1000));
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
