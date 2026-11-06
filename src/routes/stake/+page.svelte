<script lang="ts">
  import ConnectWallet from '#lib/components/ConnectWallet.svelte';
  import AppShell from '#lib/components/AppShell.svelte';
  import {
    AUTO_FEED_EXTENDS_SECONDS,
    AUTO_FEED_WHEN_UNDER_SECONDS,
    BATTLE_WAGER_CREDITS,
    CREDIT_ITEM_LIST,
    FAUCET_GRANT_CREDITS,
    SINKS,
    WOUND_HOURS,
    formatCredits,
    petWeight,
    type Rarity
  } from '#lib/economy/params';
  import {
    activeUntil,
    claimCredits,
    claimFaucetCredits,
    setAutoFeedBudget,
    stakePet,
    unstakePet
  } from '#lib/economy/stake';
  import { evolutionPhase, petImage, type Pet } from '#lib/game/pets';
  import { wallet } from '#lib/near/wallet.svelte';
  import {
    creditsPerDay,
    isStaked,
    liveCredits,
    petState,
    player,
    refreshPlayer
  } from '#lib/stores/player.svelte';
  import { withToast } from '#lib/stores/toast.svelte';
  import { haptic } from '#lib/platform';

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

  /**
   * Auto-feeds the rest of today's budget still covers.
   *
   * Floored the same way the contract and the cron floor it: a remaining 0.01
   * credit buys no feed at all, and saying "0.01 left" where the player reads
   * "still working" is the kind of promise this whole feature was broken by.
   */
  const feedsLeftToday = $derived(
    Math.floor(
      Math.max(0, player.autoFeedBudget - player.autoFeedSpent) / SINKS.auto_feed.credits
    )
  );
  const share = $derived(
    player.totalWeight > 0 ? (player.weight / player.totalWeight) * 100 : 0
  );

  /**
   * v2 reports rarity for real. v1 has no such field and every pet on it is
   * star 0, so there is nothing to derive it from — `common` is the honest
   * answer rather than a guess.
   *
   * This used to read stars at thresholds of 9, 6 and 3, which `MAX_STAR = 5`
   * made unreachable. See issue #12.
   */
  const rarityOf = (pet: Pet): Rarity => pet.rarity ?? 'common';

  const weightOf = (pet: Pet) =>
    petWeight({
      rarity: rarityOf(pet),
      level: pet.level,
      star: pet.star,
      evolutionPhase: evolutionPhase(pet)
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
    await withToast('Claiming…', claimFaucetCredits, () => `+${FAUCET_GRANT_CREDITS} credit`);
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

<AppShell title="JOYGOTCHI · AI Lab">
  {#if !wallet.accountId}
    <div class="flex h-[60vh] items-center justify-center px-6">
      <ConnectWallet />
    </div>
  {:else}
    <div class="grid grid-cols-1 gap-3 md:grid-cols-2 md:items-start">
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
            <small class="font-semibold text-[#a9c6e4]">FREE GRANT</small>
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
          {@const info = petState(pet.pet_id)}
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
                <div class="flex flex-row gap-1">
                  {#if activeUntil(info.wounded_until, now)}
                    <small class="rounded bg-[#f8d7da] px-1 text-[0.5rem] text-[#a3242c]">
                      wounded
                    </small>
                  {/if}
                  {#if activeUntil(info.shielded_until, now)}
                    <small class="rounded bg-[#d4edda] px-1 text-[0.5rem] text-[#155724]">
                      warded
                    </small>
                  {/if}
                  {#if info.lens_feeds_left > 0}
                    <small class="rounded bg-[#a9c6e4] px-1 text-[0.5rem] text-[#2d3c53]">
                      lens {info.lens_feeds_left}
                    </small>
                  {/if}
                  {#if info.star_guard}
                    <small class="rounded bg-[#a9c6e4] px-1 text-[0.5rem] text-[#2d3c53]">
                      charm
                    </small>
                  {/if}
                </div>
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
          While you are away, any pet of yours within {AUTO_FEED_WHEN_UNDER_SECONDS / 3600} hours of
          starving is fed from your credit balance — {SINKS.auto_feed.credits} credit for
          {AUTO_FEED_EXTENDS_SECONDS / 3600} more hours of life. Spending stops at the budget below,
          which resets at 00:00 UTC. Set it to 0 to turn it off.
        </small>
        {#if player.autoFeedBudget > 0}
          <small class="text-[0.6rem] text-[#00000088]">
            Spent today: {formatCredits(player.autoFeedSpent)} of {formatCredits(
              player.autoFeedBudget
            )} credit
            {#if feedsLeftToday > 0}
              · {feedsLeftToday} more {feedsLeftToday === 1 ? 'feed' : 'feeds'} covered
            {:else}
              · today's budget is used up
            {/if}
          </small>
        {/if}
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

      <!-- Items -->
      <section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
        <span class="text-black">Credit items</span>
        {#each CREDIT_ITEM_LIST as item (item.id)}
          <div class="flex flex-row items-start justify-between gap-2 border-b border-[#304053]/20 pb-1">
            <div class="flex min-w-0 flex-col">
              <small class="text-black">{item.label}</small>
              <small class="text-[0.6rem] text-[#00000088]">{item.description}</small>
            </div>
            <small class="shrink-0 text-black">{item.credits} ACR</small>
          </div>
        {/each}
        <small class="text-[0.6rem] text-[#00000088]">
          Buy them on the pet screen. A staked pet can never be attacked, so a Sentinel Ward only
          matters once you take it out of the lab.
        </small>
      </section>

      <!-- Battle -->
      <section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
        <span class="text-black">Battle</span>
        <div class="flex flex-row justify-between">
          <small class="text-[#00000088]">Wager</small>
          <small class="text-black">{BATTLE_WAGER_CREDITS} ACR → winner</small>
        </div>
        <div class="flex flex-row justify-between">
          <small class="text-[#00000088]">Losing costs</small>
          <small class="text-black">the wager + {WOUND_HOURS}h wounded</small>
        </div>
        <small class="text-[0.6rem] text-[#00000088]">
          Pets never die from a fight. Credits are streamed by weight share, so killing another
          staker's pet would raise the attacker's own income — death and a share-based stream
          together would make griefing profitable.
        </small>
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
            </div>
          </div>
        {/each}
      </section>

    </div>
  {/if}
</AppShell>
