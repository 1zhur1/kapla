<script lang="ts">
  import { cubicOut, quintOut } from "svelte/easing";
  import Icon from "./Icon.svelte";
  import HomeTab from "./HomeTab.svelte";
  import ClipboardTab from "./ClipboardTab.svelte";
  import ShelfTab from "./ShelfTab.svelte";
  import TextTab from "./TextTab.svelte";
  import NoteTab from "./NoteTab.svelte";
  import BrowserTab from "./BrowserTab.svelte";
  import ControlsTab from "./ControlsTab.svelte";
  import ImageEditor from "./ImageEditor.svelte";
  import { ui, savePrefs, type Tab } from "../lib/state.svelte";
  import { TAB_META, tabIndex, themeOf } from "../lib/theme";

  let {
    timerLeft = null,
    onStartTimer,
    onStopTimer,
    onClose,
  }: {
    timerLeft: number | null;
    onStartTimer: (sec: number) => void;
    onStopTimer: () => void;
    onClose: () => void;
  } = $props();

  const th = $derived(themeOf(ui.tab));
  const timeStr = $derived(
    new Date(ui.now).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" }),
  );
  const dateStr = $derived(
    new Date(ui.now).toLocaleDateString("ru-RU", { weekday: "short", day: "numeric", month: "short" }),
  );

  // ── Направление перелистывания вкладок ──
  let dir = $state(1);
  let lastIdx = tabIndex(ui.tab);
  function go(t: Tab) {
    if (t === ui.tab) return;
    const i = tabIndex(t);
    dir = i >= lastIdx ? 1 : -1;
    lastIdx = i;
    ui.tab = t;
  }
  $effect(() => {
    const i = tabIndex(ui.tab);
    if (i !== lastIdx) {
      dir = i > lastIdx ? 1 : -1;
      lastIdx = i;
    }
  });

  function tabIn(_node: Element, { d }: { d: number }) {
    if (ui.tab === "browser") return { duration: 150, css: (t: number) => `opacity:${t}` };
    return {
      duration: 280,
      easing: quintOut,
      css: (t: number, u: number) =>
        `opacity:${t};transform:translateX(${u * d * 24}px) scale(${0.985 + 0.015 * t})`,
    };
  }
  function tabOut(_node: Element, { d }: { d: number }) {
    return {
      duration: 150,
      easing: cubicOut,
      css: (t: number, u: number) =>
        `opacity:${t};transform:translateX(${-u * d * 16}px) scale(${1 - 0.01 * u})`,
    };
  }
  function titleIn(_node: Element) {
    return {
      duration: 240,
      easing: quintOut,
      css: (t: number, u: number) => `opacity:${t};transform:translateY(${u * 5}px)`,
    };
  }

  // ── Плавный скользящий индикатор активного раздела ──
  let dockEl = $state<HTMLElement | null>(null);
  let glider = $state({ x: 4, w: 0, ready: false });

  $effect(() => {
    const tab = ui.tab;
    const _shelf = ui.shelf.length;
    void _shelf;
    if (!dockEl) return;
    requestAnimationFrame(() => {
      const btn = dockEl?.querySelector<HTMLButtonElement>(`button[data-tab="${tab}"]`);
      if (btn) {
        glider = { x: btn.offsetLeft, w: btn.offsetWidth, ready: true };
      }
    });
  });

  // ── Деликатный световой блик под стеклом за курсором ──
  let panelEl = $state<HTMLElement | null>(null);
  let spotEl = $state<HTMLElement | null>(null);
  function onPanelMove(e: PointerEvent) {
    if (!panelEl || !spotEl) return;
    const rc = panelEl.getBoundingClientRect();
    spotEl.style.transform = `translate(${e.clientX - rc.left - 190}px, ${e.clientY - rc.top - 190}px)`;
    spotEl.style.opacity = "1";
  }
  function onPanelLeave() {
    if (spotEl) spotEl.style.opacity = "0";
  }

  function onBarDblClick(e: MouseEvent) {
    if ((e.target as HTMLElement)?.closest("button, a, input")) return;
    ui.tiny = true;
    ui.pinned = false;
    ui.open = false;
    savePrefs();
  }
</script>

{#if ui.editorClip}
  <ImageEditor clip={ui.editorClip} onClose={() => (ui.editorClip = null)} />
{:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="panel"
    bind:this={panelEl}
    onpointermove={onPanelMove}
    onpointerleave={onPanelLeave}
  >
    <div class="spot" bind:this={spotEl} aria-hidden="true"></div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <header class="bar" ondblclick={onBarDblClick} title="Двойной клик — свернуть в мини-каплю">
      <div class="brand">
        <span class="logo" aria-hidden="true">
          <svg class="logo-svg" viewBox="0 0 24 24" fill="none">
            <path
              d="M12 2.5C12 2.5 5.5 10.2 5.5 15a6.5 6.5 0 0 0 13 0c0-4.8-6.5-12.5-6.5-12.5Z"
              stroke="url(#kaplaDropG)"
              stroke-width="1.8"
              stroke-linejoin="round"
            />
            <path
              d="M9.6 14.8a2.7 2.7 0 0 0 2.4 2.8"
              stroke="rgba(255,255,255,0.65)"
              stroke-width="1.4"
              stroke-linecap="round"
            />
            <defs>
              <linearGradient id="kaplaDropG" x1="12" y1="2.5" x2="12" y2="21.5" gradientUnits="userSpaceOnUse">
                <stop offset="0%" stop-color="#ffd5b8" />
                <stop offset="55%" stop-color="#ff8a47" />
                <stop offset="100%" stop-color="#ff622d" />
              </linearGradient>
            </defs>
          </svg>
        </span>
        <div class="title-slot">
          {#key ui.tab}
            <div class="title" in:titleIn>
              <span class="name">{th.label}</span>
              <span class="hint">{th.hint}</span>
            </div>
          {/key}
        </div>
      </div>

      <nav class="dock" bind:this={dockEl} aria-label="Разделы">
        {#if glider.ready}
          <span
            class="glider"
            style:transform="translateX({glider.x}px)"
            style:width="{glider.w}px"
            aria-hidden="true"
          ></span>
        {/if}
        {#each TAB_META as t (t.id)}
          <button
            class="dk"
            class:active={ui.tab === t.id}
            data-tab={t.id}
            onclick={() => go(t.id)}
            aria-label={t.label}
            title={t.label}
          >
            <span class="dk-ic"><Icon name={t.icon} size={15} /></span>
            {#if t.id === "shelf" && ui.shelf.length > 0}
              <span class="badge mono">{ui.shelf.length}</span>
            {/if}
          </button>
        {/each}
      </nav>

      <div class="right">
        <div class="status" title={dateStr}>
          <span class="mono st-time">{timeStr}</span>
          {#if ui.sys.battery !== null}
            <span class="st-sep"></span>
            <span class="st-bat" class:chg={ui.sys.charging} class:low={ui.sys.battery <= 20 && !ui.sys.charging}>
              <span class="bat-body"><span class="bat-lvl" style:width="{ui.sys.battery}%"></span></span>
              <span class="mono">{ui.sys.battery}%</span>
            </span>
          {/if}
        </div>
        <button
          class="hbtn"
          class:on={ui.pinned}
          onclick={() => (ui.pinned = !ui.pinned)}
          title={ui.pinned ? "Открепить панель" : "Закрепить панель открытой"}
        >
          <Icon name="pin" size={14} />
        </button>
        <button class="hbtn close" onclick={onClose} title="Свернуть">
          <Icon name="close" size={14} />
        </button>
      </div>
    </header>

    <main class="content">
      {#key ui.tab}
        <div class="tab-view" in:tabIn={{ d: dir }} out:tabOut={{ d: dir }}>
          {#if ui.tab === "home"}
            <HomeTab onTab={(t) => go(t)} />
          {:else if ui.tab === "clipboard"}
            <ClipboardTab />
          {:else if ui.tab === "shelf"}
            <ShelfTab />
          {:else if ui.tab === "text"}
            <TextTab />
          {:else if ui.tab === "note"}
            <NoteTab {timerLeft} {onStartTimer} {onStopTimer} />
          {:else if ui.tab === "browser"}
            <BrowserTab />
          {:else}
            <ControlsTab />
          {/if}
        </div>
      {/key}
    </main>
  </div>
{/if}

<style>
  .panel {
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 13px 18px 20px;
    gap: 12px;
    isolation: isolate;
  }

  /* ── Нейтральный стеклянный блик за курсором ── */
  .spot {
    position: absolute;
    left: 0;
    top: 0;
    width: 380px;
    height: 380px;
    border-radius: 50%;
    pointer-events: none;
    z-index: 0;
    opacity: 0;
    background: radial-gradient(
      circle,
      rgba(255, 255, 255, 0.065) 0%,
      rgba(var(--accent-rgb), 0.03) 40%,
      transparent 70%
    );
    transition: opacity 320ms var(--ease);
    will-change: transform;
  }

  /* ── Строгая шапка без наложений ── */
  .bar {
    position: relative;
    z-index: 3;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 12px;
    animation: barIn 320ms var(--ease) both;
  }
  @keyframes barIn {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }

  .brand {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .logo {
    position: relative;
    width: 30px;
    height: 30px;
    flex: none;
    display: grid;
    place-items: center;
    filter: drop-shadow(0 0 8px rgba(var(--accent-rgb), 0.55));
    transition: transform 260ms var(--ease-spring), filter 260ms var(--ease);
  }
  .logo-svg {
    width: 24px;
    height: 24px;
  }
  .brand:hover .logo {
    transform: scale(1.08);
    filter: drop-shadow(0 0 12px rgba(var(--accent-rgb), 0.78));
  }

  .title-slot {
    display: grid;
    min-width: 0;
  }
  .title {
    grid-area: 1 / 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.18;
  }
  .name {
    font-size: 14px;
    font-weight: 650;
    letter-spacing: -0.015em;
    color: var(--text);
  }
  .hint {
    font-size: 10.5px;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Капсульный док в стиле hero-preview ── */
  .dock {
    position: relative;
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 4px 5px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.12),
      0 8px 20px rgba(0, 0, 0, 0.28);
    backdrop-filter: var(--glass-blur);
  }
  .glider {
    position: absolute;
    left: 0;
    top: 4px;
    bottom: 4px;
    border-radius: 999px;
    pointer-events: none;
    background: linear-gradient(180deg, #ffffff 0%, #e7ebf3 100%);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.35);
    transition:
      transform 280ms var(--ease-spring),
      width 220ms var(--ease);
  }

  .dk {
    position: relative;
    z-index: 1;
    width: 36px;
    height: 30px;
    border-radius: 999px;
    display: grid;
    place-items: center;
    color: rgba(235, 240, 250, 0.72);
    transition: color 160ms var(--ease), transform 160ms var(--ease);
  }
  .dk-ic {
    display: grid;
    place-items: center;
    transition: transform 200ms var(--ease-spring);
  }
  .dk:hover {
    color: #ffffff;
  }
  .dk:hover .dk-ic {
    transform: translateY(-1px);
  }
  .dk.active {
    color: #15171d;
  }
  .dk.active .dk-ic {
    color: #15171d;
  }
  .dk.active :global(svg.i .duo) {
    opacity: 0.26;
  }

  .badge {
    position: absolute;
    top: 1px;
    right: 2px;
    min-width: 14px;
    height: 14px;
    padding: 0 3.5px;
    border-radius: 99px;
    display: grid;
    place-items: center;
    font-size: 9px;
    font-weight: 700;
    color: #10131a;
    background: rgb(var(--accent-rgb));
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.35);
  }

  /* ── Правый системный блок (никогда не залезает на док) ── */
  .right {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 10px;
    height: 30px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    font-size: 11px;
    white-space: nowrap;
  }
  .st-time {
    font-weight: 600;
    font-size: 12px;
    color: var(--text);
  }
  .st-sep {
    width: 1px;
    height: 12px;
    background: rgba(255, 255, 255, 0.14);
  }
  .st-bat {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-size: 10.5px;
  }
  .bat-body {
    position: relative;
    width: 16px;
    height: 8.5px;
    border-radius: 2.5px;
    border: 1.2px solid currentColor;
    padding: 1px;
    display: flex;
  }
  .bat-body::after {
    content: "";
    position: absolute;
    right: -3px;
    top: 2px;
    width: 1.5px;
    height: 2.5px;
    border-radius: 0 1px 1px 0;
    background: currentColor;
  }
  .bat-lvl {
    height: 100%;
    border-radius: 1px;
    background: currentColor;
  }
  .st-bat.chg {
    color: var(--ok);
  }
  .st-bat.low {
    color: var(--danger);
  }

  .hbtn {
    width: 30px;
    height: 30px;
    flex: none;
    border-radius: 10px;
    display: grid;
    place-items: center;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.055);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.12);
    transition:
      background 160ms var(--ease),
      color 160ms var(--ease),
      border-color 160ms var(--ease),
      transform 180ms var(--ease);
  }
  .hbtn:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.12);
    border-color: var(--line-2);
  }
  .hbtn.on {
    color: rgb(var(--accent-rgb));
    border-color: rgba(var(--accent-rgb), 0.5);
    background: rgba(var(--accent-rgb), 0.14);
  }
  .hbtn.on :global(svg.i) {
    transform: rotate(-35deg);
  }
  .hbtn.close:hover {
    color: #fff;
    background: rgba(255, 90, 100, 0.55);
    border-color: rgba(255, 130, 140, 0.55);
  }

  /* ── Контент ── */
  .content {
    position: relative;
    z-index: 1;
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
  }
  .tab-view {
    grid-area: 1 / 1;
    height: 100%;
    min-height: 0;
    min-width: 0;
    will-change: transform, opacity;
  }
</style>
