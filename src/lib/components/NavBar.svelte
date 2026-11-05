<script lang="ts">
  import { page } from '$app/state';
  import { isActive, NAV_ITEMS } from '#lib/nav';
  import { haptic } from '#lib/platform';
</script>

<!--
  The mobile dock. On md+ the same destinations live in the header bar, so this
  is hidden rather than duplicated on screen.
-->
<nav
  class="fixed bottom-0 left-0 z-30 w-full border-t-2 border-[#2d3c53] bg-[#e5f2f8] pb-[env(safe-area-inset-bottom)] md:hidden"
  aria-label="Primary"
>
  <ul class="flex flex-row items-end justify-between px-1 py-1">
    {#each NAV_ITEMS as item (item.href)}
      {@const active = isActive(item.href, page.url.pathname)}
      <li class="flex-1">
        <a
          href={item.href}
          onclick={() => haptic()}
          aria-current={active ? 'page' : undefined}
          class="flex flex-col items-center gap-0.5 rounded-lg py-1 {active
            ? 'bg-[#a9c6e4]'
            : ''}"
        >
          <img
            width="34"
            height="34"
            class={active ? 'h-9 w-9' : 'h-8 w-8 opacity-75'}
            src={item.icon}
            alt=""
          />
          <small class="text-[0.5rem] text-[#2d3c53]">{item.label}</small>
        </a>
      </li>
    {/each}
  </ul>
</nav>
