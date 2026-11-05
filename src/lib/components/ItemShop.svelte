<script lang="ts">
  import { feedSummary, type AiFeedResult } from '#lib/ai/client';
  import CreditItemShop from './CreditItemShop.svelte';
  import { performAiAction } from '#lib/economy/actions';
  import { SINKS, formatCredits } from '#lib/economy/params';
  import { activeUntil } from '#lib/economy/stake';
  import type { Pet } from '#lib/game/pets';
  import { liveCredits, petState, player } from '#lib/stores/player.svelte';
  import { toastError, toastSuccess } from '#lib/stores/toast.svelte';
  import { haptic, notify } from '#lib/platform';

  type Props = { pet: Pet | undefined };
  let { pet }: Props = $props();

  let feeding = $state(false);
  let lastMeal = $state<AiFeedResult | null>(null);

  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  // Two numbers, not one: `player.credits` is the spendable balance, and
  // `liveCredits` is what the stream has earned but not yet claimed. The
  // contract only spends the first, so the display adds them while the action
  // keeps them apart and claims when it has to.
  const claimable = $derived(liveCredits(now));
  const credits = $derived(player.credits + claimable);
  const feedCost = SINKS.ai_feed.credits;
  const petInfo = $derived(pet ? petState(pet.pet_id) : null);
  const lensFeeds = $derived(petInfo?.lens_feeds_left ?? 0);

  async function aiFeed() {
    if (!pet || feeding) return;
    feeding = true;
    haptic();

    try {
      const result = await performAiAction<AiFeedResult>('ai_feed', pet, {
        creditBalance: player.credits,
        pendingCredits: claimable
      });
      lastMeal = result;
      notify(result.xpAwarded ? 'success' : 'error');
      if (result.xpAwarded) {
        toastSuccess(feedSummary(result));
      } else {
        toastError(feedSummary(result));
      }
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

  {#if petInfo && activeUntil(petInfo.shielded_until, now)}
    <small class="text-center text-[0.6rem] text-[#628ab4]">
      {pet?.name} is warded — nobody can attack it right now.
    </small>
  {/if}
</div>
