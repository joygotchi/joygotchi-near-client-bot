<script lang="ts">
  import Footer from '#lib/components/Footer.svelte';
  import Header from '#lib/components/Header.svelte';
</script>

<svelte:head>
  <title>JOYGOTCHI · Space</title>
</svelte:head>

<Header />

<div class="screen relative h-full w-full bg-[#e5f2f8] md:w-[400px]">
  <div class="flex h-[60vh] flex-col items-center justify-center gap-2 px-6">
    <img width="60" height="60" src="/assets/icon/planet.svg" alt="" />
    <span class="text-lg font-semibold text-black">Coming soon</span>
    <small class="text-center text-[#00000088]">
      Seasonal missions land here once the AI Lab economy settles.
    </small>
  </div>
</div>

<Footer />
