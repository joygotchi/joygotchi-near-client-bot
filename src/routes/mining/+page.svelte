<script lang="ts">
  import CountDownTimer from '#lib/components/CountDownTimer.svelte';
  import Footer from '#lib/components/Footer.svelte';
  import Header from '#lib/components/Header.svelte';
  import Modal from '#lib/components/Modal.svelte';
  import { claimJoy } from '#lib/game/pets';
  import { wallet } from '#lib/near/wallet.svelte';
  import { refreshPlayer } from '#lib/stores/player.svelte';
  import { toastError, withToast } from '#lib/stores/toast.svelte';
  import { haptic } from '#lib/telegram';

  const COOLDOWN_SECONDS = 60;
  const STORAGE_KEY = 'joygotchi:lastClaim';

  let lastClaim = $state(0);
  let now = $state(Date.now());
  let toolsOpen = $state(false);

  $effect(() => {
    lastClaim = Number(localStorage.getItem(STORAGE_KEY) ?? 0);
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const cooldownLeft = $derived(
    Math.max(0, COOLDOWN_SECONDS - Math.floor((now - lastClaim) / 1000))
  );

  async function claim() {
    if (!wallet.accountId) {
      toastError('Connect a wallet first');
      return;
    }
    haptic();

    const result = await withToast(
      'Claiming…',
      () => claimJoy(wallet.accountId as string),
      () => 'Claim successful!'
    );

    if (result) {
      lastClaim = Date.now();
      localStorage.setItem(STORAGE_KEY, String(lastClaim));
      await refreshPlayer(true);
    }
  }

  const slots = [0, 1, 2];
</script>

<svelte:head>
  <title>JOYGOTCHI · Mining</title>
</svelte:head>

<Header />

<div class="screen h-full w-full overflow-y-auto bg-[#e5f2f8] md:w-[400px]">
  <div class="mt-2 px-2">
    <div class="relative h-60 w-full rounded-lg border-2 border-[#304053] shadow-sm">
      <img
        class="h-60 w-full rounded-lg object-cover"
        src="/assets/tools/mining.png"
        alt="Mining rig"
      />
      <button
        type="button"
        disabled={cooldownLeft > 0}
        onclick={claim}
        class="absolute bottom-2 left-1/2 flex w-2/4 -translate-x-1/2 flex-row items-center justify-center gap-2 rounded-lg border border-white bg-[#2d3c53] py-3 font-semibold text-white disabled:opacity-70"
      >
        {#if cooldownLeft > 0}
          <CountDownTimer seconds={cooldownLeft} />
        {:else}
          <img width="23" height="23" src="/assets/tools/mining.png" alt="" />
          <span class="text-2xl">Claim</span>
        {/if}
      </button>
    </div>

    <div class="mt-5 flex flex-row justify-between gap-3 pb-10">
      {#each slots as slot (slot)}
        <button
          type="button"
          onclick={() => (toolsOpen = true)}
          class="relative flex h-44 w-36 flex-col items-center rounded-lg border-2 border-[#304053] bg-[#a9c6e4]/85"
        >
          <div class="absolute -top-1 left-1 text-white">
            <small class="text-[0.75rem]">Tool</small>
          </div>
          <div class="mt-7 flex h-16 w-16 items-center justify-center rounded-lg bg-white">
            <img width="40" height="40" src="/assets/icon/add-black.svg" alt="Add tool" />
          </div>
          <div class="mt-5 flex flex-col gap-1 px-2">
            <div class="flex flex-row gap-4">
              <small class="text-black">Lucky:</small>
              <small>10%</small>
            </div>
            <div class="flex flex-row gap-4">
              <small class="text-black">Power:</small>
              <small>10%</small>
            </div>
          </div>
        </button>
      {/each}
    </div>
  </div>
  <div class="clear"></div>
</div>

<Footer />

<Modal title="Tool slot" open={toolsOpen} onclose={() => (toolsOpen = false)}>
  <div class="flex flex-col gap-3">
    <p class="text-sm">Tools are crafted in the AI Lab and raise your mining yield.</p>
    <a
      href="/stake"
      class="flex h-12 items-center justify-center rounded-lg bg-[#2d3c53] text-white"
    >
      <span>Go to the AI Lab</span>
    </a>
  </div>
</Modal>
