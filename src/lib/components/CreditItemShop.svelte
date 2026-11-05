<script lang="ts">
  import { buyAndUseItem, useItem } from '#lib/economy/stake';
  import {
    CREDIT_ITEM_LIST,
    formatCredits,
    type CreditItemKey
  } from '#lib/economy/params';
  import { activeUntil } from '#lib/economy/stake';
  import type { Pet } from '#lib/game/pets';
  import { itemCount, petState, player, refreshPlayer } from '#lib/stores/player.svelte';
  import { toastError, withToast } from '#lib/stores/toast.svelte';
  import { haptic } from '#lib/platform';

  type Props = { pet: Pet | undefined; credits: number };
  let { pet, credits }: Props = $props();

  let busy = $state<CreditItemKey | null>(null);
  let now = $state(Date.now());

  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 10_000);
    return () => clearInterval(timer);
  });

  const petInfo = $derived(pet ? petState(pet.pet_id) : null);

  /** Short badge shown when an item is currently doing something. */
  function activeNote(key: CreditItemKey): string | null {
    if (!petInfo) return null;
    if (key === 'ward' && activeUntil(petInfo.shielded_until, now)) {
      const hours = Math.ceil((Number(petInfo.shielded_until) / 1e6 - now) / 3_600_000);
      return `${hours}h left`;
    }
    if (key === 'lens' && petInfo.lens_feeds_left > 0) return `${petInfo.lens_feeds_left} feeds`;
    if (key === 'charm' && petInfo.star_guard) return 'ready';
    if (key === 'tonic' && activeUntil(petInfo.wounded_until, now)) {
      const hours = Math.ceil((Number(petInfo.wounded_until) / 1e6 - now) / 3_600_000);
      return `wounded ${hours}h`;
    }
    return null;
  }

  /**
   * Use one from the inventory if there is one — items now drop and can be
   * bought from other players, so buying is the fallback, not the only path.
   */
  async function use(key: CreditItemKey) {
    if (!pet) return;
    const item = CREDIT_ITEM_LIST.find((entry) => entry.key === key)!;
    const owned = itemCount(item.id);

    if (owned === 0 && credits < item.credits) {
      toastError(`Needs ${item.credits} credit, you have ${formatCredits(credits)}`);
      return;
    }

    haptic();
    busy = key;
    await withToast(
      owned > 0 ? `Using ${item.label}…` : `Buying ${item.label}…`,
      () => (owned > 0 ? useItem(key, pet.pet_id) : buyAndUseItem(key, pet.pet_id)),
      () => `${item.label} applied to ${pet.name}`
    );
    busy = null;
    await refreshPlayer(true);
  }
</script>

<section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
  <div class="flex flex-row items-center justify-between">
    <span class="text-black">Credit items</span>
    <div class="flex flex-row items-center gap-1">
      <img width="16" height="16" src="/assets/item/credit_card.png" alt="" />
      <small class="text-black">{formatCredits(credits)}</small>
    </div>
  </div>

  {#if !pet}
    <small class="text-[0.6rem] text-[#00000088]">Mint a pet to use these.</small>
  {:else}
    {#if petInfo && activeUntil(petInfo.wounded_until, now)}
      <div class="rounded-lg bg-[#f8d7da] px-2 py-1">
        <small class="text-[0.6rem] text-[#a3242c]">
          {pet.name} is wounded and cannot battle. An AI Tonic clears it now.
        </small>
      </div>
    {/if}

    <div class="grid grid-cols-2 gap-2">
      {#each CREDIT_ITEM_LIST as item (item.id)}
        {@const note = activeNote(item.key)}
        {@const owned = itemCount(item.id)}
        <button
          type="button"
          onclick={() => use(item.key)}
          disabled={busy !== null || (owned === 0 && credits < item.credits)}
          class="flex flex-col gap-1 rounded-lg border-2 border-[#304053] p-2 text-left disabled:opacity-50"
        >
          <div class="flex flex-row items-center justify-between">
            <div class="flex flex-row items-center gap-1">
              <img width="18" height="18" src={item.icon} alt="" />
              <small class="text-black">{item.label}</small>
            </div>
            {#if owned > 0}
              <small class="rounded bg-[#a9c6e4] px-1 text-[0.6rem] text-[#2d3c53]">x{owned}</small>
            {:else}
              <small class="text-[0.6rem] text-black">{item.credits}</small>
            {/if}
          </div>
          <small class="text-[0.55rem] leading-tight text-[#00000088]">
            {item.description}
          </small>
          {#if busy === item.key}
            <small class="text-[0.55rem] text-[#628ab4]">· working…</small>
          {:else if note}
            <small class="text-[0.55rem] text-[#628ab4]">· {note}</small>
          {:else if owned > 0}
            <small class="text-[0.55rem] text-[#628ab4]">· tap to use</small>
          {/if}
        </button>
      {/each}
    </div>

    {#if player.stakedPetIds.length === 0 && credits <= 0}
      <small class="text-[0.6rem] text-[#00000088]">
        No credits yet — stake a pet in the AI Lab, or claim the one-off free grant there.
      </small>
    {/if}

    <a href="/market" class="text-center text-[0.6rem] text-[#628ab4] underline">
      Items also drop while you play, and other players sell them
    </a>
  {/if}
</section>
