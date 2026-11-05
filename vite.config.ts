import adapter from '@sveltejs/adapter-vercel';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [
    tailwindcss(),
    sveltekit({
      adapter: adapter()
    })
  ],
  server: {
    host: true,
    // The Telegram Mini App is served through a tunnel in dev (ngrok / cloudflared).
    allowedHosts: true
  }
});
