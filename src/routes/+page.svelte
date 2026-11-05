<script lang="ts">
  import AppShell from '#lib/components/AppShell.svelte';
  import ConnectWallet from '#lib/components/ConnectWallet.svelte';
  import CountDownTimer from '#lib/components/CountDownTimer.svelte';
  import ItemShop from '#lib/components/ItemShop.svelte';
  import Loading from '#lib/components/Loading.svelte';
  import MintPanel from '#lib/components/MintPanel.svelte';
  import Modal from '#lib/components/Modal.svelte';
  import PetCarousel from '#lib/components/PetCarousel.svelte';
  import { formatCredits } from '#lib/economy/params';
  import { petStatus, renamePet, secondsUntilStarving, starProgress } from '#lib/game/pets';
  import { wallet } from '#lib/near/wallet.svelte';
  import { creditsPerDay, isStaked, player, refreshPlayer } from '#lib/stores/player.svelte';
  import { withToast } from '#lib/stores/toast.svelte';

  let index = $state(0);
  let renaming = $state(false);
  let draftName = $state('');

  const pet = $derived(player.pets[index]);

  $effect(() => {
    if (index >= player.pets.length) index = 0;
  });

  async function rename() {
    if (!pet) return;
    await withToast('Renaming…', () => renamePet(pet.pet_id, draftName), () => 'Name changed!');
    renaming = false;
    await refreshPlayer(true);
  }
</script>

<AppShell title="JOYGOTCHI">
  {#snippet banner()}
    {#if pet}
      <div class="flex flex-row items-center justify-center gap-2">
        <span class="rounded-full bg-[#f48f59] px-3 py-1 text-sm text-white">
          <CountDownTimer seconds={secondsUntilStarving(pet)} />
        </span>
        <small class="text-white/70">until {pet.name} starves</small>
      </div>
    {/if}
  {/snippet}

  {#if !wallet.ready}
    <div class="flex h-[60vh] items-center justify-center"><Loading /></div>
  {:else if !wallet.accountId}
    <div class="flex h-[60vh] items-center justify-center px-4">
      <ConnectWallet />
    </div>
  {:else if player.pets.length === 0}
    {#if player.loading && !player.loaded}
      <div class="flex h-[60vh] items-center justify-center"><Loading /></div>
    {:else}
      <div class="mx-auto w-full max-w-[460px]">
        <MintPanel />
      </div>
    {/if}
  {:else}
    <!-- Two columns from md up: the pet on the left, everything you do to it on the right. -->
    <div class="grid grid-cols-1 gap-4 md:grid-cols-2 md:items-start">
      <div class="flex flex-col gap-3">
        <div class="relative flex h-[250px] w-full flex-row justify-center rounded-md md:h-[320px]">
          <img class="h-full w-full rounded-md object-cover" src="/assets/background/screen_pet.png" alt="" />

          <div class="absolute top-[40%] left-1/2 -translate-x-1/2 -translate-y-1/2">
            <PetCarousel pets={player.pets} {index} onselect={(next) => (index = next)} />
          </div>

          <button
            type="button"
            class="absolute top-2 left-1/2 flex -translate-x-1/2 flex-row items-center gap-1"
            onclick={() => {
              draftName = pet?.name ?? '';
              renaming = true;
            }}
          >
            <p class="text-white">{pet?.name}</p>
            <img width="14" height="14" src="/assets/icon/pen.svg" alt="Rename" />
          </button>

          {#if pet && isStaked(pet.pet_id)}
            <div class="absolute top-2 right-2 rounded-full bg-[#2d3c53] px-2 py-1">
              <small class="text-[0.6rem] text-white">IN AI LAB</small>
            </div>
          {/if}
        </div>

        <div class="grid grid-cols-4 gap-2 rounded-lg bg-[#a9c6e4] px-3 py-4">
          <div class="flex flex-col text-center">
            <p class="text-xl">{pet?.level ?? '-'}</p>
            <span class="text-[0.6rem] text-[#00000088]">LEVEL</span>
          </div>
          <div class="flex flex-col text-center">
            <p class="text-xl">{pet?.star ?? '-'}</p>
            <span class="text-[0.6rem] text-[#00000088]">
              {#if pet}
                {@const progress = starProgress(pet)}
                {#if progress}
                  STAR · {progress.needed} TO GO
                {:else}
                  STAR
                {/if}
              {:else}
                STAR
              {/if}
            </span>
          </div>
          <div class="flex flex-col text-center">
            <p class="text-sm">{pet ? petStatus(pet) : '-'}</p>
            <span class="text-[0.6rem] text-[#00000088]">STATUS</span>
          </div>
          <div class="flex flex-col text-center">
            <p class="text-xl">{formatCredits(creditsPerDay(), 2)}</p>
            <span class="text-[0.6rem] text-[#00000088]">ACR/DAY</span>
          </div>
        </div>

        {#if player.stakedPetIds.length === 0}
          <a
            href="/stake"
            class="flex flex-row items-center justify-between rounded-lg border-2 border-[#6251A2] bg-[#6251A2]/25 px-3 py-3 hover:bg-[#6251A2]/35"
          >
            <div class="flex flex-col">
              <span class="text-black">Open the AI Lab</span>
              <small class="text-[0.6rem] text-[#00000088]">
                Stake a pet to earn AI credits every second
              </small>
            </div>
            <img width="14" height="14" src="/assets/icon/arrow_right.png" alt="" />
          </a>
        {/if}
      </div>

      <ItemShop {pet} />
    </div>
  {/if}
</AppShell>

<Modal title="Change pet name" open={renaming} onclose={() => (renaming = false)}>
  <label class="flex flex-col gap-1">
    <span>Pet name</span>
    <input
      type="text"
      bind:value={draftName}
      maxlength="32"
      placeholder="Enter new name"
      class="rounded-lg border-2 border-gray-300 px-3 py-2 outline-none hover:border-[#2d3c53] focus:border-[#2d3c53]"
    />
  </label>
  <div class="mt-7 flex justify-end">
    <button
      type="button"
      onclick={rename}
      class="h-12 w-32 rounded-lg bg-[#2d3c53] px-3 py-2 hover:bg-[#2d3c53]/90"
    >
      <span class="text-white">Change</span>
    </button>
  </div>
</Modal>
