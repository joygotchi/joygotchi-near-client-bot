import { error, json } from '@sveltejs/kit';
import { NEAR_AI_API_KEY, NEAR_AI_BASE_URL, NEAR_AI_MODEL } from '$app/env/private';
import type { RequestHandler } from './$types';
import { buildTask, extractJson, type PetContext } from '#lib/ai/prompts';
import { BASE_FEED_XP, SINKS, type CreditAction } from '#lib/economy/params';
import { ReceiptError, verifySpendReceipt } from '#lib/server/verify-tx';

const AI_ACTIONS: CreditAction[] = ['ai_feed', 'train', 'craft_item', 'chat'];

type Body = {
  accountId?: string;
  /** Hash of the `spend_credits` transaction that paid for this call. */
  txHash?: string;
  action?: CreditAction;
  pet?: PetContext;
  message?: string;
};

type Completion = {
  choices?: Array<{ message?: { content?: string } }>;
  usage?: { total_tokens?: number };
};

export const POST: RequestHandler = async ({ request }) => {
  if (!NEAR_AI_API_KEY) {
    error(503, 'NEAR AI is not configured on this deployment (set NEAR_AI_API_KEY)');
  }

  const body = (await request.json().catch(() => null)) as Body | null;
  if (!body?.accountId || !body.txHash || !body.action || !body.pet) {
    error(400, 'accountId, txHash, action and pet are required');
  }
  if (!AI_ACTIONS.includes(body.action)) {
    error(400, `action must be one of: ${AI_ACTIONS.join(', ')}`);
  }

  // No inference happens until we have seen the on-chain payment.
  try {
    await verifySpendReceipt({
      txHash: body.txHash,
      accountId: body.accountId,
      action: body.action
    });
  } catch (err) {
    if (err instanceof ReceiptError) error(402, err.message);
    throw err;
  }

  const task = buildTask(body.action, body.pet, body.message);

  const response = await fetch(`${NEAR_AI_BASE_URL.replace(/\/+$/, '')}/chat/completions`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      authorization: `Bearer ${NEAR_AI_API_KEY}`
    },
    body: JSON.stringify({
      model: NEAR_AI_MODEL,
      max_tokens: task.maxTokens,
      temperature: 0.9,
      messages: [
        { role: 'system', content: task.system },
        { role: 'user', content: task.user }
      ]
    })
  });

  if (!response.ok) {
    const detail = await response.text();
    error(502, `NEAR AI request failed (HTTP ${response.status}): ${detail.slice(0, 300)}`);
  }

  const completion = (await response.json()) as Completion;
  const content = completion.choices?.[0]?.message?.content?.trim() ?? '';

  const result: Record<string, unknown> = {
    action: body.action,
    creditsSpent: SINKS[body.action].credits,
    model: NEAR_AI_MODEL,
    tokens: completion.usage?.total_tokens ?? null
  };

  if (!task.json) {
    return json({ ...result, reaction: content });
  }

  const parsed = extractJson<Record<string, unknown>>(content);
  if (!parsed) {
    // The credits are already spent, so degrade gracefully instead of failing.
    return json({ ...result, reaction: content || 'Your pet looks confused.', raw: content });
  }

  if (body.action === 'ai_feed') {
    const match = Math.min(1, Math.max(0, Number(parsed.taste_match) || 0));
    // 1.0x for a miss, up to 1.8x for a perfect match.
    const multiplier = 1 + match * 0.8;
    return json({
      ...result,
      meal: String(parsed.meal ?? 'mystery stew'),
      reaction: String(parsed.reaction ?? ''),
      tasteMatch: match,
      xpMultiplier: Number(multiplier.toFixed(2)),
      xpGained: Math.round(BASE_FEED_XP * multiplier)
    });
  }

  return json({ ...result, ...parsed });
};
