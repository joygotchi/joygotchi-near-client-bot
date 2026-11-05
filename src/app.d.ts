declare global {
  namespace App {
    // interface Error {}
    // interface Locals {}
    // interface PageData {}
    // interface Platform {}
  }

  interface TelegramWebApp {
    ready(): void;
    expand(): void;
    close(): void;
    initData: string;
    initDataUnsafe: {
      user?: { id: number; username?: string; first_name?: string; language_code?: string };
      start_param?: string;
    };
    colorScheme: 'light' | 'dark';
    platform: string;
    version: string;
    setHeaderColor?(color: string): void;
    setBackgroundColor?(color: string): void;
    disableVerticalSwipes?(): void;
    HapticFeedback?: {
      impactOccurred(style: 'light' | 'medium' | 'heavy' | 'rigid' | 'soft'): void;
      notificationOccurred(type: 'error' | 'success' | 'warning'): void;
    };
    openLink?(url: string, options?: { try_instant_view?: boolean }): void;
    openTelegramLink?(url: string): void;
  }

  interface Window {
    Telegram?: { WebApp?: TelegramWebApp };
  }
}

export {};
