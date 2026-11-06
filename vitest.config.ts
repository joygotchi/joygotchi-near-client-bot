import adapter from '@sveltejs/adapter-vercel';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

/**
 * Unit tests for the pure logic in `src/lib` — amounts, pet state, economy
 * formulas. These are the functions that decide what a player is shown and
 * charged, and all three client bugs found so far lived here rather than in a
 * component.
 *
 * The SvelteKit plugin is loaded so `$app/env/public` and friends resolve the
 * same way they do in the app. Stubbing them instead would mean the tests run
 * against a different module graph than production, which is how a passing
 * suite starts lying.
 */
export default defineConfig({
  plugins: [sveltekit({ adapter: adapter() })],
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts']
  }
});
