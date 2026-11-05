import { error, json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { OneClickError, submitDepositTx } from '#lib/server/oneclick';

/**
 * Optional but worth calling: handing 1Click the deposit transaction hash makes
 * it notice the deposit immediately instead of waiting for its own indexer.
 */
export const POST: RequestHandler = async ({ request }) => {
  const body = (await request.json().catch(() => null)) as {
    depositAddress?: string;
    txHash?: string;
    nearSenderAccount?: string;
    memo?: string;
  } | null;

  if (!body?.depositAddress || !body.txHash) {
    error(400, 'depositAddress and txHash are required');
  }

  try {
    return json(
      await submitDepositTx({
        depositAddress: body.depositAddress,
        txHash: body.txHash,
        nearSenderAccount: body.nearSenderAccount,
        memo: body.memo
      })
    );
  } catch (err) {
    if (err instanceof OneClickError) {
      return json({ error: err.message }, { status: err.status });
    }
    return json({ error: (err as Error).message }, { status: 502 });
  }
};
