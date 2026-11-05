<script lang="ts">
  import { connectWallet, FEATURED_WALLETS, wallet } from '#lib/near/wallet.svelte';
  import { refreshPlayer } from '#lib/stores/player.svelte';
  import { haptic } from '#lib/telegram';

  type Props = { compact?: boolean };
  let { compact = false }: Props = $props();

  async function connect(walletId?: string) {
    haptic();
    const accountId = await connectWallet(walletId);
    if (accountId) await refreshPlayer(true);
  }
</script>

{#if compact}
  <button
    type="button"
    class="h-8 rounded-full bg-[#a9c6e4] px-2 text-white disabled:opacity-60"
    disabled={wallet.connecting}
    onclick={() => connect()}
  >
    <small class="font-semibold">{wallet.connecting ? '...' : 'Connect'}</small>
  </button>
{:else}
  <div class="flex w-full flex-col items-center gap-3">
    <p class="text-center text-sm text-[#2d3c53]">Connect a NEAR wallet to start</p>

    {#each FEATURED_WALLETS as featured (featured.id)}
      <button
        type="button"
        class="flex h-12 w-64 flex-row items-center justify-center gap-2 rounded-lg bg-[#304053] px-3 hover:bg-[#304053]/90 disabled:opacity-60"
        disabled={wallet.connecting}
        onclick={() => connect(featured.id)}
      >
        <img width="20" height="20" src={featured.icon} alt="" />
        <span class="text-white">{featured.label}</span>
      </button>
    {/each}

    <button
      type="button"
      class="h-10 w-64 rounded-lg border-2 border-[#304053] px-3 text-[#304053] disabled:opacity-60"
      disabled={wallet.connecting}
      onclick={() => connect()}
    >
      <small class="font-semibold">All wallets…</small>
    </button>

    {#if wallet.error}
      <small class="max-w-64 text-center text-[#ff0000]">{wallet.error}</small>
    {/if}
  </div>
{/if}
