<script lang="ts">
  import { page } from '$app/state';
  import { haptic } from '#lib/telegram';

  const buttons = [
    { src: '/assets/button/home.png', title: 'home', url: '/' },
    { src: '/assets/button/mining.png', title: 'mining', url: '/mining' },
    { src: '/assets/button/attack.png', title: 'battle', url: '/battle' },
    { src: '/assets/button/petlist.png', title: 'pets', url: '/mint' },
    { src: '/assets/button/house.png', title: 'AI lab', url: '/stake' },
    { src: '/assets/button/training.png', title: 'missions', url: '/mission' }
  ];

  const active = (url: string) =>
    url === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(url);
</script>

<div class="fix-header sticky bottom-0 w-full bg-[#e5f2f8]">
  <img
    width="200"
    height="108"
    class="h-[108px] w-full"
    src="/assets/background/frame_bottom.png"
    alt=""
  />
  <nav class="absolute top-2 left-0 flex w-full justify-center">
    <div class="flex w-full flex-row items-center justify-between gap-1 px-2">
      {#each buttons as button (button.url)}
        <a href={button.url} title={button.title} onclick={() => haptic()}>
          <img
            width="60"
            height="60"
            class={active(button.url)
              ? 'h-[70px] w-[70px]'
              : 'h-[56px] w-[56px] transition-all delay-100 hover:h-[70px] hover:w-[70px]'}
            src={button.src}
            alt={button.title}
          />
        </a>
      {/each}
    </div>
  </nav>
</div>
