import type { CreditAction } from '#lib/economy/params';
import type { PetContext } from './prompts';

export type AiFeedResult = {
  action: 'ai_feed';
  creditsSpent: number;
  meal: string;
  reaction: string;
  tasteMatch: number;
  xpMultiplier: number;
  xpGained: number;
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
