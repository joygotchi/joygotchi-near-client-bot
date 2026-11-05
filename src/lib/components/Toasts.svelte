<script lang="ts">
  import { dismissToast, toasts } from '#lib/stores/toast.svelte';

  const style = {
    success: 'bg-[#d4edda] border-[#c3e6cb] text-[#155724]',
    error: 'bg-[#f8d7da] border-[#ff0000] text-[#ff0000]',
    info: 'bg-[#97b5d5] border-[#e5f2f8] text-[#2d3c53]'
  } as const;

  const icon = {
    success: '/assets/icon/success.svg',
    error: '/assets/icon/error.svg',
    info: '/assets/icon/loading.svg'
  } as const;
</script>

<div class="pointer-events-none fixed top-4 left-1/2 z-[60] flex -translate-x-1/2 flex-col gap-2">
  {#each toasts.items as toast (toast.id)}
    <button
      type="button"
      onclick={() => dismissToast(toast.id)}
      class="pointer-events-auto flex min-w-[240px] max-w-[92vw] flex-row items-center gap-2 rounded-lg border-2 px-3 py-2 shadow-sm {style[
        toast.kind
      ]}"
    >
      <img width="20" height="20" src={icon[toast.kind]} alt={toast.kind} />
      <small class="text-left text-sm font-semibold">{toast.message}</small>
    </button>
  {/each}
</div>
