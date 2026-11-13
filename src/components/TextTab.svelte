<script lang="ts">
  import { api } from "../lib/ipc";
  import { flash, ui } from "../lib/state.svelte";

  let text = $state("");

  const EN = "`qwertyuiop[]asdfghjkl;'zxcvbnm,./~QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?@#$^&";
  const RU = "ёйцукенгшщзхъфывапролджэячсмитьбю.ЁЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,\"№;:?";

  function swapLayout(s: string) {
    let ru = 0, en = 0;
    for (const ch of s) {
      if (/[а-яёА-ЯЁ]/.test(ch)) ru++;
      else if (/[a-zA-Z]/.test(ch)) en++;
    }
    const from = ru > en ? RU : EN;
    const to = ru > en ? EN : RU;
    return [...s].map((c) => {
      const idx = from.indexOf(c);
      return idx >= 0 ? to[idx] : c;
    }).join("");
  }

  const TR: Record<string, string> = {
    а: "a", б: "b", в: "v", г: "g", д: "d", е: "e", ё: "yo", ж: "zh", з: "z", и: "i", й: "y",
    к: "k", л: "l", м: "m", н: "n", о: "o", п: "p", р: "r", с: "s", т: "t", у: "u", ф: "f",
    х: "kh", ц: "ts", ч: "ch", ш: "sh", щ: "shch", ъ: "", ы: "y", ь: "", э: "e", ю: "yu", я: "ya",
  };

  function translit(s: string) {
    return [...s].map((c) => {
      const low = c.toLowerCase();
      const t = TR[low];
      if (t === undefined) return c;
      return c !== low && t ? t[0].toUpperCase() + t.slice(1) : t;
    }).join("");
  }

  function run(fn: (s: string) => string) {
    if (!text) {
      const latestText = ui.clips.find((c) => c.kind === "text");
      if (latestText) text = latestText.text;
    }
    try {
      text = fn(text);
    } catch {
      flash("copied", "Ошибка формата");
    }
  }

  function fromClip() {
    const latestText = ui.clips.find((c) => c.kind === "text");
    if (latestText) text = latestText.text;
  }

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

  function toBase64(s: string): string {
    const bytes = new TextEncoder().encode(s);
    let bin = "";
    for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
    return btoa(bin);
  }

  function fromBase64(s: string): string {
    const bin = atob(s.trim());
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  }

  const wordCount = $derived(text.trim() ? text.trim().split(/\s+/).length : 0);
  const lineCount = $derived(text ? text.split(/\r?\n/).length : 0);

  async function copyOut() {
    if (!text.trim()) return;
    await api.setClip(text).catch(() => {});
    flash("copied", "Скопировано");
  }
</script>

<div class="wrap">
  <div class="bar">
    <button class="act" onclick={() => run(swapLayout)}>Раскладка RU↔EN</button>
    <button class="act" onclick={() => run((s) => s.toUpperCase())}>ВЕРХНИЙ</button>
    <button class="act" onclick={() => run((s) => s.toLowerCase())}>нижний</button>
    <button class="act" onclick={() => run(translit)}>Транслит</button>
    <button class="act" onclick={() => run((s) => JSON.stringify(JSON.parse(s), null, 2))}>JSON +</button>
    <button class="act" onclick={() => run((s) => JSON.stringify(JSON.parse(s)))}>JSON −</button>
    <button class="act" onclick={() => run(toBase64)}>Base64 +</button>
    <button class="act" onclick={() => run(fromBase64)}>Base64 −</button>
    <button class="act" onclick={() => run((s) => encodeURIComponent(s))}>URL</button>
  </div>

  <div class="editor-card">
    <textarea
      bind:value={text}
      placeholder="Вставь или набери текст — либо нажми «Из буфера» ниже…"
      onpointerdown={onPointerDown}
      onfocus={onFocus}
      onblur={onBlur}
      onkeydown={onKey}
    ></textarea>
  </div>

  <div class="foot">
    <span class="mono cnt">{text.length} симв. · {wordCount} сл. · {lineCount} стр.</span>
    <div class="btns">
      <button class="fbtn" onclick={fromClip} disabled={!ui.clips.some((c) => c.kind === "text")}>Из буфера</button>
      <button class="fbtn" onclick={() => (text = "")}>Очистить</button>
      <button class="fbtn pri" onclick={copyOut} disabled={!text.trim()}>В буфер</button>
    </div>
  </div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
    min-height: 0;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    flex: none;
  }
  .act {
    height: 26px;
    padding: 0 11px;
    border-radius: 999px;
    background: var(--glass-bg);
    border: 1px solid var(--line);
    font-size: 11px;
    font-weight: 500;
    color: var(--muted);
    transition: background var(--t-fast), border-color var(--t-fast), color var(--t-fast);
  }
  .act:hover {
    color: var(--fg);
    background: var(--glass-bg-strong);
    border-color: rgba(var(--accent-rgb), 0.45);
  }
  .editor-card {
    flex: 1;
    min-height: 0;
    display: flex;
    border-radius: var(--r-m);
    background: rgba(20, 22, 28, 0.72);
    border: 1px solid var(--line);
    box-shadow: var(--shadow-soft), inset 0 1px 0 rgba(255, 255, 255, 0.04);
    transition: border-color var(--t-fast), box-shadow var(--t-fast);
  }
  .editor-card:focus-within {
    border-color: rgba(var(--accent-rgb), 0.5);
    box-shadow: 0 0 0 2px rgba(var(--accent-rgb), 0.14);
  }
  textarea {
    flex: 1;
    width: 100%;
    height: 100%;
    resize: none;
    background: transparent;
    border: 0;
    padding: 12px 14px;
    color: var(--fg);
    font: inherit;
    font-size: 12.5px;
    line-height: 1.55;
    outline: none;
  }
  textarea::placeholder {
    color: var(--muted);
  }
  .foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex: none;
  }
  .cnt {
    font-size: 11px;
    color: var(--muted);
  }
  .btns {
    display: flex;
    gap: 6px;
  }
  .fbtn {
    height: 29px;
    padding: 0 12px;
    border-radius: 999px;
    background: var(--glass-bg);
    border: 1px solid var(--line);
    font-size: 11.5px;
    font-weight: 500;
    color: var(--muted);
    transition: background var(--t-fast), border-color var(--t-fast), color var(--t-fast);
  }
  .fbtn:hover:not(:disabled) {
    color: var(--fg);
    background: var(--glass-bg-strong);
    border-color: var(--line-strong);
  }
  .fbtn:disabled {
    opacity: 0.38;
    cursor: default;
  }
  .fbtn.pri {
    background: rgba(var(--accent-rgb), 0.24);
    border-color: rgba(var(--accent-rgb), 0.55);
    color: #ffe3d1;
    font-weight: 600;
  }
  .fbtn.pri:hover:not(:disabled) {
    background: rgba(var(--accent-rgb), 0.36);
    border-color: rgba(var(--accent-rgb), 0.85);
    color: #ffffff;
  }
</style>
