/**
 * JOYGOTCHI economy parameters.
 *
 * Two resources:
 *  - $JOY   — NEP-141 soft currency, earned by playing, spent on items.
 *  - Credit — "AI Credit" (ACR), 6 decimals, 1 ACR = US$1 of NEAR AI Cloud
 *             compute. Credits are not inflationary: they are the treasury's
 *             NEAR AI stake-to-compute allowance, streamed to whoever has pets
 *             staked. See docs/ECONOMY.md for the full rationale.
 */

export const CREDIT_DECIMALS = 6;
export const JOY_DECIMALS = 18;

/** One ACR in the smallest unit. */
export const ONE_CREDIT = 10n ** BigInt(CREDIT_DECIMALS);

export const SECONDS_PER_DAY = 86_400;
export const EPOCH_DAYS = 30;
export const EPOCH_SECONDS = EPOCH_DAYS * SECONDS_PER_DAY;

/**
 * Credit budget the treasury streams to pet stakers each 30-day epoch, in ACR.
 * This mirrors what NEAR AI grants for the treasury's staked NEAR (the published
 * example is ~US$5/month of credits per 500 NEAR staked), so the number is set
 * on-chain by the treasury rather than hard-coded in the client.
 */
export const DEFAULT_EPOCH_BUDGET_ACR = 500;

// ── Pet weight ───────────────────────────────────────────────────────────────

export type Rarity = 'common' | 'rare' | 'epic' | 'legendary';

/** Base weight per rarity. A legendary pet earns 4x what a common one does. */
export const RARITY_WEIGHT: Record<Rarity, number> = {
  common: 100,
  rare: 160,
  epic: 260,
  legendary: 400
};

/** Evolution phase multiplier (phase 1 = hatchling, 4 = mature). */
export const EVOLUTION_MULTIPLIER = [1, 1, 1.15, 1.35, 1.6];

export type PetWeightInput = {
  rarity?: Rarity;
  level?: number;
  star?: number;
  evolutionPhase?: number;
};

/**
 * Staking weight of one pet.
 *
 *   weight = base(rarity) x (1 + level/20) x (1 + star x 0.15) x evolution
 *
 * Levels and stars matter, but sub-linearly — a whale with one maxed pet cannot
 * crowd out a hundred casual players.
 */
export function petWeight(pet: PetWeightInput): number {
  const base = RARITY_WEIGHT[pet.rarity ?? 'common'];
  const level = Math.max(0, pet.level ?? 0);
  const star = Math.max(0, pet.star ?? 0);
  const phase = EVOLUTION_MULTIPLIER[Math.min(pet.evolutionPhase ?? 1, 4)] ?? 1;

  return Math.round(base * (1 + level / 20) * (1 + star * 0.15) * phase);
}

/**
 * Credits per second for a player, given their share of total staked weight.
 * Same accumulator shape as the game contract's existing `reward_debt` model.
 */
export function creditsPerSecond(
  playerWeight: number,
  totalWeight: number,
  epochBudgetAcr = DEFAULT_EPOCH_BUDGET_ACR
): number {
  if (totalWeight <= 0 || playerWeight <= 0) return 0;
  return (epochBudgetAcr / EPOCH_SECONDS) * (playerWeight / totalWeight);
}

export const creditsPerDay = (
  playerWeight: number,
  totalWeight: number,
  epochBudgetAcr = DEFAULT_EPOCH_BUDGET_ACR
) => creditsPerSecond(playerWeight, totalWeight, epochBudgetAcr) * SECONDS_PER_DAY;

// ── Credit sinks ─────────────────────────────────────────────────────────────

export type CreditAction =
  | 'ai_feed'
  | 'train'
  | 'evolve'
  | 'craft_item'
  | 'chat'
  | 'revive'
  | 'auto_feed';

export type SinkSpec = {
  action: CreditAction;
  label: string;
  /** Cost in ACR. */
  credits: number;
  /** Extra $JOY cost, in whole $JOY. */
  joy?: number;
  description: string;
  /** Does this action actually call NEAR AI inference? */
  usesInference: boolean;
};

/**
 * Every credit sink. The ones marked `usesInference` really do spend NEAR AI
 * compute, which is why they cost credits rather than $JOY: the sink is matched
 * by a real cost, so the economy cannot drift into pure inflation.
 */
export const SINKS: Record<CreditAction, SinkSpec> = {
  ai_feed: {
    action: 'ai_feed',
    label: 'AI Feed',
    credits: 0.02,
    description:
      'Your pet reasons about what it wants to eat. Matching its taste profile multiplies XP by 1.0–1.8x.',
    usesInference: true
  },
  train: {
    action: 'train',
    label: 'Train',
    credits: 0.05,
    joy: 10,
    description: 'A simulated sparring session raises ATK or DEF.',
    usesInference: true
  },
  evolve: {
    action: 'evolve',
    label: 'Evolve',
    credits: 0.5,
    joy: 100,
    description: 'Advance to the next evolution phase. Raises staking weight permanently.',
    usesInference: false
  },
  craft_item: {
    action: 'craft_item',
    label: 'Craft item',
    credits: 0.1,
    joy: 25,
    description: 'Generate an item whose stats are rolled from your pet’s history.',
    usesInference: true
  },
  chat: {
    action: 'chat',
    label: 'Talk to pet',
    credits: 0.005,
    description: 'One message to your pet, answered by NEAR AI with the pet’s own personality.',
    usesInference: true
  },
  revive: {
    action: 'revive',
    label: 'Revive',
    credits: 1,
    description: 'Bring a dying pet back to full health.',
    usesInference: false
  },
  auto_feed: {
    action: 'auto_feed',
    label: 'Auto-feed',
    credits: 0.015,
    description:
      'While you are away, the pet feeds itself from your credit balance, up to the budget you set.',
    usesInference: false
  }
};

/** $JOY fallback when a player has no credits — cheaper, but much less XP. */
export const JOY_FEED_COST = 1;
export const JOY_FEED_XP_PENALTY = 0.4;

/**
 * Buying credits with $JOY is deliberately a bad deal, and only goes one way.
 * It gives $JOY a permanent sink without letting credits leak back into a
 * tradeable token.
 */
export const JOY_PER_CREDIT = 400;

/** Daily sponsored credits so a new player can try AI feed before owning a pet. */
export const FAUCET_DAILY_CREDITS = 0.05;

// ── Levelling ────────────────────────────────────────────────────────────────

/** XP needed to go from `level` to `level + 1`. */
export const xpForLevel = (level: number) => Math.round(100 * Math.pow(1.18, Math.max(0, level)));

/** XP granted by one feed, before the taste multiplier. */
export const BASE_FEED_XP = 35;

export function formatCredits(credits: number, maxFraction = 3): string {
  if (!Number.isFinite(credits)) return '0';
  return credits.toLocaleString('en-US', {
    minimumFractionDigits: 0,
    maximumFractionDigits: maxFraction
  });
}
