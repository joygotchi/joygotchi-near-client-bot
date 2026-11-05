<script lang="ts">
  import type { AiFeedResult } from '#lib/ai/client';
  import { performAiAction } from '#lib/economy/actions';
  import { JOY_FEED_COST, SINKS, formatCredits } from '#lib/economy/params';
  import { buyItem, ITEMS, isPetAlive, type Pet } from '#lib/game/pets';
  import { liveCredits, player, refreshPlayer } from '#lib/stores/player.svelte';
  import { toastError, toastSuccess, withToast } from '#lib/stores/toast.svelte';
  import { haptic, notify } from '#lib/telegram';

  type Props = { pet: Pet | undefined };
  let { pet }: Props = $props();

  let itemIndex = $state(0);
  let feeding = $state(false);
  let lastMeal = $state<AiFeedResult | null>(null);

  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const credits = $derived(player.credits + liveCredits(now));
  const item = $derived(ITEMS[itemIndex]);
  const feedCost = SINKS.ai_feed.credits;

  async function buy() {
    if (!pet) return;
    haptic();

    const alive = await isPetAlive(pet.pet_id).catch(() => true);
    if (!alive && item.name !== 'holy water') {
      toastError('Pet is not alive — use holy water');
      return;
    }

    await withToast('Buying…', () => buyItem(pet.pet_id, item.id), () => `Bought ${item.name}!`);
    await refreshPlayer(true);
  }

  /** The headline action: feed through NEAR AI and get a taste-based XP bonus. */
  async function aiFeed() {
    if (!pet || feeding) return;
    feeding = true;
    haptic();

    try {
      const result = await performAiAction<AiFeedResult>('ai_feed', pet, {
        creditBalance: credits
      });
      lastMeal = result;
      notify('success');
      toastSuccess(`+${result.xpGained} XP (${result.xpMultiplier}x)`);
    } catch (error) {
      notify('error');
      toastError(error instanceof Error ? error.message : String(error));
    } finally {
      feeding = false;
    }
  }
</script>

<div class="flex flex-col">
  <!-- AI feed -->
  <div class="mt-3 flex flex-col gap-2 rounded-lg border-2 border-[#304053] bg-[#a9c6e4] px-3 py-3">
    <div class="flex flex-row items-center justify-between">
      <div class="flex flex-col">
        <span class="text-black">AI FEED</span>
        <small class="text-[0.6rem] text-[#00000088]">
          Your pet picks its own meal · up to 1.8x XP
        </small>
      </div>
      <div class="flex flex-row items-center gap-1">
        <img width="18" height="18" src="/assets/item/credit_card.png" alt="" />
        <small class="text-black">{feedCost}</small>
      </div>
    </div>

    <button
      type="button"
      onclick={aiFeed}
      disabled={!pet || feeding || credits < feedCost}
      class="h-10 w-full rounded-lg bg-[#2f3b53] disabled:opacity-50"
    >
      <span class="font-semibold text-white">
        {feeding ? 'Thinking…' : credits < feedCost ? 'Not enough credits' : 'FEED WITH AI'}
      </span>
    </button>

    {#if credits < feedCost}
      <small class="text-center text-[0.6rem] text-[#00000088]">
        You have {formatCredits(credits)} credit. Stake a pet in the AI Lab, or buy an item with
        $JOY below.
      </small>
    {/if}

    {#if lastMeal}
      <div class="rounded-lg bg-white/70 px-2 py-2">
        <small class="block text-[0.7rem] font-bold text-black">{lastMeal.meal}</small>
        <small class="block text-[0.65rem] text-[#00000099]">“{lastMeal.reaction}”</small>
      </div>
    {/if}
  </div>

  <!-- $JOY items -->
  <div class="mt-3 flex w-full flex-row items-center justify-between gap-5">
    {#each ITEMS as option, index (option.id)}
      <button type="button" onclick={() => (itemIndex = index)} aria-label={option.name}>
        <div
          class="flex h-16 w-16 justify-center rounded-lg p-2 hover:bg-[#628ab4] {itemIndex ===
          index
            ? 'bg-[#628ab4]'
            : 'bg-[#a9c6e4]'}"
        >
          <img width={option.width} src={option.icon} alt={option.name} />
        </div>
      </button>
    {/each}
  </div>

  <div class="mt-3 max-h-36 w-full rounded-lg bg-[#a9c6e4] px-3 py-4">
    <div class="flex w-full flex-row justify-between">
      <p>BUY 1 {item.name}</p>
      <p>{item.joy ?? JOY_FEED_COST} JOY</p>
    </div>
    <div class="mt-2 flex w-full flex-row justify-center">
      <button type="button" onclick={buy} disabled={!pet} class="h-10 w-48 rounded-lg bg-[#2f3b53] disabled:opacity-50">
        <span class="font-semibold text-[#fff]">BUY</span>
      </button>
    </div>
  </div>
</div>
