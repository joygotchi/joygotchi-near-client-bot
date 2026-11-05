<script lang="ts">
  import type { Snippet } from 'svelte';
  import Header from './Header.svelte';
  import NavBar from './NavBar.svelte';

  type Props = {
    title: string;
    /** Extra row under the header bar. */
    banner?: Snippet;
    children: Snippet;
  };

  let { title, banner, children }: Props = $props();
</script>

<svelte:head>
  <title>{title}</title>
</svelte:head>

<div class="flex min-h-screen w-full flex-col bg-[#b8e3f8]">
  <Header {banner} />

  <!--
    One content container for every page. It is a narrow column on a phone and
    widens in two steps, so a desktop browser gets a real layout instead of a
    400px strip. Bottom padding clears the mobile dock.
  -->
  <main
    class="mx-auto flex w-full max-w-[1040px] flex-1 flex-col px-2 pb-28 md:px-4 md:pb-8"
  >
    <div class="flex-1 rounded-lg bg-[#e5f2f8] p-2 md:p-4">
      {@render children()}
    </div>
  </main>

  <NavBar />
</div>
