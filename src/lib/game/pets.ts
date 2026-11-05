import { FT_CONTRACT, GAME_CONTRACT, GAS } from '#lib/config';
import { viewFunction } from '#lib/near/rpc';
import { callAndDecode, callFunction } from '#lib/near/tx';

/** Pet metadata as returned by `game.joyv1.near::get_all_pet_metadata`. */
export type Pet = {
  pet_id: number;
  owner_id: string;
  name: string;
  category: string;
  pet_evolution_phase: number;
  level: number;
  star: number;
  status: string;
  reward_debt: string | number;
  /** Nanoseconds until the pet starves. */
  time_until_starving: number;
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

export const ITEMS = [
  { id: 1, name: 'water', icon: '/assets/items/water.png', width: 20, joy: 1 },
  { id: 2, name: 'beef', icon: '/assets/items/beef.png', width: 40, joy: 1 },
  { id: 3, name: 'shield', icon: '/assets/items/shield.png', width: 40, joy: 1 },
  { id: 4, name: 'holy water', icon: '/assets/items/holy_water.png', width: 40, joy: 2 }
] as const;

export const petImage = (pet: Pet) =>
  `/assets/animation/${pet.category}/${pet.pet_evolution_phase}.gif`;

/** `time_until_starving` is in nanoseconds; the countdown wants seconds. */
export const secondsUntilStarving = (pet: Pet) => Math.max(0, Math.round(pet.time_until_starving / 1e7));

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

export const buyItem = (petId: number, itemId: number) =>
  callFunction(GAME_CONTRACT, 'buy_item', { pet_id: petId, item_id: itemId });

export const attack = (petId: number, fromId: number, toId: number) =>
  callFunction(GAME_CONTRACT, 'attack', { pet_id: petId, from_id: fromId, to_id: toId }, { gas: GAS.heavy });

export const checkAlive = (petId: number) =>
  callAndDecode<boolean>(GAME_CONTRACT, 'is_pet_alive', { pet_id: petId });

/** Legacy faucet: hands out a little $JOY once a minute. */
export const claimJoy = (accountId: string) =>
  callFunction(FT_CONTRACT, 'get_joychi', { addr_to: accountId });

/**
 * Can `mine` attack `target` right now? Mirrors the rules the old React client
 * checked before sending the transaction, so players get an explanation instead
 * of a failed transaction.
 */
export function attackBlockedReason(mine: Pet | undefined, target: Pet | undefined): string | null {
  if (!mine) return 'Pick one of your pets first';
  if (!target) return 'Pick an opponent';
  if (mine.status === 'DYING') return 'Your pet is dying — feed it first';
  if (target.status === 'DYING') return 'That opponent is already dying';
  if (mine.level > target.level) return 'You can only attack pets at your level or above';

  const lastAttacked = Number(target.last_attacked ?? 0);
  if (lastAttacked > 0) {
    const hoursSince = (Date.now() - lastAttacked / 1e6) / 3_600_000;
    if (hoursSince < 1) return 'That opponent was attacked less than an hour ago';
  }
  return null;
}
