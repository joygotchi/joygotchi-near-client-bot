<script lang="ts">
  import Modal from './Modal.svelte';
  import { formatNear } from '#lib/near/amount';
  import {
    buyPet,
    getPetListings,
    listPet,
    petImage,
    unlistPet,
    type Pet,
    type PetListing
  } from '#lib/game/pets';
  import { wallet } from '#lib/near/wallet.svelte';
  import { isStaked, player, refreshPlayer } from '#lib/stores/player.svelte';
  import { toastError, withToast } from '#lib/stores/toast.svelte';

  let listings = $state<PetListing[]>([]);
  let loading = $state(true);
  let selling = $state<Pet | null>(null);
  let askingPrice = $state('5');

  async function load() {
    loading = true;
    listings = await getPetListings();
    loading = false;
  }

  $effect(() => {
    void load();
  });

  const petById = (id: number) =>
    player.pets.find((p) => p.pet_id === id) ?? player.opponents.find((p) => p.pet_id === id);

  async function buy(listing: PetListing) {
    const price = Number(formatNear(listing.price, 6));
    if (player.near < price) {
      toastError(`Needs ${price} NEAR, you have ${player.near}`);
      return;
    }
    await withToast('Buying…', () => buyPet(listing.pet_id, listing.price), () => 'Pet bought');
    await Promise.all([refreshPlayer(true), load()]);
  }

  async function sell() {
    if (!selling) return;
    const pet = selling;
    await withToast('Listing…', () => listPet(pet.pet_id, askingPrice), () => 'Listed for sale');
    selling = null;
    await Promise.all([refreshPlayer(true), load()]);
  }

  async function unlist(petId: number) {
    await withToast('Removing…', () => unlistPet(petId), () => 'Listing removed');
    await Promise.all([refreshPlayer(true), load()]);
  }
</script>

<div class="grid grid-cols-1 gap-3 md:grid-cols-2 md:items-start">
  <!-- Your pets -->
  <section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
    <div class="flex flex-row items-center justify-between">
      <span class="text-black">Your pets</span>
      <small class="text-[#00000075]">{player.near.toLocaleString('en-US')} NEAR</small>
    </div>

    {#each player.pets as pet (pet.pet_id)}
      {@const staked = isStaked(pet.pet_id)}
      {@const listed = pet.listed_price}
      <div class="flex flex-row items-center justify-between gap-2 border-b border-[#304053]/20 pb-2">
        <div class="flex min-w-0 flex-row items-center gap-2">
          <img width="32" height="32" src={petImage(pet)} alt="" />
          <div class="flex min-w-0 flex-col">
            <small class="truncate text-black">{pet.name}</small>
            <small class="text-[0.55rem] text-[#00000088]">
              LV {pet.level} · {pet.star}★{#if pet.rarity} · {pet.rarity}{/if}
            </small>
          </div>
        </div>

        {#if listed}
          <div class="flex shrink-0 flex-row items-center gap-2">
            <small class="text-black">{formatNear(listed)} N</small>
            <button
              type="button"
              onclick={() => unlist(pet.pet_id)}
              class="rounded-lg border-2 border-[#304053] px-2 py-1"
            >
              <small class="text-[#304053]">REMOVE</small>
            </button>
          </div>
        {:else if staked}
          <small class="shrink-0 text-[0.55rem] text-[#00000088]">at work</small>
        {:else}
          <button
            type="button"
            onclick={() => {
              selling = pet;
              askingPrice = '5';
            }}
            class="shrink-0 rounded-lg bg-[#304053] px-3 py-1"
          >
            <small class="text-white">SELL</small>
          </button>
        {/if}
      </div>
    {:else}
      <small class="text-[0.6rem] text-[#00000088]">
        No pets yet. A staked pet cannot be sold — take it out of the AI Lab first.
      </small>
    {/each}
  </section>

  <!-- For sale -->
  <section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
    <div class="flex flex-row items-center justify-between">
      <span class="text-black">Pets for sale</span>
      <small class="text-[#00000075]">2.5% fee</small>
    </div>

    {#if loading}
      <small class="text-[0.6rem] text-[#00000088]">Loading…</small>
    {:else}
      {#each listings as listing (listing.pet_id)}
        {@const pet = petById(listing.pet_id)}
        {@const mine = listing.seller_id === wallet.accountId}
        <div
          class="flex flex-row items-center justify-between gap-2 border-b border-[#304053]/20 pb-2"
        >
          <div class="flex min-w-0 flex-row items-center gap-2">
            {#if pet}
              <img width="32" height="32" src={petImage(pet)} alt="" />
            {/if}
            <div class="flex min-w-0 flex-col">
              <small class="truncate text-black">{pet?.name ?? `Pet #${listing.pet_id}`}</small>
              <small class="truncate text-[0.55rem] text-[#00000088]">
                {mine ? 'your listing' : listing.seller_id}
              </small>
            </div>
          </div>
          <div class="flex shrink-0 flex-row items-center gap-2">
            <small class="text-black">{formatNear(listing.price)} NEAR</small>
            {#if mine}
              <button
                type="button"
                onclick={() => unlist(listing.pet_id)}
                class="rounded-lg border-2 border-[#304053] px-2 py-1"
              >
                <small class="text-[#304053]">REMOVE</small>
              </button>
            {:else}
              <button
                type="button"
                onclick={() => buy(listing)}
                class="rounded-lg bg-[#304053] px-3 py-1"
              >
                <small class="text-white">BUY</small>
              </button>
            {/if}
          </div>
        </div>
      {:else}
        <small class="text-[0.6rem] text-[#00000088]">
          Nothing listed yet. Pets sell for NEAR — credits are what you earn by playing and are
          deliberately not transferable.
        </small>
      {/each}
    {/if}
  </section>
</div>

<Modal
  title={selling ? `Sell ${selling.name}` : 'Sell'}
  open={selling !== null}
  onclose={() => (selling = null)}
>
  {#if selling}
    <div class="flex flex-col gap-3">
      <label class="flex flex-col gap-1">
        <small>Asking price, in NEAR</small>
        <input
          type="text"
          inputmode="decimal"
          bind:value={askingPrice}
          class="rounded-lg border-2 border-gray-300 px-3 py-2 outline-none focus:border-[#2d3c53]"
        />
      </label>
      <div class="flex flex-row justify-between rounded-lg bg-[#a9c6e4] px-3 py-2">
        <small class="text-black">You receive, after the 2.5% fee</small>
        <small class="text-black">
          {(Number(askingPrice || 0) * 0.975).toFixed(3)} NEAR
        </small>
      </div>
      <small class="text-[0.6rem] text-[#00000075]">
        Staking the pet, or transferring it, removes the listing — a buyer should never be sold a
        pet they cannot move.
      </small>
      <button
        type="button"
        onclick={sell}
        disabled={!(Number(askingPrice) > 0)}
        class="h-12 w-full rounded-lg bg-[#2d3c53] disabled:opacity-50"
      >
        <span class="text-white">LIST FOR SALE</span>
      </button>
    </div>
  {/if}
</Modal>
