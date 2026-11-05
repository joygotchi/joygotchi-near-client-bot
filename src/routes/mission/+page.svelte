<script lang="ts">
  import Footer from '#lib/components/Footer.svelte';
  import Header from '#lib/components/Header.svelte';

  const missions = [
    {
      title: 'Peacekeepers',
      subtitle: 'Battle · 4 weeks',
      state: 'Starting',
      reward: '45.000',
      icon: '/assets/icon/diamond.svg',
      color: '#6251A2'
    },
    {
      title: 'Gold rush',
      subtitle: 'Mining · 2 weeks',
      state: 'Closed',
      reward: '45.000',
      icon: '/assets/icon/gold.svg',
      color: '#EBC351'
    },
    {
      title: 'Lab assistants',
      subtitle: 'Stake 3 pets in the AI Lab',
      state: 'Open',
      reward: '12.000',
      icon: '/assets/icon/star.svg',
      color: '#C0C0C0'
    }
  ];
</script>

<svelte:head>
  <title>JOYGOTCHI · Missions</title>
</svelte:head>

<Header />

<div class="screen relative h-full w-full overflow-y-auto bg-[#e5f2f8] md:w-[400px]">
  <div class="mt-8 flex flex-col gap-3 px-2">
    {#each missions as mission (mission.title)}
      <a
        href="/space"
        class="flex w-full flex-row items-center justify-between rounded-lg border-2 py-2 pl-3"
        style="border-color: {mission.color}; background-color: {mission.color}40"
      >
        <img width="45" height="45" src={mission.icon} alt="" />
        <div class="flex flex-col gap-1 text-black">
          <span class="text-sm">{mission.title}</span>
          <small class="text-[0.6rem]">{mission.subtitle}</small>
        </div>
        <div class="flex flex-col gap-2">
          <div
            class="flex items-center justify-center rounded-full border px-2 text-center"
            style="border-color: {mission.color}"
          >
            <small style="color: {mission.color}">{mission.state}</small>
          </div>
          <div
            class="flex flex-row gap-1 rounded-full px-2 py-1"
            style="background-color: {mission.color}90"
          >
            <img width="12" height="12" src="/assets/item/coin.png" alt="" />
            <small class="text-black">{mission.reward}</small>
          </div>
        </div>
        <div class="mt-2 h-8 w-8">
          <img width="14" height="14" src="/assets/icon/arrow_right.png" alt="" />
        </div>
      </a>
    {/each}
  </div>
  <div class="clear"></div>
</div>

<Footer />
