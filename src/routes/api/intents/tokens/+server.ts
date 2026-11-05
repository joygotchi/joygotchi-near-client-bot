import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { PAY_ASSETS, SETTLEMENT_ASSET } from '#lib/intents/assets';
import { getTokens, OneClickError } from '#lib/server/oneclick';

/**
 * Live prices and decimals for the assets JOYGOTCHI accepts. The browser never
 * talks to 1Click directly, so the partner JWT stays on the server.
 */
export const GET: RequestHandler = async ({ setHeaders }) => {
  const wanted = new Set([...PAY_ASSETS.map((asset) => asset.assetId), SETTLEMENT_ASSET]);

  try {
    const tokens = await getTokens();
    setHeaders({ 'cache-control': 'public, max-age=20' });
    return json({ tokens: tokens.filter((token) => wanted.has(token.assetId)) });
  } catch (error) {
    const status = error instanceof OneClickError ? error.status : 502;
    return json({ error: (error as Error).message, tokens: [] }, { status });
  }
};
