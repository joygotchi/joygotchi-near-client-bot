import { error, json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { CREDIT_DECIMALS, EPOCH_SECONDS, SINKS } from '#lib/economy/params';
import { getStakeAccount, getStakePool } from '#lib/economy/stake';
import { formatAmount } from '#lib/near/amount';

/**
 * Credit balance and accrual rate for one account. The browser could read these
 * views itself; going through the server keeps the numbers identical between SSR
 * and hydration, and gives one place to add caching later.
 */
export const GET: RequestHandler = async ({ url, fetch, setHeaders }) => {
  const accountId = url.searchParams.get('accountId');
  if (!accountId) error(400, 'accountId is required');

  const [account, pool] = await Promise.all([
    getStakeAccount(accountId, fetch),
    getStakePool(fetch)
  ]);

  const weight = Number(account.weight || '0');
  const totalWeight = Number(pool?.total_weight || '0');
  const epochBudget = Number(pool?.epoch_budget || '0') / 10 ** CREDIT_DECIMALS;

  const perSecond =
    totalWeight > 0 && weight > 0 ? (epochBudget / EPOCH_SECONDS) * (weight / totalWeight) : 0;

  setHeaders({ 'cache-control': 'no-store' });

  return json({
    accountId,
    stakedPets: account.staked_pets,
    weight,
    totalWeight,
    balance: Number(formatAmount(account.credit_balance, CREDIT_DECIMALS)),
    pending: Number(formatAmount(account.pending_credits, CREDIT_DECIMALS)),
    autoFeedBudget: Number(formatAmount(account.auto_feed_budget, CREDIT_DECIMALS)),
    autoFeedSpent: Number(formatAmount(account.auto_feed_spent ?? '0', CREDIT_DECIMALS)),
    creditsPerSecond: perSecond,
    creditsPerDay: perSecond * 86_400,
    epochBudget,
    sinks: SINKS
  });
};
