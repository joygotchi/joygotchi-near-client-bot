<script lang="ts">
  import { JOY_PER_CREDIT, formatCredits } from '#lib/economy/params';
  import { buyCreditsWithJoy } from '#lib/economy/stake';
  import { wallet } from '#lib/near/wallet.svelte';
  import { player, refreshPlayer } from '#lib/stores/player.svelte';
  import { withToast } from '#lib/stores/toast.svelte';
  import ConnectWallet from './ConnectWallet.svelte';
  import CrossChainPay from './CrossChainPay.svelte';

  let joyToSpend = $state(JOY_PER_CREDIT);

  const creditsOut = $derived(joyToSpend / JOY_PER_CREDIT);

  async function convert() {
    await withToast(
      'Converting…',
      () => buyCreditsWithJoy(joyToSpend),
      () => `+${formatCredits(creditsOut)} AI credit`
    );
    await refreshPlayer(true);
  }
</script>

<div class="flex h-full w-full flex-col gap-4 pb-20">
  {#if !wallet.accountId}
    <div class="rounded-lg border-2 border-[#304053] p-4">
      <ConnectWallet />
    </div>
  {:else}
    <!-- Money in: any chain -> wNEAR -> $JOY -->
    <section class="flex flex-col gap-3 rounded-lg border-2 border-[#304053] p-3">
      <div class="flex flex-row items-center justify-between">
        <span class="text-black">Top up $JOY</span>
        <small class="text-[#00000075]">via NEAR Intents</small>
      </div>
      <CrossChainPay purpose="buy_joy" summary="a $JOY pack" onsettled={() => refreshPlayer(true)} />
    </section>

    <!-- $JOY -> credits, one way -->
    <section class="flex flex-col gap-3 rounded-lg border-2 border-[#304053] p-3">
      <div class="flex flex-row items-center justify-between">
        <span class="text-black">$JOY → AI credit</span>
        <small class="text-[#00000075]">{JOY_PER_CREDIT} JOY = 1 credit</small>
      </div>

      <label class="flex flex-col gap-1">
        <small class="text-[#00000075]">Spend (you have {player.joy.toLocaleString('en-US')})</small>
        <input
          type="number"
          min={JOY_PER_CREDIT}
          step={JOY_PER_CREDIT}
          bind:value={joyToSpend}
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
        disabled={joyToSpend < JOY_PER_CREDIT || joyToSpend > player.joy}
        class="h-12 w-full rounded-lg bg-[#304053] disabled:opacity-60"
      >
        <span class="font-semibold text-white">CONVERT</span>
      </button>

      <small class="text-[0.6rem] text-[#00000075]">
        One-way on purpose. Credits are real NEAR AI compute, so they never convert back into a
        tradeable token — see docs/ECONOMY.md.
      </small>
    </section>
  {/if}
  <div class="swap"></div>
</div>
