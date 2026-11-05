<script lang="ts">
  import { toastSuccess } from '#lib/stores/toast.svelte';

  type Props = { label: string; value: string };
  let { label, value }: Props = $props();

  async function copy() {
    try {
      await navigator.clipboard.writeText(value);
      toastSuccess(`${label} copied`);
    } catch {
      toastSuccess('Select and copy manually');
    }
  }
</script>

<button
  type="button"
  onclick={copy}
  class="flex w-full flex-row items-center justify-between gap-2 rounded-lg border-2 border-[#304053] p-2 text-left"
>
  <div class="flex min-w-0 flex-col">
    <small class="text-[#00000075]">{label}</small>
    <span class="truncate text-sm text-black">{value}</span>
  </div>
  <img width="16" height="16" src="/assets/icon/copy.svg" alt="Copy" />
</button>
