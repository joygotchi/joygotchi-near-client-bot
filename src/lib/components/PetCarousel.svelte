<script lang="ts">
  import { petImage, type Pet } from '#lib/game/pets';
  import { haptic } from '#lib/platform';

  type Props = {
    pets: Pet[];
    index: number;
    onselect: (index: number) => void;
    /** Smaller sprite + name label, used on the battle screen. */
    battle?: boolean;
  };

  let { pets, index, onselect, battle = false }: Props = $props();

  const current = $derived(pets[index]);

  function step(delta: number) {
    if (pets.length === 0) return;
    haptic();
    onselect((index + delta + pets.length) % pets.length);
  }
</script>

{#if current}
  <div class="relative">
    <button type="button" aria-label="Previous pet" onclick={() => step(-1)}>
      <img class="arrow-left absolute" src="/assets/icon/arrow_left.png" alt="" />
    </button>

    <img
      class={battle ? 'pet -mt-10' : 'ml-6 w-[150px]'}
      src={petImage(current)}
      alt={current.name}
    />

    <button type="button" aria-label="Next pet" onclick={() => step(1)}>
      <img class="arrow-right absolute" src="/assets/icon/arrow_right.png" alt="" />
    </button>

    {#if battle}
      <div class="absolute top-[70%] left-[20%] text-black">
        <small>{current.name}</small>
      </div>
    {/if}
  </div>
{/if}
