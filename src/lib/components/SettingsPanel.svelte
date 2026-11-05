<script lang="ts">
  import { EXPLORER_URL } from '#lib/config';
  import { formatCredits } from '#lib/economy/params';
  import { disconnectWallet, wallet } from '#lib/near/wallet.svelte';
  import { liveCredits, player } from '#lib/stores/player.svelte';
  import { openLink, telegramStartParam } from '#lib/telegram';
  import ConnectWallet from './ConnectWallet.svelte';
  import CopyField from './CopyField.svelte';

  let sound = $state(true);

  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  const credits = $derived(player.credits + liveCredits(now));
  const referral = $derived(telegramStartParam() ?? '—');

  async function logout() {
    await disconnectWallet();
    location.assign('/');
  }
</script>

<div class="flex h-full w-full flex-col gap-3">
  {#if wallet.accountId}
    <CopyField label="Wallet address" value={wallet.accountId} />

    <div class="flex flex-col gap-2">
      <div
        class="flex flex-row items-center justify-between rounded-lg border-2 border-[#304053] p-2 text-black"
      >
        <span>Wallet</span>
        <span class="text-sm">{wallet.walletId ?? '—'}</span>
      </div>
      <div
        class="flex flex-row items-center justify-between rounded-lg border-2 border-[#304053] p-2 text-black"
      >
        <span>Invite code</span>
        <span class="text-sm">{referral}</span>
      </div>
    </div>

    <div class="flex flex-col rounded-lg border-2 border-[#304053] p-2 text-black">
      <span>Balance</span>
      <div class="mt-3 flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-1">
          <img width="16" height="16" class="mt-1" src="/assets/item/coin.png" alt="" />
          <span class="text-lg">$JOY</span>
        </div>
        <span>{player.joy.toLocaleString('en-US')}</span>
      </div>
      <div class="mt-1 flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-1">
          <img width="16" height="16" class="mt-1" src="/assets/item/credit_card.png" alt="" />
          <span class="text-lg">AI credit</span>
        </div>
        <span>{formatCredits(credits)}</span>
      </div>
      <div class="mt-1 flex flex-row items-center justify-between">
        <div class="flex flex-row items-center gap-1">
          <img width="16" height="16" class="mt-1" src="/assets/icon/star.svg" alt="" />
          <span class="text-lg">Stake weight</span>
        </div>
        <span>{player.weight.toLocaleString('en-US')}</span>
      </div>
    </div>

    <button
      type="button"
      onclick={logout}
      class="mt-2 h-14 w-full rounded-lg bg-[#304053] hover:bg-[#304053]/85"
    >
      <span class="text-xl">LOG OUT</span>
    </button>
  {:else}
    <div class="mt-4 rounded-lg border-2 border-[#304053] p-4">
      <ConnectWallet />
    </div>
  {/if}

  <div class="mt-3 flex flex-col gap-2">
    <button
      type="button"
      class="flex cursor-pointer flex-row items-center gap-1 text-black"
      onclick={() => (sound = !sound)}
    >
      <img
        width="30"
        height="30"
        src={sound ? '/assets/icon/sound.svg' : '/assets/icon/disable-sound.svg'}
        alt=""
      />
      <span class="ml-1 -mt-1 text-lg">Sound</span>
    </button>

    <button
      type="button"
      class="flex cursor-pointer flex-row items-center gap-1 text-black"
      onclick={() => openLink('https://t.me/joygotchi_bot')}
    >
      <img width="28" height="28" src="/assets/icon/telegram.svg" alt="" />
      <span class="ml-1 -mt-1 text-lg">Telegram</span>
    </button>

    <button
      type="button"
      class="flex cursor-pointer flex-row items-center gap-1 text-black"
      onclick={() => openLink(`${EXPLORER_URL}/address/${wallet.accountId ?? ''}`)}
    >
      <img width="26" height="26" src="/assets/icon/docs.svg" alt="" />
      <span class="ml-2 -mt-1 text-lg">Explorer</span>
    </button>
  </div>
  <div class="clear"></div>
</div>
