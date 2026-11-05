<script lang="ts">
  import { NEAR_PER_CREDIT, formatCredits } from '#lib/economy/params';
  import { buyCreditsWithNear } from '#lib/economy/stake';
  import { wallet } from '#lib/near/wallet.svelte';
  import { player, refreshPlayer } from '#lib/stores/player.svelte';
  import { withToast } from '#lib/stores/toast.svelte';
  import ConnectWallet from './ConnectWallet.svelte';
  import CrossChainPay from './CrossChainPay.svelte';

  let nearToSpend = $state(1);

  const creditsOut = $derived(nearToSpend / NEAR_PER_CREDIT);
  const tooMuch = $derived(nearToSpend > Math.max(0, player.near - 0.05));

  async function convert() {
    if (!wallet.accountId) return;
    await withToast(
      'Converting…',
      () => buyCreditsWithNear(nearToSpend, wallet.accountId as string),
      () => `+${formatCredits(creditsOut)} AI credit`
    );
    await refreshPlayer(true);
  }
</script>

<div class="flex h-full w-full flex-col gap-4">
  {#if !wallet.accountId}
    <div class="rounded-lg border-2 border-[#304053] p-4">
      <ConnectWallet />
    </div>
  {:else}
    <!-- Money in from any chain -->
    <section class="flex flex-col gap-3 rounded-lg border-2 border-[#304053] p-3">
      <div class="flex flex-row items-center justify-between">
        <span class="text-black">Buy credits with any coin</span>
        <small class="text-[#00000075]">via NEAR Intents</small>
      </div>
      <CrossChainPay
        purpose="buy_credits"
        summary="AI credits"
        onsettled={() => refreshPlayer(true)}
      />
    </section>

    <!-- NEAR you already hold -->
    <section class="flex flex-col gap-3 rounded-lg border-2 border-[#304053] p-3">
      <div class="flex flex-row items-center justify-between">
        <span class="text-black">NEAR → AI credit</span>
        <small class="text-[#00000075]">{NEAR_PER_CREDIT} NEAR = 1 credit</small>
      </div>

      <label class="flex flex-col gap-1">
        <small class="text-[#00000075]">
          Spend (you have {player.near.toLocaleString('en-US')} NEAR)
        </small>
        <input
          type="number"
          min="0.1"
          step="0.1"
          bind:value={nearToSpend}
          class="rounded-lg border-2 border-gray-300 px-3 py-2 text-black outline-none focus:border-[#2d3c53]"
        />
      </label>

      <div class="flex flex-row items-center justify-between rounded-lg bg-[#a9c6e4] px-3 py-2">
        <small class="text-black">You receive</small>
        <small class="text-black">{formatCredits(creditsOut)} AI credit</small>
      </div>

      <button
        type="button"
        onclick={convert}
        disabled={nearToSpend < NEAR_PER_CREDIT || tooMuch}
        class="h-12 w-full rounded-lg bg-[#304053] disabled:opacity-60"
      >
        <span class="font-semibold text-white">CONVERT</span>
      </button>

      <small class="text-[0.6rem] text-[#00000075]">
        Two transactions in one approval: your NEAR is wrapped, then the mint router grants the
        credits. One-way on purpose — credits are real NEAR AI compute, so they never convert back
        into anything tradeable. See docs/ECONOMY.md.
      </small>
    </section>
  {/if}
</div>
