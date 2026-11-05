declare global {
  namespace App {
    // interface Error {}
    // interface Locals {}
    // interface PageData {}
    // interface Platform {}
  }

  interface Navigator {
    /** Not in every browser; callers must use optional access. */
    vibrate?(pattern: number | number[]): boolean;
  }
}

export {};
