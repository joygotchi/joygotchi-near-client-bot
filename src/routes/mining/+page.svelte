<script lang="ts">
  import CountDownTimer from '#lib/components/CountDownTimer.svelte';
  import AppShell from '#lib/components/AppShell.svelte';
  import Modal from '#lib/components/Modal.svelte';
  import { MINING_COOLDOWN_SECONDS, MINING_REWARD_CREDITS } from '#lib/economy/params';
  import { claimMining } from '#lib/economy/stake';
  import { wallet } from '#lib/near/wallet.svelte';
  import { player, refreshPlayer } from '#lib/stores/player.svelte';
  import { toastError, withToast } from '#lib/stores/toast.svelte';
  import { haptic } from '#lib/platform';

  const STORAGE_KEY = 'joygotchi:lastMiningClaim';

  let lastClaim = $state(0);
  let now = $state(Date.now());
  let toolsOpen = $state(false);

  $effect(() => {
    lastClaim = Number(localStorage.getItem(STORAGE_KEY) ?? 0);
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const cooldownLeft = $derived(
    Math.max(0, MINING_COOLDOWN_SECONDS - Math.floor((now - lastClaim) / 1000))
  );

  /** Mining pays credits and needs a pet at work — that is what stops it being farmable. */
  const hasWorkingPet = $derived(player.stakedPetIds.length > 0);

  async function claim() {
    if (!wallet.accountId) {
      toastError('Connect a wallet first');
      return;
    }
    if (!hasWorkingPet) {
      toastError('Stake a pet in the AI Lab first');
      return;
    }
    haptic();

    const result = await withToast(
      'Mining…',
      claimMining,
      () => `+${MINING_REWARD_CREDITS} AI credit`
    );

    if (result) {
      lastClaim = Date.now();
      localStorage.setItem(STORAGE_KEY, String(lastClaim));
      await refreshPlayer(true);
    }
  }

  const slots = [0, 1, 2];
</script>

<AppShell title="JOYGOTCHI · Mining">
  <div class="mx-auto w-full max-w-[720px]">
    <!--
      The asset is a small icon, not a banner. Centring it on the panel keeps it
      sharp instead of stretching a 64px sprite across a 240px box.
    -->
    <div
      class="relative flex h-60 w-full items-center justify-center rounded-lg border-2 border-[#304053] bg-[#a9c6e4] shadow-sm"
    >
      <img class="h-24 w-24" src="/assets/tools/mining.png" alt="Mining rig" />
      <button
        type="button"
        disabled={cooldownLeft > 0 || !hasWorkingPet}
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

    <div class="mt-3 flex flex-row items-center justify-between rounded-lg bg-[#a9c6e4] px-3 py-2">
      <small class="text-black">Pays</small>
      <small class="text-black">
        {MINING_REWARD_CREDITS} AI credit · every {MINING_COOLDOWN_SECONDS / 3600}h
      </small>
    </div>

    {#if !hasWorkingPet}
      <a
        href="/stake"
        class="mt-2 flex flex-row items-center justify-between rounded-lg border-2 border-[#304053] px-3 py-2"
      >
        <small class="text-black">Mining needs a pet in the AI Lab</small>
        <img width="14" height="14" src="/assets/icon/arrow_right.png" alt="" />
      </a>
    {/if}

    <div class="mt-5 grid grid-cols-3 gap-3">
      {#each slots as slot (slot)}
        <button
          type="button"
          onclick={() => (toolsOpen = true)}
          class="relative flex h-44 flex-col items-center rounded-lg border-2 border-[#304053] bg-[#a9c6e4]/85"
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
</AppShell>

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
