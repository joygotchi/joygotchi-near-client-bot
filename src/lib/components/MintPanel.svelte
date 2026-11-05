<script lang="ts">
  import { MINT_PRICE_WNEAR } from '#lib/config';
  import { createPet, PET_SPECIES } from '#lib/game/pets';
  import { wallet } from '#lib/near/wallet.svelte';
  import { refreshPlayer } from '#lib/stores/player.svelte';
  import { withToast } from '#lib/stores/toast.svelte';
  import ConnectWallet from './ConnectWallet.svelte';
  import CrossChainPay from './CrossChainPay.svelte';

  let speciesIndex = $state(0);
  let petName = $state('PET');
  let method = $state<'coin' | 'joy'>('coin');

  const species = $derived(PET_SPECIES[speciesIndex]);

  async function mintWithJoy() {
    await withToast(
      'Minting…',
      () =>
        createPet(petName || 'PET', {
          title: species.label,
          description: `Power of ${species.label.toLowerCase()}`,
          media: species.preview
        }),
      () => 'Pet minted!'
    );
    await refreshPlayer(true);
  }
</script>

<div class="flex flex-col gap-3">
  <!-- Preview card -->
  <div class="w-full rounded-lg border-2 border-[#304053] shadow-sm">
    <div class="w-full rounded-t-md bg-[#304053] py-1 text-center">
      <span class="text-xl">NFT PET</span>
    </div>
    <div class="flex flex-row items-center justify-between gap-5 px-3 py-4">
      <div class="flex items-center justify-center rounded-md border-2 border-[#304053] p-3">
        <img width="110" height="110" src={species.preview} alt={species.label} />
      </div>
      <div class="w-full text-black">
        <div class="flex flex-col gap-2">
          <div class="flex flex-row items-center justify-between border-b-2 border-[#304053]">
            <span class="text-[#00000075]">NAME</span>
            <span>{species.label}</span>
          </div>
          <div class="flex flex-row items-center justify-between">
            <span class="text-[#00000075]">ATK</span>
            <span>{species.atk}</span>
          </div>
          <div class="flex flex-row items-center justify-between">
            <span class="text-[#00000075]">DEF</span>
            <span>{species.def}</span>
          </div>
        </div>
      </div>
    </div>
  </div>

  <!-- Species picker -->
  <div class="flex flex-row items-center justify-start gap-5">
    {#each PET_SPECIES as option, index (option.category)}
      <button
        type="button"
        onclick={() => (speciesIndex = index)}
        class="relative mt-2 h-28 w-28 cursor-pointer rounded-lg border-2 border-[#304053] {speciesIndex ===
        index
          ? 'border-opacity-100'
          : 'opacity-50'}"
      >
        <img class="ml-3 h-20 w-20 rounded-lg" src={option.preview} alt={option.label} />
        <small class="absolute bottom-1 ml-1 w-full text-[0.7rem] text-black">
          {option.label}
        </small>
      </button>
    {/each}
  </div>

  <label class="flex flex-col gap-1">
    <small class="text-[#00000075]">Pet name</small>
    <input
      type="text"
      bind:value={petName}
      maxlength="32"
      class="rounded-lg border-2 border-gray-300 px-3 py-2 text-black outline-none focus:border-[#2d3c53]"
    />
  </label>

  {#if !wallet.accountId}
    <div class="rounded-lg border-2 border-[#304053] p-4">
      <ConnectWallet />
    </div>
  {:else}
    <!-- Payment method -->
    <div class="flex flex-row items-center justify-between rounded-lg bg-[#a9c6e4]">
      <button
        type="button"
        onclick={() => (method = 'coin')}
        class="w-1/2 rounded-lg p-2 {method === 'coin' ? 'bg-[#628ab4]' : ''}"
      >
        <span class="text-sm">ZEC / SOL / BTC</span>
      </button>
      <button
        type="button"
        onclick={() => (method = 'joy')}
        class="w-1/2 rounded-lg p-2 {method === 'joy' ? 'bg-[#628ab4]' : ''}"
      >
        <span class="text-sm">$JOY on NEAR</span>
      </button>
    </div>

    {#if method === 'coin'}
      <CrossChainPay
        purpose="mint_pet"
        summary={`1 ${species.label}`}
        petCategory={species.category}
        {petName}
        onsettled={() => refreshPlayer(true)}
      />
    {:else}
      <div
        class="flex h-32 w-full flex-col items-center justify-center gap-2 rounded-lg border-2 border-[#304053]"
      >
        <div class="flex flex-col text-center">
          <span class="text-black">MINT 1 pet</span>
          <span class="text-black">costs {MINT_PRICE_WNEAR} wNEAR of $JOY</span>
        </div>
        <button
          type="button"
          onclick={mintWithJoy}
          class="h-12 w-44 rounded-lg bg-[#304053] px-3 py-2"
        >
          <span class="font-semibold">MINT</span>
        </button>
      </div>
    {/if}
  {/if}
</div>
