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
  <div class="search-bar-row">
    <form class="addr-row" onsubmit={go}>
      <span class="s-ico">
        <Icon name="globe" size={14} />
      </span>
      <input
        type="text"
        bind:value={address}
        placeholder="Поиск или URL сайта…"
        onpointerdown={onFocusInput}
        onfocus={onFocusInput}
        onblur={onBlurInput}
      />
      <button class="go-btn" type="submit">Открыть</button>
    </form>

    <div class="nav-btns">
      <button class="nbtn" onclick={() => api.browserNav("back")} title="Назад">
        <Icon name="arrowLeft" size={13} />
      </button>
      <button class="nbtn" onclick={() => api.browserNav("forward")} title="Вперёд">
        <Icon name="arrowRight" size={13} />
      </button>
      <button class="nbtn" class:spin={loading} onclick={() => api.browserNav("reload")} title="Обновить">
        <Icon name="refresh" size={13} />
      </button>
      <button
        class="nbtn"
        onclick={() => {
          api.browserOpenExternal(ui.browserUrl).catch(() => {});
          flash("copied", "Открыто в браузере");
        }}
        title="Открыть в основном браузере"
      >
        <Icon name="external" size={13} />
      </button>
    </div>
  </div>

  <div class="seg-bar">
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
    min-height: 0;
  }

  .search-bar-row {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: none;
  }

  .addr-row {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 5px 0 13px;
    border-radius: 999px;
    background: rgba(20, 22, 28, 0.78);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
    color: var(--muted);
    transition: border-color var(--t-fast), box-shadow var(--t-fast);
  }
  .addr-row:focus-within {
    border-color: rgba(var(--accent-rgb), 0.52);
    box-shadow: 0 0 0 2px rgba(var(--accent-rgb), 0.14);
    color: var(--fg);
  }
  .s-ico {
    display: grid;
    place-items: center;
    opacity: 0.72;
    flex: none;
  }
  .addr-row input {
    flex: 1;
    min-width: 0;
    background: none;
    border: 0;
    outline: none;
    font-size: 12px;
    color: var(--fg);
  }
  .addr-row input::placeholder {
    color: var(--muted);
  }

  .go-btn {
    height: 24px;
    padding: 0 11px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.1);
    font-size: 11px;
    font-weight: 500;
    color: var(--fg);
    transition: background var(--t-fast), border-color var(--t-fast), color var(--t-fast);
  }
  .go-btn:hover {
    background: rgba(var(--accent-rgb), 0.22);
    border-color: rgba(var(--accent-rgb), 0.5);
    color: #fff;
  }

  .nav-btns {
    display: flex;
    gap: 4px;
    flex: none;
  }
  .nbtn {
    width: 30px;
    height: 30px;
    border-radius: 999px;
    background: rgba(20, 22, 28, 0.76);
    border: 1px solid var(--line);
    display: grid;
    place-items: center;
    color: var(--muted);
    transition: background var(--t-fast), color var(--t-fast), border-color var(--t-fast);
  }
  .nbtn:hover {
    color: var(--fg);
    background: var(--glass-bg-strong);
    border-color: var(--line-strong);
  }
  .nbtn.spin {
    color: rgb(var(--accent-rgb));
  }

  /* Unified segmented AI preset bar */
  .seg-bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 3px;
    border-radius: 999px;
    background: rgba(18, 20, 26, 0.76);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
  }
  .pchip {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 10px;
    border-radius: 999px;
    border: 1px solid transparent;
    font-size: 11.5px;
    font-weight: 500;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: background var(--t-fast), color var(--t-fast), border-color var(--t-fast), box-shadow var(--t-fast);
  }
  .pchip:hover {
    color: var(--fg);
    background: rgba(255, 255, 255, 0.05);
  }
  .pchip.active {
    background: rgba(54, 58, 68, 0.92);
    border-color: rgba(255, 255, 255, 0.14);
    color: #ffffff;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35), inset 0 1px 0 rgba(255, 255, 255, 0.12);
  }

  .slot {
    position: relative;
    flex: 1;
    min-height: 0;
    border-radius: var(--r-m);
    background: rgba(18, 20, 26, 0.8);
    border: 1px solid var(--line);
    box-shadow: var(--shadow-soft);
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
