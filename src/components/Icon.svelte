<script lang="ts">
  import { iconFills, icons, type IconName } from "../lib/icons";
  let {
    name,
    fill = false,
    size = 18,
    duotone = true,
  }: { name: IconName; fill?: boolean; size?: number; duotone?: boolean } = $props();

  const duoPath = $derived(!fill && duotone ? iconFills[name] : undefined);
</script>

<svg
  class="i"
  class:fill
  viewBox="0 0 24 24"
  width={size}
  height={size}
  style:width="{size}px"
  style:height="{size}px"
  aria-hidden="true"
>
  {#if duoPath}
    <path class="duo" d={duoPath} />
  {/if}
  <path d={icons[name]} />
</svg>

<style>
  .duo {
    fill: currentColor;
    stroke: none;
    opacity: 0.22;
    transition: opacity 180ms cubic-bezier(0.16, 1, 0.3, 1);
  }
</style>
