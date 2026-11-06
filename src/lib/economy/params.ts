/**
 * JOYGOTCHI economy parameters.
 *
 * Two resources, and only two:
 *  - NEAR   — the money rail. It buys pets and credits, and nothing else.
 *  - Credit — "AI Credit" (ACR), 6 decimals, 1 ACR = US$1 of NEAR AI Cloud
 *             compute. The only thing players earn and the only thing they
 *             spend in game.
 *
 * There is no game token. A mintable soft currency is what lets a game reward
 * play cheaply, so removing it means rewards had to move somewhere — they pay
 * credits, which are capped by the treasury's NEAR AI allowance rather than by
 * a number someone chose. See docs/ECONOMY.md.
 */

export const CREDIT_DECIMALS = 6;
export const NEAR_DECIMALS = 24;

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

  description: string;
  /** Does this action actually call NEAR AI inference? */
  usesInference: boolean;
};

/**
 * Every credit sink. The ones marked `usesInference` really do spend NEAR AI
 * compute, which is why they cost credits at all: the sink is matched
 * by a real cost, so the economy cannot drift into pure inflation.
 */
export const SINKS: Record<CreditAction, SinkSpec> = {
  ai_feed: {
    action: 'ai_feed',
    label: 'AI Feed',
    // 0.002 ACR, not 0.02. A ~200-token completion costs roughly $0.0004, so
    // this still carries a 5x margin — but at $0.02 the whole 500 ACR epoch
    // budget bought only 25,000 feeds a month, which is under one feed per
    // player per day once a thousand people play. See docs/ECONOMY.md.
    credits: 0.002,
    description:
      'Your pet reasons about what it wants to eat. Matching its taste profile multiplies XP by 1.0–1.8x.',
    usesInference: true
  },
  train: {
    action: 'train',
    label: 'Train',
    credits: 0.008,
    description: 'A simulated sparring session raises ATK or DEF.',
    usesInference: true
  },
  evolve: {
    action: 'evolve',
    label: 'Evolve',
    credits: 0.5,
    description: 'Advance to the next evolution phase. Raises staking weight permanently.',
    usesInference: false
  },
  craft_item: {
    action: 'craft_item',
    label: 'Craft item',
    credits: 0.02,
    description: 'Generate an item whose stats are rolled from your pet’s history.',
    usesInference: true
  },
  chat: {
    action: 'chat',
    label: 'Talk to pet',
    credits: 0.001,
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

/**
 * NEAR needed for one credit, mirroring `mint-router::near_per_credit`.
 *
 * A credit is a dollar of compute and NEAR trades around $4, so this is about
 * a 2x markup: buying credits is meant to be worse than earning them. One way
 * only — credits never convert back into anything tradeable.
 */
export const NEAR_PER_CREDIT = 0.5;

// ── Auto-feed ────────────────────────────────────────────────────────────────

/**
 * How long one auto-feed window lasts. The allowance is daily, and the window
 * is a fixed UTC day — `block_timestamp / AUTO_FEED_WINDOW_SECONDS` on chain —
 * so it resets at 00:00 UTC rather than 24 hours after a player's last feed.
 *
 * Keep in step with `AUTO_FEED_WINDOW_SECONDS` in
 * contracts/pet-stake/src/types.rs.
 */
export const AUTO_FEED_WINDOW_SECONDS = SECONDS_PER_DAY;

/**
 * How much life one auto-feed buys — half a full meal, so two a day keep a pet
 * alive. At the 0.015 ACR price that is 0.03 ACR/day per pet, well inside the
 * 0..1 ACR/day the AI Lab slider offers.
 *
 * Keep in step with `AUTO_FEED_SECONDS` in contracts/game/src/types.rs.
 */
export const AUTO_FEED_EXTENDS_SECONDS = 12 * 3600;

/**
 * How close to starving a pet has to be before the cron feeds it.
 *
 * Not zero, and not "always": feeding a pet that has 20 hours left spends the
 * player's budget to bank food they did not need, and the game contract caps
 * banked food at 7 days anyway. Six hours leaves a cron that runs hourly five
 * missed runs of slack before a pet actually dies.
 */
export const AUTO_FEED_WHEN_UNDER_SECONDS = 6 * 3600;

/**
 * A one-off sponsored grant so a new player can try AI Feed before owning a
 * pet. Deliberately once per account and small: a daily 0.05 ACR faucet is
 * worth $18/account/year, and NEAR implicit accounts are nearly free to
 * create, so a script with a thousand accounts would drain a $500 monthly
 * budget three times over.
 */
export const FAUCET_GRANT_CREDITS = 0.01;

// ── Credit items ─────────────────────────────────────────────────────────────

/**
 * Items paid for in AI credit, held by the staking contract.
 *
 * The game contract's own shop ($JOY: Beef, Water, Shield, Holy Water) is
 * fixed on-chain and cannot be extended from here, so these live in
 * `pet-stake` instead — see `JOY_ITEMS` in `#lib/game/pets` for the other set.
 *
 * Every effect here is pet *state* with a deadline or a counter. None of them
 * touch staking weight: the credit accumulator is exact and well tested, and a
 * time-varying weight would need the pool to be updated at a moment when
 * nothing is calling it. Keeping items out of the weight maths is what makes
 * them safe to add.
 */
export const CREDIT_ITEM_IDS = {
  tonic: 101,
  ward: 102,
  lens: 103,
  charm: 104
} as const;

export type CreditItemKey = keyof typeof CREDIT_ITEM_IDS;

export type CreditItem = {
  id: number;
  key: CreditItemKey;
  label: string;
  credits: number;
  icon: string;
  description: string;
  /** Short line shown once the item is active on a pet. */
  activeLabel: string;
};

export const CREDIT_ITEMS: Record<CreditItemKey, CreditItem> = {
  tonic: {
    id: CREDIT_ITEM_IDS.tonic,
    key: 'tonic',
    label: 'AI Tonic',
    credits: 0.01,
    icon: '/assets/items/holy_water.png',
    description: 'Clears a wound immediately instead of waiting it out.',
    activeLabel: 'Healed'
  },
  ward: {
    id: CREDIT_ITEM_IDS.ward,
    key: 'ward',
    label: 'Sentinel Ward',
    credits: 0.05,
    icon: '/assets/items/shield.png',
    description: 'Nobody can attack this pet for 24 hours. Works while staked.',
    activeLabel: 'Warded'
  },
  lens: {
    id: CREDIT_ITEM_IDS.lens,
    key: 'lens',
    label: 'Focus Lens',
    credits: 0.02,
    icon: '/assets/item/star.png',
    description: 'The next 10 AI feeds get +0.25 on the XP multiplier.',
    activeLabel: 'Focused'
  },
  charm: {
    id: CREDIT_ITEM_IDS.charm,
    key: 'charm',
    label: 'Star Charm',
    credits: 0.015,
    icon: '/assets/icon/diamond.svg',
    description: 'Keeps your star if you lose the next battle.',
    activeLabel: 'Charmed'
  }
};

export const CREDIT_ITEM_LIST = Object.values(CREDIT_ITEMS);

export const creditItemById = (id: number) =>
  CREDIT_ITEM_LIST.find((item) => item.id === id);

/** How long a Sentinel Ward lasts. */
export const WARD_HOURS = 24;
/** How long a battle wound lasts without a tonic. */
export const WOUND_HOURS = 6;
/** Feeds covered by one Focus Lens. */
export const LENS_FEEDS = 10;
/** Extra XP multiplier while a Focus Lens is active. */
export const LENS_BONUS = 0.25;

// ── Battle ───────────────────────────────────────────────────────────────────

/**
 * Credits a challenger must put up to attack, transferred to the winner.
 *
 * Battle moves credits between players rather than destroying a pet. Pets
 * never die from a fight: with credits streamed by weight share, killing
 * another staker's pet would raise the killer's own income, which makes
 * griefing directly profitable. See the "post-battle" section of
 * docs/ECONOMY.md.
 */
export const BATTLE_WAGER_CREDITS = 0.01;

/** Share of the pot the treasury takes, in basis points. */
export const BATTLE_RAKE_BPS = 500;

// ── Drops and the item market ────────────────────────────────────────────────

/**
 * Chance of an item drop, in basis points, from a player's staked weight.
 * Mirrors `drop_odds` in the contract, which is the authority.
 */
export const DROP_BASE_BPS = 200;
export const DROP_MAX_BPS = 2_500;
export const DROP_WEIGHT_PER_BPS = 1;

export function dropChanceBps(weight: number): number {
  if (weight <= 0) return 0;
  return Math.min(DROP_MAX_BPS, DROP_BASE_BPS + Math.floor(weight / DROP_WEIGHT_PER_BPS));
}

export const dropChancePercent = (weight: number) => dropChanceBps(weight) / 100;

/** Items a pet of this weight can find, best last. Mirrors the contract. */
export function droppableItems(weight: number): CreditItemKey[] {
  const order: CreditItemKey[] = ['tonic', 'charm', 'lens', 'ward'];
  if (weight <= 0) return [];
  const tier = weight < 200 ? 0 : weight < 600 ? 1 : weight < 1_400 ? 2 : 3;
  return order.slice(0, tier + 1);
}

/** Actions that roll for a drop. */
export const DROPPING_ACTIONS: CreditAction[] = ['ai_feed', 'train', 'craft_item'];

/** The marketplace cut, in basis points. Leaves circulation into the reserve. */
export const MARKET_RAKE_BPS = 500;

// ── Mining ───────────────────────────────────────────────────────────────────

/** Credits one mining claim pays. Requires a staked pet. */
export const MINING_REWARD_CREDITS = 0.005;
export const MINING_COOLDOWN_SECONDS = 6 * 3600;

// ── Levelling ────────────────────────────────────────────────────────────────

/** XP needed to go from `level` to `level + 1`. */
export const xpForLevel = (level: number) => Math.round(100 * Math.pow(1.18, Math.max(0, level)));

/** XP granted by one feed, before the taste multiplier. */
export const BASE_FEED_XP = 35;

/** Final XP for one AI feed. `tasteMatch` is 0..1, as the model reports it. */
export function feedXp(tasteMatch: number, lensActive = false): number {
  const match = Math.min(1, Math.max(0, tasteMatch));
  const multiplier = 1 + match * 0.8 + (lensActive ? LENS_BONUS : 0);
  return Math.round(BASE_FEED_XP * multiplier);
}

/**
 * The most XP one feed can be worth: a perfect taste match with a Focus Lens.
 *
 * `pet-stake::award_feed_xp` clamps to the same number, and that is the one
 * that matters — the XP award is signed by a server key, and a server can be
 * compromised. The game contract's own clamp is 500, which is five levels in
 * five calls. Keep this in step with `MAX_FEED_XP` in
 * contracts/pet-stake/src/types.rs.
 */
export const MAX_FEED_XP = feedXp(1, true);

export function formatCredits(credits: number, maxFraction = 3): string {
  if (!Number.isFinite(credits)) return '0';
  return credits.toLocaleString('en-US', {
    minimumFractionDigits: 0,
    maximumFractionDigits: maxFraction
  });
}
