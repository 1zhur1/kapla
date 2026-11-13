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

  <textarea
    bind:value={text}
    placeholder="Вставь или набери текст — либо нажми «Из буфера» ниже…"
    onpointerdown={onPointerDown}
    onfocus={onFocus}
    onblur={onBlur}
    onkeydown={onKey}
  ></textarea>

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
    gap: 8px;
    height: 100%;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .act {
    padding: 5px 9px;
    border-radius: 8px;
    background: var(--ink-2);
    border: 1px solid var(--line);
    font-size: 11.5px;
    color: var(--muted);
    transition: all 140ms var(--ease);
  }
  .act:hover {
    color: var(--text);
    background: var(--ink-3);
    border-color: rgba(var(--accent-rgb), 0.45);
  }
  textarea {
    flex: 1;
    width: 100%;
    resize: none;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: var(--surface-sheen);
    border-radius: var(--r-m);
    padding: 10px 12px;
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
    line-height: 1.5;
    outline: none;
  }
  textarea:focus {
    border-color: rgba(var(--accent-rgb), 0.55);
  }
  .foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .cnt {
    font-size: 11px;
    color: var(--faint);
  }
  .btns {
    display: flex;
    gap: 6px;
  }
  .fbtn {
    padding: 5px 11px;
    border-radius: 8px;
    background: var(--ink-2);
    border: 1px solid var(--line);
    font-size: 12px;
    color: var(--muted);
  }
  .fbtn:hover:not(:disabled) {
    color: var(--text);
    background: var(--ink-3);
  }
  .fbtn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .fbtn.pri {
    background: linear-gradient(155deg, rgba(255, 255, 255, 0.28) 0%, transparent 45%), rgb(var(--accent-rgb));
    border-color: transparent;
    color: #0b0e14;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.45), 0 4px 14px rgba(var(--accent-rgb), 0.28);
    font-weight: 600;
  }
</style>
