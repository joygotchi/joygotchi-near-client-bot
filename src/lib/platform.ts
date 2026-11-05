import { browser } from '$app/env';
import { page } from '$app/state';

/**
 * Small platform helpers that work in any browser.
 *
 * This replaces the Telegram WebApp bridge the app used to depend on. Haptics
 * come from the Vibration API, which Android Chrome supports and everything
 * else quietly ignores — so the call sites stay meaningful on a phone and are
 * free on a desktop.
 */

export function haptic(strength: 'light' | 'medium' | 'heavy' = 'light'): void {
  if (!browser) return;
  const pattern = { light: 10, medium: 25, heavy: 45 }[strength];
  navigator.vibrate?.(pattern);
}

export function notify(type: 'success' | 'error' | 'warning'): void {
  if (!browser) return;
  const pattern = { success: [12, 40, 12], error: [40, 60, 40], warning: [25, 50] }[type];
  navigator.vibrate?.(pattern);
}

/** Open a link in a new tab, without handing the opener to the target page. */
export function openExternal(url: string): void {
  if (!browser) return;
  window.open(url, '_blank', 'noopener,noreferrer');
}

/**
 * Referral code from the URL: `?ref=alice`.
 *
 * Previously this came from Telegram's `start_param`. A query parameter works
 * on every platform and is shareable as a plain link.
 */
export function referralCode(): string | null {
  const code = page.url?.searchParams.get('ref');
  return code && code.length <= 64 ? code : null;
}

/** A shareable invite link for the current account. */
export function inviteLink(accountId: string | null): string | null {
  if (!browser || !accountId) return null;
  return `${location.origin}/?ref=${encodeURIComponent(accountId)}`;
}

/** Copy text, reporting whether it worked so callers can fall back. */
export async function copyText(text: string): Promise<boolean> {
  if (!browser) return false;
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
