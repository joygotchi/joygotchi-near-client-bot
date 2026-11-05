<script lang="ts">
  import { page } from '$app/state';
  import Footer from '#lib/components/Footer.svelte';
  import Header from '#lib/components/Header.svelte';
  import MintPanel from '#lib/components/MintPanel.svelte';
  import SettingsPanel from '#lib/components/SettingsPanel.svelte';
  import TopUpPanel from '#lib/components/TopUpPanel.svelte';

  const tabs = ['Mint NFT', 'Top up', 'Settings'];

  let current = $state(Number(page.url.searchParams.get('tab') ?? 0));

  $effect(() => {
    const tab = Number(page.url.searchParams.get('tab') ?? 0);
    if (!Number.isNaN(tab) && tab >= 0 && tab < tabs.length) current = tab;
  });
</script>

<svelte:head>
  <title>JOYGOTCHI · Pets</title>
</svelte:head>

<Header />

<div class="screen h-full w-full overflow-y-auto bg-[#e5f2f8] md:w-[400px]">
  <div class="mt-4 px-2">
    <div class="mb-5 flex w-full flex-row items-center justify-between rounded-lg bg-[#a9c6e4]">
      {#each tabs as tab, index (tab)}
        <button type="button" onclick={() => (current = index)} class="w-1/3">
          <div class="flex justify-center rounded-lg p-2 hover:bg-[#628ab4] {current === index ? 'bg-[#628ab4]' : ''}">
            <span class="text-sm">{tab}</span>
          </div>
        </button>
      {/each}
    </div>

    {#if current === 0}
      <MintPanel />
    {:else if current === 1}
      <TopUpPanel />
    {:else}
      <SettingsPanel />
    {/if}
  </div>
  <div class="clean"></div>
</div>

<Footer />
