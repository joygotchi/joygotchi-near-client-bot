import { browser } from '$app/env';

/**
 * Thin wrapper around the Telegram WebApp bridge. We read `window.Telegram`
 * directly instead of pulling in a SDK, so nothing breaks during SSR.
 */
export function tg(): TelegramWebApp | null {
  if (!browser) return null;
  return window.Telegram?.WebApp ?? null;
}

export const isTelegram = () => tg() !== null;

export function initTelegram(): void {
  const app = tg();
  if (!app) return;
  app.ready();
  app.expand();
  app.setHeaderColor?.('#2d3c53');
  app.setBackgroundColor?.('#b8e3f8');
  app.disableVerticalSwipes?.();
}

export function haptic(style: 'light' | 'medium' | 'heavy' = 'light'): void {
  tg()?.HapticFeedback?.impactOccurred(style);
}

export function notify(type: 'success' | 'error' | 'warning'): void {
  tg()?.HapticFeedback?.notificationOccurred(type);
}

/** Telegram user id, when the app runs inside Telegram. */
export const telegramUserId = () => tg()?.initDataUnsafe?.user?.id ?? null;

/** `?startapp=` payload, used for referral codes. */
export const telegramStartParam = () => tg()?.initDataUnsafe?.start_param ?? null;

export function openLink(url: string): void {
  const app = tg();
  if (app?.openLink) app.openLink(url);
  else if (browser) window.open(url, '_blank', 'noopener,noreferrer');
}
