import type { CreditAction } from '#lib/economy/params';
import type { PetContext } from './prompts';

export type AiFeedResult = {
  action: 'ai_feed';
  creditsSpent: number;
  meal: string;
  reaction: string;
  tasteMatch: number;
  /** True when a Focus Lens was consumed, as reported by the contract. */
  lensApplied: boolean;
  xpMultiplier: number;
  xpGained: number;
  /**
   * Whether the XP was actually written to the pet on chain.
   *
   * False is a real outcome, not an error: the credits are spent and the
   * inference has run by the time the award is attempted, so the server
   * reports a failed award instead of failing the whole request. Before the
   * oracle key existed this was always false and nothing said so — a feed
   * showed "+63 XP" and the pet stayed at `xp: 0`.
   */
  xpAwarded: boolean;
  /** The `award_feed_xp` transaction, when it landed. */
  xpTxHash: string | null;
  /** Why it did not land, when it did not. */
  xpError: string | null;
  tokens: number | null;
};

export type AiTrainResult = {
  action: 'train';
  creditsSpent: number;
  drill: string;
  stat: 'atk' | 'def';
  gain: number;
  reaction: string;
};

export type AiCraftResult = {
  action: 'craft_item';
  creditsSpent: number;
  item: string;
  rarity: 'common' | 'rare' | 'epic';
  atk: number;
  def: number;
  reaction: string;
};

export type AiChatResult = {
  action: 'chat';
  creditsSpent: number;
  reaction: string;
};

export type AiResult = AiFeedResult | AiTrainResult | AiCraftResult | AiChatResult;

/**
 * One line about a finished feed, for the toast.
 *
 * Claiming XP the chain never got is the specific thing this exists to stop:
 * the player then sees a level that never arrives and no reason why.
 */
export function feedSummary(result: AiFeedResult): string {
  if (!result.xpAwarded) {
    const reason = (result.xpError ?? 'the award did not go through').trim();
    const short = reason.length > 80 ? `${reason.slice(0, 79)}…` : reason;
    return `No XP written on chain — ${short}`;
  }
  const lens = result.lensApplied ? ', lens' : '';
  return `+${result.xpGained} XP (${result.xpMultiplier}x${lens})`;
}

/**
 * Run an AI action. `txHash` must be the `spend_credits` transaction that paid
 * for it — the server refuses to run inference without a valid, unused receipt.
 */
export async function runAiAction<T extends AiResult = AiResult>(input: {
  accountId: string;
  txHash: string;
  action: CreditAction;
  pet: PetContext;
  message?: string;
}): Promise<T> {
  const response = await fetch('/api/ai/chat', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input)
  });

  const body = (await response.json().catch(() => null)) as
    | (T & { error?: string; message?: string })
    | null;

  if (!response.ok) {
    throw new Error(
      body?.message ?? body?.error ?? `AI request failed with HTTP ${response.status}`
    );
  }
  return body as T;
}
