import { error, json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { getExecutionStatus, OneClickError } from '#lib/server/oneclick';

export const GET: RequestHandler = async ({ url }) => {
  const depositAddress = url.searchParams.get('depositAddress');
  if (!depositAddress) error(400, 'depositAddress is required');

  const depositMemo = url.searchParams.get('depositMemo') ?? undefined;

  try {
    return json(await getExecutionStatus(depositAddress, depositMemo));
  } catch (err) {
    if (err instanceof OneClickError) {
      return json({ error: err.message }, { status: err.status });
    }
    return json({ error: (err as Error).message }, { status: 502 });
  }
};
