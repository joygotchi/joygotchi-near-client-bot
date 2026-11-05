<script lang="ts">
  import AppShell from '#lib/components/AppShell.svelte';
  import ConnectWallet from '#lib/components/ConnectWallet.svelte';
  import Modal from '#lib/components/Modal.svelte';
  import PetMarket from '#lib/components/PetMarket.svelte';
  import {
    CREDIT_ITEMS,
    CREDIT_ITEM_LIST,
    MARKET_RAKE_BPS,
    creditItemById,
    dropChancePercent,
    droppableItems,
    formatCredits,
    type CreditItemKey
  } from '#lib/economy/params';
  import {
    buyListing,
    cancelListing,
    getListings,
    listItem,
    type Listing
  } from '#lib/economy/stake';
  import { CREDIT_DECIMALS } from '#lib/economy/params';
  import { formatAmount } from '#lib/near/amount';
  import { wallet } from '#lib/near/wallet.svelte';
  import { itemCount, liveCredits, player, refreshPlayer } from '#lib/stores/player.svelte';
  import { toastError, withToast } from '#lib/stores/toast.svelte';

  let tab = $state<'items' | 'pets'>('items');

  let listings = $state<Listing[]>([]);
  let loading = $state(true);
  let selling = $state<CreditItemKey | null>(null);
  let sellQty = $state(1);
  let sellPrice = $state(0.02);

  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 5000);
    return () => clearInterval(timer);
  });

  const credits = $derived(player.credits + liveCredits(now));
  const chance = $derived(dropChancePercent(player.weight));
  const canFind = $derived(droppableItems(player.weight));

  async function load() {
    loading = true;
    listings = await getListings();
    loading = false;
  }

  $effect(() => {
    void load();
  });

  const priceOf = (listing: Listing) =>
    Number(formatAmount(listing.price_each, CREDIT_DECIMALS));

  async function buy(listing: Listing) {
    if (credits < priceOf(listing)) {
      toastError('Not enough credits');
      return;
    }
    await withToast('Buying…', () => buyListing(listing.id, 1), () => 'Bought');
    await Promise.all([refreshPlayer(true), load()]);
  }

  async function cancel(listing: Listing) {
    await withToast('Cancelling…', () => cancelListing(listing.id), () => 'Items returned');
    await Promise.all([refreshPlayer(true), load()]);
  }

  async function sell() {
    if (!selling) return;
    const key = selling;
    await withToast(
      'Listing…',
      () => listItem(key, sellQty, sellPrice),
      () => 'Listed for sale'
    );
    selling = null;
    await Promise.all([refreshPlayer(true), load()]);
  }
</script>

<AppShell title="JOYGOTCHI · Market">
  {#if !wallet.accountId}
    <div class="flex h-[60vh] items-center justify-center px-6">
      <ConnectWallet />
    </div>
  {:else}
    <div class="mb-3 flex w-full flex-row items-center justify-between rounded-lg bg-[#a9c6e4]">
      <button type="button" onclick={() => (tab = 'items')} class="w-1/2">
        <div class="flex justify-center rounded-lg p-2 {tab === 'items' ? 'bg-[#628ab4]' : ''}">
          <span class="text-sm">Items</span>
        </div>
      </button>
      <button type="button" onclick={() => (tab = 'pets')} class="w-1/2">
        <div class="flex justify-center rounded-lg p-2 {tab === 'pets' ? 'bg-[#628ab4]' : ''}">
          <span class="text-sm">Pets</span>
        </div>
      </button>
    </div>

    {#if tab === 'pets'}
      <PetMarket />
    {:else}
    <div class="grid grid-cols-1 gap-3 md:grid-cols-2 md:items-start">
      <!-- What your pets find -->
      <section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
        <div class="flex flex-row items-center justify-between">
          <span class="text-black">Your drop rate</span>
          <span class="text-xl text-[#f48f59]">{chance.toFixed(1)}%</span>
        </div>
        <small class="text-[0.6rem] text-[#00000088]">
          Every AI feed, training session, craft, mining claim and battle win rolls for an item.
          A stronger pet finds things more often <em>and</em> finds better things — staking weight
          {player.weight.toLocaleString('en-US')} is what sets both.
        </small>

        {#if canFind.length > 0}
          <div class="flex flex-row flex-wrap gap-1">
            {#each canFind as key (key)}
              <span class="flex flex-row items-center gap-1 rounded bg-[#a9c6e4] px-2 py-1">
                <img width="14" height="14" src={CREDIT_ITEMS[key].icon} alt="" />
                <small class="text-[0.55rem] text-[#2d3c53]">{CREDIT_ITEMS[key].label}</small>
              </span>
            {/each}
          </div>
        {:else}
          <small class="text-[0.6rem] text-[#00000088]">
            Stake a pet in the AI Lab to start finding items.
          </small>
        {/if}
      </section>

      <!-- Inventory -->
      <section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3">
        <div class="flex flex-row items-center justify-between">
          <span class="text-black">Your items</span>
          <small class="text-[#00000075]">{formatCredits(credits)} ACR</small>
        </div>

        {#each CREDIT_ITEM_LIST as item (item.id)}
          {@const owned = itemCount(item.id)}
          {#if owned > 0}
            <div class="flex flex-row items-center justify-between gap-2">
              <div class="flex flex-row items-center gap-2">
                <img width="18" height="18" src={item.icon} alt="" />
                <small class="text-black">{item.label}</small>
                <small class="text-[0.6rem] text-[#00000088]">x{owned}</small>
              </div>
              <button
                type="button"
                onclick={() => {
                  selling = item.key;
                  sellQty = 1;
                  sellPrice = item.credits;
                }}
                class="rounded-lg bg-[#304053] px-3 py-1"
              >
                <small class="text-white">SELL</small>
              </button>
            </div>
          {/if}
        {:else}
          <small class="text-[0.6rem] text-[#00000088]">Nothing yet.</small>
        {/each}

        {#if player.items.length === 0}
          <small class="text-[0.6rem] text-[#00000088]">
            Items you find or buy show up here. Use them from the pet screen.
          </small>
        {/if}
      </section>

      <!-- Listings -->
      <section class="flex flex-col gap-2 rounded-lg border-2 border-[#304053] p-3 md:col-span-2">
        <div class="flex flex-row items-center justify-between">
          <span class="text-black">For sale</span>
          <small class="text-[#00000075]">{MARKET_RAKE_BPS / 100}% fee</small>
        </div>

        {#if loading}
          <small class="text-[0.6rem] text-[#00000088]">Loading…</small>
        {:else}
          {#each listings as listing (listing.id)}
            {@const item = creditItemById(listing.item_id)}
            {@const mine = listing.seller_id === wallet.accountId}
            <div
              class="flex flex-row items-center justify-between gap-2 border-b border-[#304053]/20 pb-2"
            >
              <div class="flex min-w-0 flex-row items-center gap-2">
                <img width="20" height="20" src={item?.icon ?? '/assets/item/star.png'} alt="" />
                <div class="flex min-w-0 flex-col">
                  <small class="text-black">{item?.label ?? `Item ${listing.item_id}`}</small>
                  <small class="truncate text-[0.55rem] text-[#00000088]">
                    {listing.quantity} left · {mine ? 'your listing' : listing.seller_id}
                  </small>
                </div>
              </div>
              <div class="flex shrink-0 flex-row items-center gap-2">
                <small class="text-black">{formatCredits(priceOf(listing), 4)} ACR</small>
                {#if mine}
                  <button
                    type="button"
                    onclick={() => cancel(listing)}
                    class="rounded-lg border-2 border-[#304053] px-2 py-1"
                  >
                    <small class="text-[#304053]">CANCEL</small>
                  </button>
                {:else}
                  <button
                    type="button"
                    onclick={() => buy(listing)}
                    disabled={credits < priceOf(listing)}
                    class="rounded-lg bg-[#304053] px-3 py-1 disabled:opacity-50"
                  >
                    <small class="text-white">BUY</small>
                  </button>
                {/if}
              </div>
            </div>
          {:else}
            <small class="text-[0.6rem] text-[#00000088]">
              Nothing listed yet. Items sell between players, never back to the contract — selling
              drops to the contract would turn free loot into minted credits.
            </small>
          {/each}
        {/if}
      </section>
    </div>
    {/if}
  {/if}
</AppShell>

<Modal
  title={selling ? `Sell ${CREDIT_ITEMS[selling].label}` : 'Sell'}
  open={selling !== null}
  onclose={() => (selling = null)}
>
  {#if selling}
    {@const owned = itemCount(CREDIT_ITEMS[selling].id)}
    <div class="flex flex-col gap-3">
      <label class="flex flex-col gap-1">
        <small>Quantity (you have {owned})</small>
        <input
          type="number"
          min="1"
          max={owned}
          bind:value={sellQty}
          class="rounded-lg border-2 border-gray-300 px-3 py-2 outline-none focus:border-[#2d3c53]"
        />
      </label>
      <label class="flex flex-col gap-1">
        <small>Price each, in credits</small>
        <input
          type="number"
          min="0.001"
          step="0.001"
          bind:value={sellPrice}
          class="rounded-lg border-2 border-gray-300 px-3 py-2 outline-none focus:border-[#2d3c53]"
        />
      </label>
      <div class="flex flex-row justify-between rounded-lg bg-[#a9c6e4] px-3 py-2">
        <small class="text-black">You receive</small>
        <small class="text-black">
          {formatCredits(sellQty * sellPrice * (1 - MARKET_RAKE_BPS / 10_000), 4)} ACR
        </small>
      </div>
      <button
        type="button"
        onclick={sell}
        disabled={sellQty < 1 || sellQty > owned || sellPrice <= 0}
        class="h-12 w-full rounded-lg bg-[#2d3c53] disabled:opacity-50"
      >
        <span class="text-white">LIST FOR SALE</span>
      </button>
    </div>
  {/if}
</Modal>
