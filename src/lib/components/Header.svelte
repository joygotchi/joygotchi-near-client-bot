<script lang="ts">
  import { formatCredits } from '#lib/economy/params';
  import { truncateAccount } from '#lib/near/amount';
  import { wallet } from '#lib/near/wallet.svelte';
  import { liveCredits, player } from '#lib/stores/player.svelte';
  import ConnectWallet from './ConnectWallet.svelte';

  type Props = {
    /** Extra row shown under the balances, e.g. the starving countdown. */
    children?: import('svelte').Snippet;
    tall?: boolean;
  };

  let { children, tall = false }: Props = $props();

  let now = $state(Date.now());

  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const credits = $derived(player.credits + liveCredits(now));
</script>

<div class="fix-header sticky top-0 z-20 w-full md:rounded-t-lg">
  <div
    class="relative w-full border-b border-gray-300 bg-[#2d3c53] {tall ? 'h-20' : 'h-16'}"
  >
    <div class="flex flex-row items-center justify-between px-2 py-2 pt-3">
      <div class="flex flex-col gap-1">
        <div class="flex flex-row items-center gap-2">
          <img width="22" height="22" src="/assets/item/coin.png" alt="JOY" />
          <p class="text-sm text-white">{player.joy.toLocaleString('en-US')}</p>
        </div>
        <div class="flex flex-row items-center gap-2">
          <img width="22" height="22" src="/assets/item/credit_card.png" alt="AI credit" />
          <p class="text-sm text-white">{formatCredits(credits)}</p>
        </div>
      </div>

      <div class="flex flex-row items-center gap-4">
        {#if wallet.accountId}
          <a
            href="/mint?tab=2"
            class="h-8 cursor-pointer rounded-full bg-[#a9c6e4] px-2 py-0.5 leading-7"
          >
            <small class="text-white">{truncateAccount(wallet.accountId)}</small>
          </a>
        {:else}
          <ConnectWallet compact />
        {/if}
      </div>
    </div>

    {#if children}
      {@render children()}
    {/if}
  </div>
</div>
