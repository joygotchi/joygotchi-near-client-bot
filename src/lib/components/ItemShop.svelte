<script lang="ts">
  import type { AiFeedResult } from '#lib/ai/client';
  import CreditItemShop from './CreditItemShop.svelte';
  import { performAiAction } from '#lib/economy/actions';
  import { SINKS, formatCredits } from '#lib/economy/params';
  import { activeUntil } from '#lib/economy/stake';
  import { buyItem, isAlive, JOY_ITEMS, REVIVAL_ITEM_ID, type Pet } from '#lib/game/pets';
  import { liveCredits, petState, player, refreshPlayer } from '#lib/stores/player.svelte';
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
  const item = $derived(JOY_ITEMS[itemIndex]);
  const feedCost = SINKS.ai_feed.credits;
  const petInfo = $derived(pet ? petState(pet.pet_id) : null);
  const lensFeeds = $derived(petInfo?.lens_feeds_left ?? 0);

  async function buy() {
    if (!pet) return;
    haptic();

    // Derived from the starving deadline, not from `pet.status`, which the
    // contract leaves stale until something touches the pet.
    if (!isAlive(pet, now) && item.id !== REVIVAL_ITEM_ID) {
      toastError('Pet has starved — use Holy Water to revive it');
      return;
    }

    await withToast('Buying…', () => buyItem(pet.pet_id, item.id), () => `Bought ${item.name}!`);
    await refreshPlayer(true);
  }

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
      toastSuccess(
        `+${result.xpGained} XP (${result.xpMultiplier}x${result.lensApplied ? ', lens' : ''})`
      );
    } catch (error) {
      notify('error');
      toastError(error instanceof Error ? error.message : String(error));
    } finally {
      feeding = false;
    }
  }
</script>

<div class="flex flex-col gap-3">
  <!-- AI feed -->
  <div class="mt-3 flex flex-col gap-2 rounded-lg border-2 border-[#304053] bg-[#a9c6e4] px-3 py-3">
    <div class="flex flex-row items-center justify-between">
      <div class="flex flex-col">
        <span class="text-black">AI FEED</span>
        <small class="text-[0.6rem] text-[#00000088]">
          Your pet picks its own meal · up to {lensFeeds > 0 ? '2.05' : '1.8'}x XP
        </small>
      </div>
      <div class="flex flex-row items-center gap-1">
        <img width="18" height="18" src="/assets/item/credit_card.png" alt="" />
        <small class="text-black">{feedCost}</small>
      </div>
    </div>

    {#if lensFeeds > 0}
      <small class="text-[0.6rem] text-[#2d3c53]">
        Focus Lens active — {lensFeeds} feed{lensFeeds === 1 ? '' : 's'} left
      </small>
    {/if}

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
        You have {formatCredits(credits)} credit. Stake a pet in the AI Lab, or buy a $JOY item
        below.
      </small>
    {/if}

    {#if lastMeal}
      <div class="rounded-lg bg-white/70 px-2 py-2">
        <small class="block text-[0.7rem] font-bold text-black">{lastMeal.meal}</small>
        <small class="block text-[0.65rem] text-[#00000099]">“{lastMeal.reaction}”</small>
      </div>
    {/if}
  </div>

  <!-- Credit items -->
  <CreditItemShop {pet} {credits} />

  <!-- $JOY items from the game contract -->
  <div class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
    <div class="flex flex-row items-center justify-between">
      <span class="text-black">$JOY shop</span>
      <div class="flex flex-row items-center gap-1">
        <img width="16" height="16" src="/assets/item/coin.png" alt="" />
        <small class="text-black">{player.joy.toLocaleString('en-US')}</small>
      </div>
    </div>

    <div class="flex w-full flex-row items-center justify-between gap-3">
      {#each JOY_ITEMS as option, index (option.id)}
        <button type="button" onclick={() => (itemIndex = index)} aria-label={option.name}>
          <div
            class="flex h-14 w-14 items-center justify-center rounded-lg hover:bg-[#628ab4] {itemIndex ===
            index
              ? 'bg-[#628ab4]'
              : 'bg-[#a9c6e4]'}"
          >
            <img width={option.width} src={option.icon} alt={option.name} />
          </div>
        </button>
      {/each}
    </div>

    <div class="flex w-full flex-row items-center justify-between">
      <div class="flex flex-col">
        <small class="text-black">BUY 1 {item.name}</small>
        <small class="text-[0.55rem] text-[#00000088]">
          {#if item.isRevival}
            Revives a starved pet
          {:else if item.timeExtension > 0}
            +{Math.round(item.timeExtension / 3_600_000)}h before starving
          {:else}
            Blocks one attack
          {/if}
        </small>
      </div>
      <small class="text-black">{item.joy} JOY</small>
    </div>

    <button
      type="button"
      onclick={buy}
      disabled={!pet}
      class="h-10 w-full rounded-lg bg-[#2f3b53] disabled:opacity-50"
    >
      <span class="font-semibold text-[#fff]">BUY</span>
    </button>
  </div>

  {#if petInfo && activeUntil(petInfo.shielded_until, now)}
    <small class="text-center text-[0.6rem] text-[#628ab4]">
      {pet?.name} is warded — nobody can attack it right now.
    </small>
  {/if}
</div>
