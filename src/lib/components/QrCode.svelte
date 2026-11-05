<script lang="ts">
  type Props = { value: string; size?: number };
  let { value, size = 160 }: Props = $props();

  let svg = $state('');
  let failed = $state(false);

  $effect(() => {
    let cancelled = false;
    const text = value;

    (async () => {
      try {
        const QRCode = (await import('qrcode')).default;
        const markup = await QRCode.toString(text, {
          type: 'svg',
          margin: 1,
          width: size,
          color: { dark: '#2d3c53ff', light: '#ffffffff' }
        });
        if (!cancelled) {
          svg = markup;
          failed = false;
        }
      } catch {
        if (!cancelled) failed = true;
      }
    })();

    return () => {
      cancelled = true;
    };
  });
</script>

<div
  class="flex items-center justify-center rounded-lg bg-white p-2"
  style="width: {size + 16}px; height: {size + 16}px"
>
  {#if svg}
    <!-- Generated locally by the `qrcode` package from `value`. -->
    {@html svg}
  {:else if failed}
    <small class="text-center text-[0.65rem] break-all text-[#2d3c53]">{value}</small>
  {/if}
</div>
