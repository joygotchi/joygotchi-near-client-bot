import { error, json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { MINT_PRICE_WNEAR, MINT_ROUTER_CONTRACT, TREASURY_ACCOUNT } from '#lib/config';
import { findPayAsset, SETTLEMENT_ASSET } from '#lib/intents/assets';
import type {
  BuyCreditsMsg,
  MintRouterMsg,
  PurchaseQuoteRequest,
  PurchaseQuoteResponse
} from '#lib/intents/purchase';
import type { QuoteRequest } from '#lib/intents/types';
import { parseAmount, formatAmount } from '#lib/near/amount';
import { accountExists } from '#lib/near/rpc';
import { findToken, getQuote, hasPartnerKey, OneClickError } from '#lib/server/oneclick';

/** wNEAR has 24 decimals, like native NEAR. */
const WNEAR_DECIMALS = 24;

/** 1% — cross-chain routes for long-block chains like Bitcoin need some room. */
const SLIPPAGE_BPS = 100;

/** How long the deposit address stays valid. Bitcoin confirmations are slow. */
const DEADLINE_MINUTES = 45;

const ACCOUNT_RE = /^(([a-z\d]+[-_])*[a-z\d]+\.)*([a-z\d]+[-_])*[a-z\d]+$/;
const PET_CATEGORIES = ['blackdragon', 'greendragon'];

function badRequest(message: string): never {
  error(400, message);
}

/**
 * Builds the 1Click quote for a purchase. The client never chooses the
 * recipient or the amount — those come from server config, so a tampered
 * request cannot redirect funds or mint a pet for free.
 */
export const POST: RequestHandler = async ({ request, fetch }) => {
  const body = (await request.json().catch(() => null)) as PurchaseQuoteRequest | null;
  if (!body) badRequest('Expected a JSON body');

  const { purpose, originAsset, refundTo, accountId } = body;

  if (purpose !== 'mint_pet' && purpose !== 'buy_credits') {
    badRequest('purpose must be "mint_pet" or "buy_credits"');
  }

  const payAsset = findPayAsset(originAsset ?? '');
  if (!payAsset) badRequest('originAsset is not an accepted payment asset');

  if (!refundTo || refundTo.trim().length < 8) {
    badRequest(`refundTo must be a valid ${payAsset.chain} address`);
  }

  if (!accountId || !ACCOUNT_RE.test(accountId) || accountId.length > 64) {
    badRequest('accountId must be a valid NEAR account id');
  }

  const quantity = Math.trunc(body.quantity ?? 1);
  if (!Number.isInteger(quantity) || quantity < 1 || quantity > 10) {
    badRequest('quantity must be between 1 and 10');
  }

  const category = (body.petCategory ?? PET_CATEGORIES[0]).toLowerCase();
  if (purpose === 'mint_pet' && !PET_CATEGORIES.includes(category)) {
    badRequest(`petCategory must be one of: ${PET_CATEGORIES.join(', ')}`);
  }

  const name = (body.petName ?? 'PET').slice(0, 32);

  // Price is fixed in wNEAR, so we ask for an exact output. 1Click quotes the
  // input amount (plus a slippage buffer) and refunds whatever is left over.
  const priceWnear = (Number(MINT_PRICE_WNEAR) * quantity).toString();
  const amountOut = parseAmount(priceWnear, WNEAR_DECIMALS);

  const routerMsg = JSON.stringify(
    purpose === 'mint_pet'
      ? ({ action: 'mint_pet', account_id: accountId, category, name, quantity } satisfies MintRouterMsg)
      : ({ action: 'buy_credits', account_id: accountId } satisfies BuyCreditsMsg)
  );

  // The router is a contract, so settlement must go through `ft_transfer_call`.
  // Without a router configured we fall back to a plain transfer to the treasury
  // and reconcile the purchase off-chain.
  const useRouter = MINT_ROUTER_CONTRACT.length > 0;

  const quoteRequest: Omit<QuoteRequest, 'referral' | 'appFees'> = {
    dry: body.dry === true,
    swapType: 'EXACT_OUTPUT',
    slippageTolerance: SLIPPAGE_BPS,
    originAsset: payAsset.assetId,
    depositType: 'ORIGIN_CHAIN',
    depositMode: 'SIMPLE',
    destinationAsset: SETTLEMENT_ASSET,
    amount: amountOut,
    refundTo: refundTo.trim(),
    refundType: 'ORIGIN_CHAIN',
    recipient: useRouter ? MINT_ROUTER_CONTRACT : TREASURY_ACCOUNT,
    recipientType: 'DESTINATION_CHAIN',
    deadline: new Date(Date.now() + DEADLINE_MINUTES * 60_000).toISOString(),
    quoteWaitingTimeMs: 3000,
    ...(useRouter ? { customRecipientMsg: routerMsg } : {})
  };

  // 1Click validates the NEAR recipient and reports a missing account as an
  // opaque `400 Internal server error`. Say what is actually wrong instead.
  if (!(await accountExists(quoteRequest.recipient, fetch))) {
    error(
      503,
      `Payments are not set up yet: ${quoteRequest.recipient} does not exist on NEAR. ` +
        'Deploy the mint router (see contracts/README.md) and set PUBLIC_CONTRACT_MINT_ROUTER.'
    );
  }

  try {
    const [quote, originToken] = await Promise.all([
      getQuote(quoteRequest),
      findToken(payAsset.assetId)
    ]);

    const payload: PurchaseQuoteResponse = {
      purpose,
      quote,
      settlementAmount: formatAmount(amountOut, WNEAR_DECIMALS, 4),
      routerMsg,
      authenticated: hasPartnerKey()
    };

    return json({
      ...payload,
      originPriceUsd: originToken?.price ?? null
    });
  } catch (err) {
    if (err instanceof OneClickError) {
      return json({ error: err.message }, { status: err.status === 401 ? 502 : err.status });
    }
    return json({ error: (err as Error).message }, { status: 502 });
  }
};
