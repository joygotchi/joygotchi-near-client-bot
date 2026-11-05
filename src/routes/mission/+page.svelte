<script lang="ts">
  import AppShell from '#lib/components/AppShell.svelte';

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

<AppShell title="JOYGOTCHI · Missions">
  <div class="grid grid-cols-1 gap-3 md:grid-cols-2 lg:grid-cols-3">
    {#each missions as mission (mission.title)}
      <a
        href="/space"
        class="flex w-full flex-row items-center justify-between gap-2 rounded-lg border-2 p-3"
        style="border-color: {mission.color}; background-color: {mission.color}40"
      >
        <img width="40" height="40" src={mission.icon} alt="" />
        <div class="flex flex-1 flex-col gap-1 text-black">
          <span class="text-sm">{mission.title}</span>
          <small class="text-[0.6rem]">{mission.subtitle}</small>
        </div>
        <div class="flex flex-col items-end gap-2">
          <div
            class="flex items-center justify-center rounded-full border px-2"
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
      </a>
    {/each}
  </div>
</AppShell>
