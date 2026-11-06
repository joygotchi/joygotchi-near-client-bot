/**
 * The auto-feed cron (issue #8).
 *
 * NEAR has no scheduler, so the "while you are away" half of auto-feed has to
 * be driven from outside the chain. This is that driver: it asks the staking
 * contract which accounts opted in, asks the game contract which of their pets
 * are close to starving, and calls `pet-stake::auto_feed` for those.
 *
 * It holds no authority of its own worth having. The budget, the daily window,
 * the balance and the pet's ownership are all checked on chain, so the worst a
 * bug here can do is waste gas on calls the contracts refuse. That is the whole
 * reason auto-feed is an oracle-gated contract method rather than a per-player
 * function-call key held by a worker — see the comment on
 * `pet-stake::auto_feed`.
 *
 * Wired to Vercel Cron in vercel.json. Vercel sends
 * `Authorization: Bearer $CRON_SECRET`, which is the only thing standing
 * between this route and anyone who can spend every budget on the deployment,
 * so it refuses outright when no secret is configured.
 */

import { json } from '@sveltejs/kit';
import { CRON_SECRET } from '$app/env/private';
import type { RequestHandler } from './$types';
import { GAME_CONTRACT, STAKE_CONTRACT } from '#lib/config';
import {
  planAutoFeeds,
  secondsUntilStarving,
  type AutoFeedAccount,
  type AutoFeedPet
} from '#lib/economy/auto-feed';
import { AUTO_FEED_WHEN_UNDER_SECONDS, SINKS } from '#lib/economy/params';
import { viewFunction } from '#lib/near/rpc';
import { autoFeedConfigured, autoFeedPet, type AutoFeedOutcome } from '#lib/server/auto-feed';

/** `pet-stake::get_auto_feed_accounts`. */
type AutoFeedAccountView = {
  account_id: string;
  budget: string;
  spent: string;
  available: string;
  credit_balance: string;
  window: string;
};

/** The fields of `game::get_pets_for_owner` this route uses. */
type PetView = {
  pet_id: number;
  owner_id: string;
  starves_at: string;
};

const PAGE = 100;
/** Safety rail on one run, so a cron cannot be turned into an unbounded job. */
const MAX_PAGES = 20;

/** Every account with a non-zero budget. A budget of 0 never appears. */
async function readOptedInAccounts(fetchImpl: typeof fetch): Promise<AutoFeedAccountView[]> {
  const all: AutoFeedAccountView[] = [];

  for (let page = 0; page < MAX_PAGES; page++) {
    const batch = await viewFunction<AutoFeedAccountView[]>(
      STAKE_CONTRACT,
      'get_auto_feed_accounts',
      { from_index: page * PAGE, limit: PAGE },
      fetchImpl
    );
    if (!batch?.length) break;
    all.push(...batch);
    if (batch.length < PAGE) break;
  }

  return all;
}

export const GET: RequestHandler = async ({ request, url, fetch }) => {
  // No secret configured means no cron, not an open cron.
  if (!CRON_SECRET) {
    return json({ error: 'The auto-feed cron is not configured (set CRON_SECRET)' }, { status: 503 });
  }
  if (request.headers.get('authorization') !== `Bearer ${CRON_SECRET}`) {
    return json({ error: 'Unauthorized' }, { status: 401 });
  }

  /**
   * `?dryRun=1` reads and plans but signs nothing.
   *
   * A cron is the one kind of endpoint nobody watches, and the only evidence a
   * real run leaves is credits already spent. This is how an operator asks
   * "what would this do right now, and when will it start doing it" without
   * spending a player's budget to find out. Still behind the secret: it reads
   * who has opted in.
   */
  const dryRun = url.searchParams.get('dryRun') === '1';
  /** Dry-run only, for seeing how close the next feed is. */
  const whenUnderSeconds = dryRun
    ? Number(url.searchParams.get('whenUnder') ?? '') || undefined
    : undefined;

  if (!dryRun && !autoFeedConfigured()) {
    return json(
      { error: 'No oracle key on this deployment (set NEAR_ORACLE_ACCOUNT_ID and NEAR_ORACLE_PRIVATE_KEY)' },
      { status: 503 }
    );
  }

  const started = Date.now();
  const price = BigInt(Math.round(SINKS.auto_feed.credits * 1e6));

  let optedIn: AutoFeedAccountView[];
  try {
    optedIn = await readOptedInAccounts(fetch);
  } catch (err) {
    return json(
      { error: `Could not read the opted-in accounts: ${err instanceof Error ? err.message : String(err)}` },
      { status: 502 }
    );
  }

  const accounts: AutoFeedAccount[] = optedIn.map((account) => ({
    accountId: account.account_id,
    available: BigInt(account.available)
  }));

  // One read per account rather than one `get_all_pet_metadata` for the whole
  // population: this is bounded by the number of players who opted in, which is
  // the number that matters, and it keeps a single unreadable account from
  // taking the whole run down with it.
  const pets: AutoFeedPet[] = [];
  const readErrors: Array<{ accountId: string; reason: string }> = [];

  for (const account of accounts) {
    try {
      const owned = await viewFunction<PetView[]>(
        GAME_CONTRACT,
        'get_pets_for_owner',
        { account_id: account.accountId },
        fetch
      );
      for (const pet of owned ?? []) {
        pets.push({
          petId: pet.pet_id,
          ownerId: pet.owner_id,
          starvesAt: BigInt(pet.starves_at)
        });
      }
    } catch (err) {
      readErrors.push({
        accountId: account.accountId,
        reason: err instanceof Error ? err.message : String(err)
      });
    }
  }

  const nowMs = Date.now();
  const plan = planAutoFeeds({ accounts, pets, nowMs, price, whenUnderSeconds });

  if (dryRun) {
    return json({
      dryRun: true,
      accounts: accounts.map((a) => ({ accountId: a.accountId, available: String(a.available) })),
      pets: pets.map((pet) => ({
        petId: pet.petId,
        ownerId: pet.ownerId,
        secondsLeft: Math.round(secondsUntilStarving(pet.starvesAt, nowMs))
      })),
      whenUnderSeconds: whenUnderSeconds ?? AUTO_FEED_WHEN_UNDER_SECONDS,
      price: String(price),
      planned: plan,
      oracleConfigured: autoFeedConfigured(),
      ms: Date.now() - started,
      readErrors
    });
  }

  // Sequential, not parallel. The signer tracks one nonce per access key, and
  // two calls signed in the same millisecond would collide on it — which costs
  // a retry each and, worse, makes the run's outcome depend on timing.
  const outcomes: AutoFeedOutcome[] = [];
  for (const task of plan) {
    outcomes.push(await autoFeedPet(task.accountId, task.petId));
  }

  const fed = outcomes.filter((o) => o.fed);
  const failed = outcomes.filter((o) => !o.fed);

  return json({
    accounts: accounts.length,
    petsRead: pets.length,
    planned: plan.length,
    fed: fed.length,
    failed: failed.length,
    ms: Date.now() - started,
    // Per-item detail, so a run that fed nothing says why rather than looking
    // like a run with nothing to do.
    results: outcomes,
    readErrors
  });
};
