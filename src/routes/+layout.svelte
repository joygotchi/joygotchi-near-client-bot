<script lang="ts">
  import type { Snippet } from 'svelte';
  import '../app.css';
  import Toasts from '#lib/components/Toasts.svelte';
  import { initWallet, wallet } from '#lib/near/wallet.svelte';
  import { refreshPlayer } from '#lib/stores/player.svelte';
  import { initTelegram } from '#lib/telegram';

  let { children }: { children: Snippet } = $props();

  $effect(() => {
    initTelegram();
    void initWallet();
  });

  // Reload balances and pets whenever the connected account changes.
  $effect(() => {
    const accountId = wallet.accountId;
    if (!wallet.ready) return;
    void refreshPlayer(true);
    void accountId;
  });

  // Keep pet state fresh — pets starve on a timer.
  $effect(() => {
    const timer = setInterval(() => void refreshPlayer(), 60_000);
    return () => clearInterval(timer);
  });
</script>

<Toasts />

<div class="flex min-h-screen w-full flex-col items-center justify-center bg-[#b8e3f8]">
  {@render children()}
</div>
