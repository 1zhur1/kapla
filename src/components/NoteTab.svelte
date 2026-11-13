<script lang="ts">
  import { api } from "../lib/ipc";
  import { flash, ui, fmtTime } from "../lib/state.svelte";

  let {
    timerLeft = null,
    onStartTimer,
    onStopTimer,
  }: {
    timerLeft: number | null;
    onStartTimer: (sec: number) => void;
    onStopTimer: () => void;
  } = $props();

  let note = $state(localStorage.getItem("kapla.note") ?? "");
  let customMin = $state("3");

  function startCustomTimer(e: Event) {
    e.preventDefault();
    const m = Math.max(1, Math.min(240, Math.round(Number(customMin) || 0)));
    if (m > 0) onStartTimer(m * 60);
  }

  async function copyNote() {
    if (!note.trim()) return;
    await api.setClip(note).catch(() => {});
    flash("copied", "Заметка скопирована");
  }

  $effect(() => {
    localStorage.setItem("kapla.note", note);
  });

  let blurTimer: number | undefined;

  function onPointerDown(e: PointerEvent) {
    clearTimeout(blurTimer);
    ui.inputMode = true;
    api.beginInput().catch(() => {});
    (e.currentTarget as HTMLElement | null)?.focus();
  }

  function onFocus() {
    clearTimeout(blurTimer);
    ui.inputMode = true;
    api.beginInput().catch(() => {});
  }

  function onBlur() {
    clearTimeout(blurTimer);
    blurTimer = window.setTimeout(() => {
      if (!document.hasFocus() || !document.activeElement?.closest("input, textarea, [contenteditable]")) {
        ui.inputMode = false;
        api.endInput().catch(() => {});
      }
    }, 50);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      (e.currentTarget as HTMLTextAreaElement).blur();
    }
  }
</script>

<div class="grid">
  <section class="card pad">
    <div class="head">
      <span class="htitle">Быстрая заметка</span>
      <div class="nacts">
        <button class="nbtn" onclick={copyNote} disabled={!note.trim()}>Копировать</button>
        <button class="nbtn" onclick={() => (note = "")} disabled={!note}>Очистить</button>
      </div>
    </div>
    <textarea
      bind:value={note}
      placeholder="Идея, ссылка или список дел — просто начните печатать…"
      onpointerdown={onPointerDown}
      onfocus={onFocus}
      onblur={onBlur}
      onkeydown={onKey}
    ></textarea>
  </section>

  <section class="card timer-box" class:running={timerLeft !== null}>
    <div class="head">
      <span class="htitle">Таймер</span>
      {#if timerLeft !== null}
        <span class="mono live">идёт</span>
      {/if}
    </div>
    <div class="display mono" class:running={timerLeft !== null}>
      {timerLeft !== null ? fmtTime(timerLeft) : "00:00"}
    </div>
    <div class="presets">
      {#each [1, 5, 10, 25] as m}
        <button class="pbtn mono" onclick={() => onStartTimer(m * 60)}>{m} мин</button>
      {/each}
    </div>
    <form class="custom-tmr" onsubmit={startCustomTimer}>
      <input
        type="number"
        min="1"
        max="240"
        bind:value={customMin}
        placeholder="мин"
        onpointerdown={onPointerDown}
        onfocus={onFocus}
        onblur={onBlur}
      />
      <button class="start-cta" type="submit">Старт</button>
    </form>
    {#if timerLeft !== null}
      <button class="stop" onclick={onStopTimer}>Сбросить таймер</button>
    {:else}
      <p class="tip">Во время отсчёта таймер виден прямо в свёрнутой капле</p>
    {/if}
  </section>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1.38fr 1fr;
    gap: 10px;
    height: 100%;
    min-height: 0;
  }
  .card {
    background: var(--glass-bg);
    border: 1px solid var(--line);
    border-radius: var(--r-m);
    box-shadow: var(--shadow-soft), inset 0 1px 0 rgba(255, 255, 255, 0.05);
    padding: 12px 13px;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .timer-box.running {
    border-color: rgba(var(--accent-rgb), 0.45);
    box-shadow: 0 0 22px rgba(var(--accent-rgb), 0.14);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 9px;
    flex: none;
  }
  .htitle {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--fg);
  }
  .nacts {
    display: flex;
    gap: 5px;
  }
  .nbtn {
    height: 24px;
    font-size: 11px;
    padding: 0 9px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid var(--line);
    color: var(--muted);
    transition: background var(--t-fast), border-color var(--t-fast), color var(--t-fast);
  }
  .nbtn:hover:not(:disabled) {
    color: var(--fg);
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(var(--accent-rgb), 0.45);
  }
  .nbtn:disabled {
    opacity: 0.35;
  }
  textarea {
    flex: 1;
    width: 100%;
    resize: none;
    background: rgba(18, 20, 26, 0.65);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 10px 12px;
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    line-height: 1.55;
    outline: none;
  }
  textarea::placeholder {
    color: var(--muted);
  }
  textarea:focus {
    border-color: rgba(var(--accent-rgb), 0.5);
  }

  .timer-box {
    justify-content: space-between;
  }
  .live {
    padding: 2px 8px;
    border-radius: 999px;
    background: rgba(var(--accent-rgb), 0.16);
    border: 1px solid rgba(var(--accent-rgb), 0.4);
    color: #ffd5bc;
    font-size: 10px;
    font-weight: 600;
  }
  .display {
    font-size: 38px;
    font-weight: 600;
    text-align: center;
    letter-spacing: -0.03em;
    color: var(--fg);
    margin: 4px 0;
  }
  .display.running {
    color: rgb(var(--accent-rgb));
  }
  .presets {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 6px;
  }
  .pbtn {
    height: 31px;
    border-radius: 10px;
    background: rgba(18, 20, 26, 0.62);
    border: 1px solid var(--line);
    font-size: 11.5px;
    color: var(--fg);
    transition: border-color var(--t-fast), background var(--t-fast);
  }
  .pbtn:hover {
    border-color: rgba(var(--accent-rgb), 0.5);
    background: rgba(var(--accent-rgb), 0.12);
    color: #ffd5bc;
  }
  .custom-tmr {
    display: flex;
    gap: 6px;
  }
  .custom-tmr input {
    width: 68px;
    height: 30px;
    border-radius: 999px;
    background: rgba(18, 20, 26, 0.68);
    border: 1px solid var(--line);
    text-align: center;
    font-size: 12px;
    color: var(--fg);
    outline: none;
  }
  .custom-tmr input:focus {
    border-color: rgba(var(--accent-rgb), 0.55);
  }
  .start-cta {
    flex: 1;
    height: 30px;
    border-radius: 999px;
    background: rgba(var(--accent-rgb), 0.24);
    border: 1px solid rgba(var(--accent-rgb), 0.55);
    color: #ffe3d1;
    font-size: 11.5px;
    font-weight: 600;
    transition: background var(--t-fast), border-color var(--t-fast), color var(--t-fast);
  }
  .start-cta:hover {
    background: rgba(var(--accent-rgb), 0.36);
    border-color: rgba(var(--accent-rgb), 0.85);
    color: #fff;
  }
  .stop {
    height: 30px;
    border-radius: 999px;
    background: rgba(255, 95, 82, 0.14);
    border: 1px solid rgba(255, 95, 82, 0.34);
    color: #ff9c9c;
    font-size: 11.5px;
    font-weight: 500;
  }
  .tip {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
    text-align: center;
    line-height: 1.35;
  }
</style>
