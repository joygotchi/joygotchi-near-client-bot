<script lang="ts">
  import { onDestroy } from 'svelte';
  import { fetchPayTokens, requestQuote, watchSwap, type QuoteResult } from '#lib/intents/client';
  import { PAY_ASSETS, type PayAsset } from '#lib/intents/assets';
  import type { PurchasePurpose } from '#lib/intents/purchase';
  import type { ExecutionStatus, TokenResponse } from '#lib/intents/types';
  import { STATUS_LABEL, TERMINAL_STATUSES } from '#lib/intents/types';
  import { wallet } from '#lib/near/wallet.svelte';
  import { refreshPlayer } from '#lib/stores/player.svelte';
  import { toastError, toastSuccess } from '#lib/stores/toast.svelte';
  import CopyField from './CopyField.svelte';
  import QrCode from './QrCode.svelte';

  type Props = {
    purpose: PurchasePurpose;
    /** What the player gets, e.g. "1 BLACK DRAGON". */
    summary: string;
    quantity?: number;
    petCategory?: string;
    petName?: string;
    onsettled?: () => void;
  };

  let { purpose, summary, quantity = 1, petCategory, petName, onsettled }: Props = $props();

  let asset = $state<PayAsset>(PAY_ASSETS[0]);
  let refundTo = $state('');
  let tokens = $state<TokenResponse[]>([]);
  let quote = $state<QuoteResult | null>(null);
  let status = $state<ExecutionStatus | null>(null);
  let loading = $state(false);
  let stopWatching: (() => void) | null = null;

  const priceOf = (assetId: string) => tokens.find((token) => token.assetId === assetId)?.price;

  $effect(() => {
    fetchPayTokens()
      .then((list) => (tokens = list))
      .catch(() => {
        /* prices are a nice-to-have; the quote is authoritative */
      });
  });

  onDestroy(() => stopWatching?.());

  const deposit = $derived(quote?.quote.quote);
  const terminal = $derived(status ? TERMINAL_STATUSES.includes(status.status) : false);

  async function getQuote() {
    if (!wallet.accountId) {
      toastError('Connect a wallet first');
      return;
    }
    if (refundTo.trim().length < 8) {
      toastError(`Enter a ${asset.chain} address for refunds`);
      return;
    }

    loading = true;
    stopWatching?.();
    status = null;

    try {
      quote = await requestQuote({
        purpose,
        originAsset: asset.assetId,
        refundTo: refundTo.trim(),
        accountId: wallet.accountId,
        quantity,
        petCategory,
        petName
      });

      const address = quote.quote.quote.depositAddress;
      if (address) watch(address, quote.quote.quote.depositMemo);
    } catch (error) {
      quote = null;
      toastError(error instanceof Error ? error.message : String(error));
    } finally {
      loading = false;
    }
  }

  function watch(depositAddress: string, memo?: string) {
    stopWatching = watchSwap(depositAddress, memo, {
      onUpdate: async (next) => {
        const changed = status?.status !== next.status;
        status = next;
        if (!changed) return;

        if (next.status === 'SUCCESS') {
          toastSuccess('Payment settled on NEAR');
          await refreshPlayer(true);
          onsettled?.();
        } else if (next.status === 'REFUNDED') {
          toastError('Swap was refunded');
        } else if (next.status === 'FAILED') {
          toastError('Swap failed');
        }
      }
    });
  }

  function reset() {
    stopWatching?.();
    stopWatching = null;
    quote = null;
    status = null;
  }
</script>

<div class="flex flex-col gap-3">
  {#if !deposit}
    <div class="flex flex-col gap-2">
      <small class="text-[#00000075]">Pay with</small>
      <div class="grid grid-cols-4 gap-2">
        {#each PAY_ASSETS as option (option.assetId)}
          <button
            type="button"
            onclick={() => (asset = option)}
            class="flex flex-col items-center gap-1 rounded-lg border-2 p-2 {asset.assetId ===
            option.assetId
              ? 'border-[#304053] bg-[#a9c6e4]'
              : 'border-[#304053]/35'}"
          >
            <img width="22" height="22" src={option.icon} alt="" />
            <small class="text-[0.65rem] text-black">{option.symbol}</small>
            <small class="text-[0.55rem] text-[#00000075]">{option.chain}</small>
          </button>
        {/each}
      </div>
    </div>

    <label class="flex flex-col gap-1">
      <small class="text-[#00000075]">Refund address on {asset.chain}</small>
      <input
        type="text"
        bind:value={refundTo}
        placeholder={asset.addressHint}
        spellcheck="false"
        autocapitalize="none"
        class="rounded-lg border-2 border-gray-300 px-3 py-2 text-sm text-black outline-none focus:border-[#2d3c53]"
      />
      <small class="text-[0.6rem] text-[#00000075]">
        If the swap cannot complete, your {asset.symbol} goes back here.
      </small>
    </label>

    <div class="flex flex-row items-center justify-between rounded-lg bg-[#a9c6e4] px-3 py-2">
      <small class="text-black">You get</small>
      <small class="text-black">{summary}</small>
    </div>

    <button
      type="button"
      onclick={getQuote}
      disabled={loading}
      class="h-12 w-full rounded-lg bg-[#304053] px-3 disabled:opacity-60"
    >
      <span class="font-semibold text-white">
        {loading ? 'Getting quote…' : `Pay with ${asset.symbol}`}
      </span>
    </button>

    {#if priceOf(asset.assetId)}
      <small class="text-center text-[0.6rem] text-[#00000075]">
        1 {asset.symbol} ≈ ${priceOf(asset.assetId)?.toLocaleString('en-US')} · routed by NEAR
        Intents
      </small>
    {/if}
  {:else}
    <div class="flex flex-col items-center gap-3">
      <div class="text-center">
        <p class="text-black">
          Send exactly
          <span class="font-bold">{deposit.amountInFormatted} {asset.symbol}</span>
        </p>
        <small class="text-[#00000075]">on {asset.chain} · ≈ ${deposit.amountInUsd}</small>
      </div>

      <QrCode value={deposit.depositAddress ?? ''} />

      <CopyField label="Deposit address" value={deposit.depositAddress ?? ''} />

      {#if deposit.depositMemo}
        <CopyField label="Memo (required!)" value={deposit.depositMemo} />
      {/if}

      <div class="flex w-full flex-col gap-1 rounded-lg bg-[#a9c6e4] px-3 py-2">
        <div class="flex flex-row justify-between">
          <small class="text-black">You get</small>
          <small class="text-black">{quote?.settlementAmount} wNEAR → {summary}</small>
        </div>
        <div class="flex flex-row justify-between">
          <small class="text-black">Settles in</small>
          <small class="text-black">~{deposit.timeEstimate}s after confirmation</small>
        </div>
        {#if deposit.deadline}
          <div class="flex flex-row justify-between">
            <small class="text-black">Valid until</small>
            <small class="text-black">{new Date(deposit.deadline).toLocaleTimeString()}</small>
          </div>
        {/if}
      </div>

      <div class="flex w-full flex-row items-center justify-between">
        <small class="text-[#00000075]">
          {status ? STATUS_LABEL[status.status] : 'Waiting for your deposit'}
        </small>
        {#if !terminal}
          <img width="18" height="18" class="animate-spin" src="/assets/icon/loading.svg" alt="" />
        {/if}
      </div>

      {#if status?.swapDetails?.destinationChainTxHashes?.length}
        <a
          class="text-[0.65rem] text-[#628ab4] underline"
          href={status.swapDetails.destinationChainTxHashes[0].explorerUrl}
          target="_blank"
          rel="noopener noreferrer"
        >
          View the NEAR transaction
        </a>
      {/if}

      <button
        type="button"
        onclick={reset}
        class="h-10 w-full rounded-lg border-2 border-[#304053] text-[#304053]"
      >
        <small class="font-semibold">{terminal ? 'Pay again' : 'Cancel'}</small>
      </button>

      {#if !quote?.authenticated}
        <small class="text-center text-[0.6rem] text-[#00000075]">
          No 1Click partner key configured — this quote carries an extra 0.25% fee.
        </small>
      {/if}
    </div>
  {/if}
</div>
