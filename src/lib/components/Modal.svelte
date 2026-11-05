<script lang="ts">
  import type { Snippet } from 'svelte';

  type Props = {
    title: string;
    open: boolean;
    onclose: () => void;
    children: Snippet;
  };

  let { title, open, onclose, children }: Props = $props();
</script>

<svelte:window
  onkeydown={(event) => {
    if (open && event.key === 'Escape') onclose();
  }}
/>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center overflow-hidden px-3">
    <button
      type="button"
      aria-label="Close dialog"
      class="absolute inset-0 h-full w-full cursor-default bg-black/50"
      onclick={onclose}
    ></button>

    <div
      class="relative w-full max-w-[375px] rounded-lg bg-[#e5f2f8] pb-5"
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <div class="flex h-12 flex-row items-center justify-between rounded-t-lg bg-[#2d3c53] px-3">
        <span>{title}</span>
        <button type="button" onclick={onclose} aria-label="Close">
          <img width="35" height="35" src="/assets/icon/close.svg" alt="" />
        </button>
      </div>
      <div class="px-3 pt-4 text-black">
        {@render children()}
      </div>
    </div>
  </div>
{/if}
