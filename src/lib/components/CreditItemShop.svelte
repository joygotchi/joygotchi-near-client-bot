<script lang="ts">
  import { buyCreditItem } from '#lib/economy/stake';
  import {
    CREDIT_ITEM_LIST,
    formatCredits,
    type CreditItemKey
  } from '#lib/economy/params';
  import { activeUntil } from '#lib/economy/stake';
  import type { Pet } from '#lib/game/pets';
  import { petState, player, refreshPlayer } from '#lib/stores/player.svelte';
  import { toastError, withToast } from '#lib/stores/toast.svelte';
  import { haptic } from '#lib/telegram';

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

  async function buy(key: CreditItemKey) {
    if (!pet) return;
    const item = CREDIT_ITEM_LIST.find((entry) => entry.key === key)!;
    if (credits < item.credits) {
      toastError(`Needs ${item.credits} credit, you have ${formatCredits(credits)}`);
      return;
    }

    haptic();
    busy = key;
    await withToast(
      `Buying ${item.label}…`,
      () => buyCreditItem(key, pet.pet_id),
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
        <button
          type="button"
          onclick={() => buy(item.key)}
          disabled={busy !== null || credits < item.credits}
          class="flex flex-col gap-1 rounded-lg border-2 border-[#304053] p-2 text-left disabled:opacity-50"
        >
          <div class="flex flex-row items-center justify-between">
            <div class="flex flex-row items-center gap-1">
              <img width="18" height="18" src={item.icon} alt="" />
              <small class="text-black">{item.label}</small>
            </div>
            <small class="text-[0.6rem] text-black">{item.credits}</small>
          </div>
          <small class="text-[0.55rem] leading-tight text-[#00000088]">
            {item.description}
          </small>
          {#if note}
            <small class="text-[0.55rem] text-[#628ab4]">· {note}</small>
          {:else if busy === item.key}
            <small class="text-[0.55rem] text-[#628ab4]">· buying…</small>
          {/if}
        </button>
      {/each}
    </div>

    {#if player.stakedPetIds.length === 0 && credits <= 0}
      <small class="text-[0.6rem] text-[#00000088]">
        No credits yet — stake a pet in the AI Lab, or claim the one-off free grant there.
      </small>
    {/if}
  {/if}
</section>
