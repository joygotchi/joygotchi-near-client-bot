<script lang="ts">
  import Footer from '#lib/components/Footer.svelte';
  import Header from '#lib/components/Header.svelte';
  import Modal from '#lib/components/Modal.svelte';
  import PetCarousel from '#lib/components/PetCarousel.svelte';
  import { BATTLE_WAGER_CREDITS, WOUND_HOURS } from '#lib/economy/params';
  import { attack, attackBlockedReason, petImage, petStatus } from '#lib/game/pets';
  import { wallet } from '#lib/near/wallet.svelte';
  import { isStaked, petState, player, refreshPlayer } from '#lib/stores/player.svelte';
  import { toastError, toastSuccess } from '#lib/stores/toast.svelte';
  import { haptic, notify } from '#lib/telegram';

  let mineIndex = $state(0);
  let targetIndex = $state(0);
  let picking = $state(false);
  let striking = $state(false);

  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 5000);
    return () => clearInterval(timer);
  });

  const mine = $derived(player.pets[mineIndex]);
  const target = $derived(player.opponents[targetIndex]);
  const staked = $derived(mine ? isStaked(mine.pet_id) : false);

  const blocked = $derived(
    attackBlockedReason(mine, target, {
      nowMs: now,
      mineStaked: staked,
      // A staked, warded or wounded pet is off limits — the contract enforces
      // this too, so attacking anyway would just burn gas.
      targetProtected: target ? !petState(target.pet_id).attackable : false
    })
  );

  async function strike() {
    if (!mine || !target) return;
    if (blocked) {
      toastError(blocked);
      return;
    }

    haptic('heavy');
    striking = true;
    try {
      await attack(mine.pet_id, mine.pet_id, target.pet_id);
      notify('success');
      toastSuccess('Attack landed!');
      await refreshPlayer(true);
    } catch (error) {
      notify('error');
      toastError(error instanceof Error ? error.message : String(error));
    } finally {
      setTimeout(() => (striking = false), 250);
    }
  }
</script>

<svelte:head>
  <title>JOYGOTCHI · Battle</title>
</svelte:head>

<Header />

<div class="screen relative h-full w-full overflow-y-auto bg-[#e5f2f8] md:w-[400px]">
  {#if !wallet.accountId}
    <div class="flex h-[60vh] items-center justify-center px-6">
      <p class="text-center text-black">Connect a wallet to battle.</p>
    </div>
  {:else if player.pets.length === 0}
    <div class="flex h-[60vh] flex-col items-center justify-center gap-3 px-6">
      <p class="text-center text-black">You need a pet first.</p>
      <a href="/mint" class="rounded-lg bg-[#2d3c53] px-4 py-2 text-white">Mint one</a>
    </div>
  {:else}
    <div class="relative px-2 pt-3">
      <img class="w-full rounded-lg" src="/assets/background/frame_battle.png" alt="" />

      <!-- Opponent -->
      <div class="absolute top-[18%] right-[18%]">
        {#if target}
          <img class="pet" src={petImage(target)} alt={target.name} />
          <small class="block text-center text-black">{target.name}</small>
          <small class="block text-center text-[0.6rem] text-[#00000088]">
            LV {target.level} · {petStatus(target, now)}
          </small>
        {:else}
          <small class="text-black">No opponents yet</small>
        {/if}
      </div>

      <!-- Mine -->
      <div class="absolute bottom-[22%] left-[14%] {striking ? 'translate-x-3 transition-transform' : 'transition-transform'}">
        <PetCarousel pets={player.pets} index={mineIndex} onselect={(next) => (mineIndex = next)} battle />
        <small class="block text-center text-[0.6rem] text-[#00000088]">
          LV {mine?.level} · {mine ? petStatus(mine, now) : '-'}
        </small>
      </div>
    </div>

    <div class="mt-3 flex flex-col gap-2 px-2">
      <button
        type="button"
        onclick={() => (picking = true)}
        class="flex flex-row items-center justify-between rounded-lg border-2 border-[#304053] px-3 py-2"
      >
        <small class="text-black">Opponent</small>
        <small class="text-black">{target ? `${target.name} (LV ${target.level})` : 'Pick one'}</small>
      </button>

      <div class="flex flex-row justify-between rounded-lg bg-[#a9c6e4] px-3 py-2">
        <small class="text-black">Wager</small>
        <small class="text-black">{BATTLE_WAGER_CREDITS} ACR · loser wounded {WOUND_HOURS}h</small>
      </div>

      {#if blocked}
        <small class="text-center text-[0.65rem] text-[#00000088]">{blocked}</small>
      {/if}

      <button
        type="button"
        onclick={strike}
        disabled={!target || !!blocked || striking}
        class="h-14 w-full rounded-lg bg-[#2d3c53] disabled:opacity-50"
      >
        <span class="text-xl font-semibold text-white">{striking ? 'ATTACKING…' : 'ATTACK'}</span>
      </button>

      <small class="text-center text-[0.6rem] text-[#00000088]">
        Nobody's pet dies here. The wager moves to the winner and the loser needs healing.
      </small>
    </div>
    <div class="clear"></div>
  {/if}
</div>

<Footer />

<Modal title="Choose an opponent" open={picking} onclose={() => (picking = false)}>
  <div class="flex max-h-[60vh] flex-col gap-2 overflow-y-auto">
    {#each player.opponents as opponent, index (opponent.pet_id)}
      {@const info = petState(opponent.pet_id)}
      <button
        type="button"
        onclick={() => {
          targetIndex = index;
          picking = false;
        }}
        disabled={!info.attackable}
        class="flex flex-row items-center justify-between rounded-lg border-2 border-[#304053] px-2 py-1 disabled:opacity-40"
      >
        <div class="flex flex-row items-center gap-2">
          <img width="36" height="36" src={petImage(opponent)} alt="" />
          <div class="flex flex-col text-left">
            <small>{opponent.name}</small>
            <small class="text-[0.6rem] text-[#00000088]">
              {#if info.staked}
                in the AI Lab
              {:else if !info.attackable}
                protected
              {:else}
                {opponent.owner_id}
              {/if}
            </small>
          </div>
        </div>
        <small>LV {opponent.level}</small>
      </button>
    {:else}
      <p class="text-sm">No other pets on-chain yet.</p>
    {/each}
  </div>
</Modal>
