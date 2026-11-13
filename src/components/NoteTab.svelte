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
      <span>Быстрая заметка</span>
      <div class="nacts">
        <button class="nbtn" onclick={copyNote} disabled={!note.trim()}>Копировать</button>
        <button class="nbtn" onclick={() => (note = "")} disabled={!note}>Очистить</button>
      </div>
    </div>
    <textarea
      bind:value={note}
      placeholder="Идея, ссылка или список дел — просто начни печатать…"
      onpointerdown={onPointerDown}
      onfocus={onFocus}
      onblur={onBlur}
      onkeydown={onKey}
    ></textarea>
  </section>

  <section class="card timer-box">
    <div class="head">
      <span>Таймер</span>
      {#if timerLeft !== null}
        <span class="mono live">идёт</span>
      {/if}
    </div>
    <div class="display mono">
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
      <button class="pbtn" type="submit">Старт</button>
    </form>
    {#if timerLeft !== null}
      <button class="stop" onclick={onStopTimer}>Сбросить таймер</button>
    {:else}
      <p class="tip">Во время отсчёта таймер виден прямо в свёрнутой капле.</p>
    {/if}
  </section>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1.4fr 1fr;
    gap: 12px;
    height: 100%;
  }
  .card {
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    border-radius: var(--r-m);
    box-shadow: var(--surface-sheen);
    padding: 14px;
    display: flex;
    flex-direction: column;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 10px;
  }
  .nacts {
    display: flex;
    gap: 5px;
  }
  .nbtn {
    font-size: 10.5px;
    padding: 2px 7px;
    border-radius: 6px;
    background: var(--ink-3);
    border: 1px solid var(--line);
    color: var(--muted);
  }
  .nbtn:hover:not(:disabled) {
    color: var(--text);
    border-color: rgba(var(--accent-rgb), 0.45);
  }
  .nbtn:disabled {
    opacity: 0.35;
  }
  .custom-tmr {
    display: flex;
    gap: 6px;
  }
  .custom-tmr input {
    width: 68px;
    height: 32px;
    border-radius: var(--r-s);
    background: var(--ink-3);
    border: 1px solid var(--line);
    text-align: center;
    font-size: 12px;
    outline: none;
  }
  .custom-tmr input:focus {
    border-color: rgba(var(--accent-rgb), 0.55);
  }
  .custom-tmr .pbtn {
    flex: 1;
    height: 32px;
  }
  .live {
    color: rgb(var(--accent-rgb));
    font-size: 11px;
  }
  textarea {
    flex: 1;
    width: 100%;
    resize: none;
    background: var(--ink-3);
    border: 1px solid var(--line);
    border-radius: var(--r-s);
    padding: 10px 12px;
    color: var(--text);
    font: inherit;
    font-size: 13px;
    line-height: 1.5;
    outline: none;
  }
  textarea:focus {
    border-color: rgba(var(--accent-rgb), 0.55);
  }
  .timer-box {
    justify-content: space-between;
  }
  .display {
    font-size: 40px;
    font-weight: 600;
    text-align: center;
    letter-spacing: -0.03em;
    margin: 6px 0;
  }
  .presets {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
  }
  .pbtn {
    height: 34px;
    border-radius: var(--r-s);
    background: var(--ink-3);
    border: 1px solid var(--line);
    font-size: 12px;
    transition: all 140ms var(--ease);
  }
  .pbtn:hover {
    border-color: rgba(var(--accent-rgb), 0.5);
    color: rgb(var(--accent-rgb));
  }
  .stop {
    height: 34px;
    border-radius: var(--r-s);
    background: rgba(255, 95, 82, 0.14);
    color: var(--danger);
    font-size: 12px;
    font-weight: 500;
  }
  .tip {
    margin: 0;
    font-size: 11.5px;
    color: var(--faint);
    text-align: center;
    line-height: 1.4;
  }
</style>
