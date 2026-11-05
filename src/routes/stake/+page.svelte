<script lang="ts">
  import ConnectWallet from '#lib/components/ConnectWallet.svelte';
  import Footer from '#lib/components/Footer.svelte';
  import Header from '#lib/components/Header.svelte';
  import {
    FAUCET_DAILY_CREDITS,
    SINKS,
    formatCredits,
    petWeight,
    type Rarity
  } from '#lib/economy/params';
  import {
    claimCredits,
    claimFaucetCredits,
    setAutoFeedBudget,
    stakePet,
    unstakePet
  } from '#lib/economy/stake';
  import { petImage, type Pet } from '#lib/game/pets';
  import { wallet } from '#lib/near/wallet.svelte';
  import { creditsPerDay, isStaked, liveCredits, player, refreshPlayer } from '#lib/stores/player.svelte';
  import { withToast } from '#lib/stores/toast.svelte';
  import { haptic } from '#lib/telegram';

  let now = $state(Date.now());
  let autoFeed = $state(0);
  let busyPetId = $state<number | null>(null);

  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    autoFeed = player.autoFeedBudget;
  });

  const pending = $derived(liveCredits(now));
  const share = $derived(
    player.totalWeight > 0 ? (player.weight / player.totalWeight) * 100 : 0
  );

  /** Rarity is not on-chain yet; derive a stand-in from stars so weights line up. */
  const rarityOf = (pet: Pet): Rarity =>
    pet.star >= 9 ? 'legendary' : pet.star >= 6 ? 'epic' : pet.star >= 3 ? 'rare' : 'common';

  const weightOf = (pet: Pet) =>
    petWeight({
      rarity: rarityOf(pet),
      level: pet.level,
      star: pet.star,
      evolutionPhase: pet.pet_evolution_phase
    });

  async function toggleStake(pet: Pet) {
    haptic();
    busyPetId = pet.pet_id;
    const staked = isStaked(pet.pet_id);
    await withToast(
      staked ? 'Unstaking…' : 'Staking…',
      () => (staked ? unstakePet(pet.pet_id) : stakePet(pet.pet_id)),
      () => (staked ? `${pet.name} is back home` : `${pet.name} clocked in`)
    );
    busyPetId = null;
    await refreshPlayer(true);
  }

  async function claim() {
    haptic();
    await withToast('Claiming…', claimCredits, () => 'Credits claimed');
    await refreshPlayer(true);
  }

  async function faucet() {
    await withToast('Claiming…', claimFaucetCredits, () => `+${FAUCET_DAILY_CREDITS} credit`);
    await refreshPlayer(true);
  }

  async function saveAutoFeed() {
    await withToast(
      'Saving…',
      () => setAutoFeedBudget(autoFeed),
      () => (autoFeed > 0 ? 'Auto-feed is on' : 'Auto-feed is off')
    );
    await refreshPlayer(true);
  }
</script>

<svelte:head>
  <title>JOYGOTCHI · AI Lab</title>
</svelte:head>

<Header />

<div class="screen relative h-full w-full overflow-y-auto bg-[#e5f2f8] md:w-[400px]">
  {#if !wallet.accountId}
    <div class="flex h-[60vh] items-center justify-center px-6">
      <ConnectWallet />
    </div>
  {:else}
    <div class="flex flex-col gap-3 px-2 pt-3">
      <!-- Credit stream -->
      <section class="rounded-lg border-2 border-[#304053] bg-[#2d3c53] p-3">
        <div class="flex flex-row items-start justify-between">
          <div class="flex flex-col">
            <small class="text-[0.6rem] text-white/70">UNCLAIMED AI CREDIT</small>
            <p class="text-3xl text-white">{formatCredits(pending, 4)}</p>
            <small class="text-[0.6rem] text-white/70">
              +{formatCredits(creditsPerDay(), 3)} / day · {share.toFixed(2)}% of the pool
            </small>
          </div>
          <img width="34" height="34" src="/assets/item/credit_card.png" alt="" />
        </div>

        <div class="mt-3 flex flex-row gap-2">
          <button
            type="button"
            onclick={claim}
            disabled={pending <= 0}
            class="h-10 flex-1 rounded-lg bg-[#f48f59] disabled:opacity-50"
          >
            <small class="font-semibold text-white">CLAIM</small>
          </button>
          <button
            type="button"
            onclick={faucet}
            class="h-10 flex-1 rounded-lg border-2 border-[#a9c6e4]"
          >
            <small class="font-semibold text-[#a9c6e4]">DAILY FREE</small>
          </button>
        </div>

        <div class="mt-3 flex flex-row justify-between">
          <small class="text-[0.6rem] text-white/70">Spendable</small>
          <small class="text-[0.6rem] text-white">{formatCredits(player.credits, 4)}</small>
        </div>
      </section>

      <!-- Pool -->
      <section class="flex flex-row justify-between rounded-lg bg-[#a9c6e4] px-3 py-3">
        <div class="flex flex-col text-center">
          <p class="text-lg">{player.weight.toLocaleString('en-US')}</p>
          <small class="text-[0.6rem] text-[#00000088]">YOUR WEIGHT</small>
        </div>
        <div class="flex flex-col text-center">
          <p class="text-lg">{player.totalWeight.toLocaleString('en-US')}</p>
          <small class="text-[0.6rem] text-[#00000088]">POOL WEIGHT</small>
        </div>
        <div class="flex flex-col text-center">
          <p class="text-lg">{player.stakedPetIds.length}/{player.pets.length}</p>
          <small class="text-[0.6rem] text-[#00000088]">PETS AT WORK</small>
        </div>
      </section>

      <!-- Pets -->
      <section class="flex flex-col gap-2">
        <div class="flex flex-row items-center justify-between">
          <span class="text-black">Your pets</span>
          <small class="text-[0.6rem] text-[#00000088]">Staked pets cannot battle</small>
        </div>

        {#each player.pets as pet (pet.pet_id)}
          {@const staked = isStaked(pet.pet_id)}
          <div
            class="flex flex-row items-center justify-between rounded-lg border-2 px-2 py-2 {staked
              ? 'border-[#f48f59] bg-[#f48f59]/10'
              : 'border-[#304053]'}"
          >
            <div class="flex flex-row items-center gap-2">
              <img width="40" height="40" src={petImage(pet)} alt="" />
              <div class="flex flex-col">
                <small class="text-black">{pet.name}</small>
                <small class="text-[0.6rem] text-[#00000088]">
                  LV {pet.level} · {pet.star}★ · weight {weightOf(pet)}
                </small>
              </div>
            </div>
            <button
              type="button"
              onclick={() => toggleStake(pet)}
              disabled={busyPetId === pet.pet_id}
              class="h-8 rounded-lg px-3 disabled:opacity-50 {staked
                ? 'border-2 border-[#304053]'
                : 'bg-[#304053]'}"
            >
              <small class={staked ? 'text-[#304053]' : 'text-white'}>
                {staked ? 'UNSTAKE' : 'STAKE'}
              </small>
            </button>
          </div>
        {:else}
          <a
            href="/mint"
            class="flex h-14 items-center justify-center rounded-lg border-2 border-dashed border-[#304053]"
          >
            <small class="text-[#304053]">Mint a pet to start earning</small>
          </a>
        {/each}
      </section>

      <!-- Auto-feed -->
      <section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
        <div class="flex flex-row items-center justify-between">
          <span class="text-black">Auto-feed</span>
          <small class="text-[#00000075]">{autoFeed.toFixed(2)} credit / day</small>
        </div>
        <small class="text-[0.6rem] text-[#00000088]">
          While you are away, your pets feed themselves from your credit balance. Set it to 0 to
          turn it off.
        </small>
        <input
          type="range"
          min="0"
          max="1"
          step="0.05"
          bind:value={autoFeed}
          class="accent-[#f48f59]"
        />
        <button
          type="button"
          onclick={saveAutoFeed}
          class="h-10 w-full rounded-lg bg-[#304053]"
        >
          <small class="font-semibold text-white">SAVE</small>
        </button>
      </section>

      <!-- Sinks -->
      <section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
        <span class="text-black">What credits buy</span>
        {#each Object.values(SINKS) as sink (sink.action)}
          <div class="flex flex-row items-start justify-between gap-2 border-b border-[#304053]/20 pb-1">
            <div class="flex min-w-0 flex-col">
              <small class="text-black">
                {sink.label}
                {#if sink.usesInference}
                  <span class="text-[0.55rem] text-[#628ab4]">· real inference</span>
                {/if}
              </small>
              <small class="text-[0.6rem] text-[#00000088]">{sink.description}</small>
            </div>
            <div class="flex shrink-0 flex-col items-end">
              <small class="text-black">{sink.credits} ACR</small>
              {#if sink.joy}
                <small class="text-[0.6rem] text-[#00000088]">+{sink.joy} JOY</small>
              {/if}
            </div>
          </div>
        {/each}
      </section>

      <div class="clean"></div>
    </div>
  {/if}
</div>

<Footer />
