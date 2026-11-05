import { browser } from '$app/env';
import type { NearConnector, NearWalletBase } from '@hot-labs/near-connect';
import { GAME_CONTRACT, NETWORK } from '#lib/config';

/** Wallet ids from the NEAR Connect manifest that we surface as one-tap buttons. */
export const FEATURED_WALLETS = [
  { id: 'hot-wallet', label: 'HOT Wallet', icon: '/assets/icon/near.svg' },
  { id: 'meteor-wallet', label: 'Meteor Wallet', icon: '/assets/icon/planet.svg' }
] as const;

export type FeaturedWalletId = (typeof FEATURED_WALLETS)[number]['id'];

export const wallet = $state({
  /** `null` until a wallet is connected. */
  accountId: null as string | null,
  /** Manifest id of the connected wallet, e.g. `hot-wallet`. */
  walletId: null as string | null,
  /** `true` once we know whether a previous session can be restored. */
  ready: false,
  connecting: false,
  error: null as string | null
});

let connector: NearConnector | null = null;
let initPromise: Promise<NearConnector | null> | null = null;

/**
 * Create the connector and restore a previous session. Safe to call from
 * several components — the work happens once.
 */
export function initWallet(): Promise<NearConnector | null> {
  if (!browser) return Promise.resolve(null);
  initPromise ??= createConnector();
  return initPromise;
}

async function createConnector(): Promise<NearConnector | null> {
  try {
    const { NearConnector } = await import('@hot-labs/near-connect');

    connector = new NearConnector({
      network: NETWORK,
      autoConnect: true,
      footerBranding: {
        heading: 'JOYGOTCHI',
        link: 'https://t.me/joygotchi_bot',
        linkText: 'Play on Telegram'
      }
    });

    connector.on('wallet:signIn', (payload) => {
      if (!payload.success) return;
      wallet.accountId = payload.accounts[0]?.accountId ?? null;
      wallet.walletId = payload.wallet?.manifest?.id ?? null;
      wallet.error = null;
    });

    connector.on('wallet:signOut', () => {
      wallet.accountId = null;
      wallet.walletId = null;
    });

    // Restore an existing session, if any.
    try {
      const { wallet: active, accounts } = await connector.getConnectedWallet();
      if (accounts.length > 0) {
        wallet.accountId = accounts[0].accountId;
        wallet.walletId = active.manifest.id;
      }
    } catch {
      // Not signed in — that's the normal first-visit path.
    }

    return connector;
  } catch (error) {
    wallet.error = asMessage(error);
    return null;
  } finally {
    wallet.ready = true;
  }
}

/**
 * Open the NEAR Connect selector, or connect straight to one wallet.
 * `walletId` is a manifest id such as `hot-wallet` or `meteor-wallet`.
 */
export async function connectWallet(walletId?: string): Promise<string | null> {
  const instance = await initWallet();
  if (!instance) return null;

  wallet.connecting = true;
  wallet.error = null;
  try {
    const connected = await instance.connect(walletId ? { walletId } : undefined);
    const accounts = await connected.getAccounts();
    wallet.accountId = accounts[0]?.accountId ?? null;
    wallet.walletId = connected.manifest.id;
    return wallet.accountId;
  } catch (error) {
    wallet.error = asMessage(error);
    return null;
  } finally {
    wallet.connecting = false;
  }
}

export async function disconnectWallet(): Promise<void> {
  const instance = await initWallet();
  if (!instance) return;
  try {
    await instance.disconnect();
  } finally {
    wallet.accountId = null;
    wallet.walletId = null;
  }
}

/** The connected wallet, or `null` when nobody is signed in. */
export async function getActiveWallet(): Promise<NearWalletBase | null> {
  const instance = await initWallet();
  if (!instance) return null;
  try {
    return await instance.wallet();
  } catch {
    return null;
  }
}

/**
 * Ask the wallet for a function-call access key limited to the game contract.
 * With that key in place the wallet stops prompting for every non-payable call,
 * which is what makes feeding / attacking feel instant.
 */
export async function requestGameplayKey(publicKey: string): Promise<void> {
  const instance = await initWallet();
  if (!instance) return;
  await instance.connect({
    walletId: wallet.walletId ?? undefined,
    addFunctionCallKey: {
      contractId: GAME_CONTRACT,
      publicKey,
      allowMethods: { anyMethod: true }
    }
  });
}

export function asMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  try {
    return JSON.stringify(error);
  } catch {
    return 'Unknown error';
  }
}
