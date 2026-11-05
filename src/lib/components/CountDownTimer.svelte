<script lang="ts">
  import { onDestroy } from 'svelte';

  type Props = {
    /** Seconds remaining. Restarting the countdown is as simple as changing it. */
    seconds: number;
    onElapsed?: () => void;
  };

  let { seconds, onElapsed }: Props = $props();

  let remaining = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  $effect(() => {
    const total = Math.max(0, seconds);
    const startedAt = Date.now();
    remaining = total;

    clearInterval(timer);
    timer = setInterval(() => {
      const left = total - (Date.now() - startedAt) / 1000;
      remaining = left > 0 ? left : 0;
      if (left <= 0) {
        clearInterval(timer);
        onElapsed?.();
      }
    }, 1000);

    return () => clearInterval(timer);
  });

  onDestroy(() => clearInterval(timer));

  const formatted = $derived(new Date(remaining * 1000).toISOString().slice(11, 19));
</script>

<span>{formatted}</span>
