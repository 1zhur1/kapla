<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, on, type BrowserNavEvt } from "../lib/ipc";
  import { flash, ui } from "../lib/state.svelte";

  const PRESETS = [
    { label: "ChatGPT", url: "https://chatgpt.com/" },
    { label: "DeepSeek", url: "https://chat.deepseek.com/" },
    { label: "Claude", url: "https://claude.ai/" },
    { label: "Perplexity", url: "https://www.perplexity.ai/" },
    { label: "Gemini", url: "https://gemini.google.com/" },
    { label: "Google", url: "https://www.google.com/" },
  ];

  let slotEl: HTMLDivElement;
  let address = $state(ui.browserUrl);
  let loading = $state(false);
  let ready = $state(false);
  let errText = $state<string | null>(null);

  function saveUrl(u: string) {
    ui.browserUrl = u;
    try {
      localStorage.setItem("kapla.browserUrl", JSON.stringify(u));
    } catch {}
  }

  function isPresetActive(url: string) {
    try {
      const a = new URL(ui.browserUrl).hostname.replace(/^www\./, "");
      const b = new URL(url).hostname.replace(/^www\./, "");
      return a === b;
    } catch {
      return false;
    }
  }

  async function syncBounds(targetUrl: string | null, fallbackUrl: string | null = null) {
    if (!slotEl) return;
    const r = slotEl.getBoundingClientRect();
    if (r.width < 100 || r.height < 100) return;
    try {
      const cur = await api.browserOpen(targetUrl, fallbackUrl, r.left, r.top, r.width, r.height);
      ready = true;
      errText = null;
      if (cur && cur.startsWith("http")) {
        address = cur;
        saveUrl(cur);
      }
    } catch (e) {
      errText = typeof e === "string" ? e : "Не удалось открыть мини-браузер";
    }
  }

  async function openPreset(url: string) {
    saveUrl(url);
    address = url;
    try {
      const cur = await api.browserGo(url);
      address = cur;
    } catch {
      await syncBounds(url);
    }
  }

  async function go(e?: Event) {
    e?.preventDefault();
    ui.inputMode = false;
    if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
    const q = address.trim();
    if (!q) return;
    try {
      const next = await api.browserGo(q);
      address = next;
      saveUrl(next);
    } catch {
      await syncBounds(q);
    }
  }

  let blurTimer: number | undefined;

  function onFocusInput(e?: PointerEvent | FocusEvent) {
    clearTimeout(blurTimer);
    ui.inputMode = true;
    api.beginInput().catch(() => {});
    const el = e?.currentTarget as HTMLInputElement | null;
    if (e && e.type === "pointerdown") {
      el?.focus();
    } else if (e && e.type === "focus") {
      el?.select();
    }
  }

  function onBlurInput() {
    clearTimeout(blurTimer);
    blurTimer = window.setTimeout(() => {
      if (!document.hasFocus() || !document.activeElement?.closest("input, textarea, [contenteditable]")) {
        ui.inputMode = false;
        api.endInput().catch(() => {});
      }
    }, 50);
  }

  onMount(() => {
    let unsub: (() => void) | undefined;
    let ro: ResizeObserver | undefined;

    (async () => {
      unsub = await on<BrowserNavEvt>("browser_nav", (evt) => {
        loading = evt.loading;
        if (evt.url && evt.url.startsWith("http")) {
          address = evt.url;
          saveUrl(evt.url);
        }
      });
    })();

    // Ждём раскрытия капли (~180 мс), чтобы дочерний WebView2 лёг точно в рамку.
    // Передаём null как явный URL, чтобы при повторном открытии живая сессия не перезагружалась!
    const openTimer = setTimeout(() => {
      syncBounds(null, ui.browserUrl);
      if (slotEl) {
        ro = new ResizeObserver(() => {
          if (ready) syncBounds(null);
        });
        ro.observe(slotEl);
      }
    }, 180);

    return () => {
      clearTimeout(openTimer);
      ro?.disconnect();
      unsub?.();
    };
  });

  onDestroy(() => {
    api.browserHide().catch(() => {});
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="bwrap"
  onpointerdown={(e) => {
    if (!(e.target as HTMLElement | null)?.closest("input, button, a, form")) {
      api.browserFocus().catch(() => {});
    }
  }}
>
  <div class="top-row">
    <div class="presets">
      {#each PRESETS as p}
        <button
          class="pchip"
          class:active={isPresetActive(p.url)}
          onclick={() => openPreset(p.url)}
        >
          {p.label}
        </button>
      {/each}
    </div>
    <div class="nav-btns">
      <button class="nbtn" onclick={() => api.browserNav("back")} title="Назад">
        <Icon name="arrowLeft" size={14} />
      </button>
      <button class="nbtn" onclick={() => api.browserNav("forward")} title="Вперёд">
        <Icon name="arrowRight" size={14} />
      </button>
      <button class="nbtn" class:spin={loading} onclick={() => api.browserNav("reload")} title="Обновить">
        <Icon name="refresh" size={14} />
      </button>
      <button
        class="nbtn"
        onclick={() => {
          api.browserOpenExternal(ui.browserUrl).catch(() => {});
          flash("copied", "Открыто в браузере");
        }}
        title="Открыть в основном браузере"
      >
        <Icon name="external" size={14} />
      </button>
    </div>
  </div>

  <form class="addr-row" onsubmit={go}>
    <Icon name="globe" size={14} />
    <input
      type="text"
      bind:value={address}
      placeholder="Введи сайт или быстрый запрос в Google и нажми Enter…"
      onpointerdown={onFocusInput}
      onfocus={onFocusInput}
      onblur={onBlurInput}
    />
    <button class="go-btn" type="submit">Открыть</button>
  </form>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="slot" bind:this={slotEl} onpointerdown={() => api.browserFocus().catch(() => {})}>
    {#if loading}
      <div class="load-bar"></div>
    {/if}
    {#if errText}
      <div class="slot-msg err">{errText}</div>
    {:else if !ready}
      <div class="slot-msg">Открываю встроенный браузер…</div>
    {/if}
  </div>
</div>

<style>
  .bwrap {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
  }
  .top-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .pchip {
    padding: 5px 12px;
    border-radius: 999px;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08);
    font-size: 11.5px;
    font-weight: 500;
    color: var(--muted);
    transition: all 140ms var(--ease);
  }
  .pchip:hover {
    color: var(--text);
    border-color: var(--line-2);
    background: var(--glass-bg-strong);
  }
  .pchip.active {
    background: rgba(255, 255, 255, 0.14);
    border-color: rgba(255, 255, 255, 0.28);
    color: var(--text);
  }
  .nav-btns {
    display: flex;
    gap: 4px;
    flex: none;
  }
  .nbtn {
    width: 28px;
    height: 28px;
    border-radius: 9px;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .nbtn:hover {
    color: var(--text);
    background: var(--glass-bg-strong);
  }
  .nbtn.spin {
    color: rgb(var(--accent-rgb));
  }
  .addr-row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 5px 0 12px;
    border-radius: 999px;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08);
    color: var(--muted);
  }
  .addr-row:focus-within {
    border-color: rgba(var(--accent-rgb), 0.55);
    color: var(--text);
  }
  .addr-row input {
    flex: 1;
    min-width: 0;
    background: none;
    border: 0;
    outline: none;
    font-size: 12px;
    color: var(--text);
  }
  .go-btn {
    height: 24px;
    padding: 0 10px;
    border-radius: 6px;
    background: var(--ink-3);
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--text);
  }
  .go-btn:hover {
    border-color: rgba(var(--accent-rgb), 0.5);
    color: rgb(var(--accent-rgb));
  }
  .slot {
    position: relative;
    flex: 1;
    min-height: 0;
    border-radius: 14px;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: var(--surface-sheen);
    overflow: hidden;
    display: grid;
    place-items: center;
  }
  .load-bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: linear-gradient(90deg, transparent, rgb(var(--accent-rgb)), transparent);
    animation: bLoad 1s ease-in-out infinite;
    z-index: 2;
  }
  @keyframes bLoad {
    0% {
      transform: translateX(-60%);
    }
    100% {
      transform: translateX(60%);
    }
  }
  .slot-msg {
    font-size: 12px;
    color: var(--faint);
  }
  .slot-msg.err {
    color: var(--danger);
  }
</style>
