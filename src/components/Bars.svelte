<script lang="ts">
  let { playing = true, height = 14 }: { playing?: boolean; height?: number } = $props();
</script>

<!-- Пять «капель»-столбиков, подпрыгивающих в своём ритме -->
<span class="bars" class:paused={!playing} style:height="{height}px" aria-hidden="true">
  {#each [0.82, 1.13, 0.67, 0.97, 0.74] as d, i}
    <i style:animation-duration="{d}s" style:animation-delay="{-i * 0.17}s"></i>
  {/each}
</span>

<style>
  .bars {
    display: inline-flex;
    align-items: center;
    gap: 2.5px;
  }
  i {
    width: 3px;
    height: 100%;
    border-radius: 3px;
    background: rgb(var(--accent-rgb));
    transform-origin: center;
    animation: hop 1s ease-in-out infinite alternate;
  }
  .paused i {
    animation-play-state: paused;
    transform: scaleY(0.25);
    opacity: 0.5;
  }
  @keyframes hop {
    0% {
      transform: scaleY(0.22);
    }
    55% {
      transform: scaleY(1);
    }
    100% {
      transform: scaleY(0.45);
    }
  }
</style>
