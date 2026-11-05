<script lang="ts">
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';
  import { formatCredits } from '#lib/economy/params';
  import { truncateAccount } from '#lib/near/amount';
  import { isActive, NAV_ITEMS } from '#lib/nav';
  import { wallet } from '#lib/near/wallet.svelte';
  import { liveCredits, player } from '#lib/stores/player.svelte';
  import ConnectWallet from './ConnectWallet.svelte';

  type Props = {
    /** Extra row under the bar, e.g. the starving countdown. */
    banner?: Snippet;
  };

  let { banner }: Props = $props();

  let now = $state(Date.now());

  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const credits = $derived(player.credits + liveCredits(now));
</script>

<header class="sticky top-0 z-30 w-full bg-[#2d3c53]">
  <div class="mx-auto flex w-full max-w-[1040px] flex-row items-center gap-3 px-3 py-2">
    <a href="/" class="shrink-0">
      <span class="text-sm text-white md:text-base">JOYGOTCHI</span>
    </a>

    <!-- Desktop navigation. The mobile dock carries the same links. -->
    <nav class="hidden flex-1 flex-row items-center gap-1 md:flex" aria-label="Primary">
      {#each NAV_ITEMS as item (item.href)}
        {@const active = isActive(item.href, page.url.pathname)}
        <a
          href={item.href}
          aria-current={active ? 'page' : undefined}
          class="rounded-lg px-2 py-1 text-white/80 hover:bg-[#a9c6e4]/30 hover:text-white {active
            ? 'bg-[#a9c6e4]/40 text-white'
            : ''}"
        >
          <small>{item.label}</small>
        </a>
      {/each}
    </nav>

    <div class="flex flex-1 flex-row items-center justify-end gap-3 md:flex-none">
      <div class="flex flex-row items-center gap-2">
        <img width="20" height="20" src="/assets/icon/near.svg" alt="NEAR" />
        <small class="text-white">{player.near.toLocaleString('en-US')}</small>
      </div>
      <div class="flex flex-row items-center gap-2">
        <img width="20" height="20" src="/assets/item/credit_card.png" alt="AI credit" />
        <small class="text-white">{formatCredits(credits)}</small>
      </div>

      {#if wallet.accountId}
        <a
          href="/mint?tab=2"
          class="rounded-full bg-[#a9c6e4] px-2 py-1"
          title={wallet.accountId}
        >
          <small class="text-white">{truncateAccount(wallet.accountId)}</small>
        </a>
      {:else}
        <ConnectWallet compact />
      {/if}
    </div>
  </div>

  {#if banner}
    <div class="mx-auto w-full max-w-[1040px] px-3 pb-2">
      {@render banner()}
    </div>
  {/if}
</header>
