<script lang="ts">
  import Icon from "./Icon.svelte";
  import { api, type Clip } from "../lib/ipc";
  import { ui, flash, openImageEditor, timeAgo } from "../lib/state.svelte";

  let query = $state("");
  let onlyPinned = $state(false);
  let kindFilter = $state<"all" | "image" | "text">("all");

  const list = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return ui.clips
      .filter(
        (c) =>
          (!onlyPinned || c.pinned) &&
          (kindFilter === "all" || c.kind === kindFilter) &&
          (!q || c.text.toLowerCase().includes(q)),
      )
      .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.ts - a.ts);
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
      (e.currentTarget as HTMLInputElement).blur();
    }
  }

  async function paste(c: Clip) {
    try {
      await api.paste(c.id);
      ui.open = false;
      ui.pinned = false;
      flash("copied", "Вставлено");
    } catch {
      flash("copied", "Ошибка вставки");
    }
  }
</script>

<div class="wrap">
  <div class="toolbar">
    <label class="search">
      <Icon name="search" size={15} />
      <input
        type="text"
        bind:value={query}
        placeholder="Поиск в истории буфера…"
        onpointerdown={onPointerDown}
        onfocus={onFocus}
        onblur={onBlur}
        onkeydown={onKey}
      />
    </label>
    <div class="seg">
      <button class="sbtn" class:on={kindFilter === "all"} onclick={() => (kindFilter = "all")}>Все</button>
      <button class="sbtn" class:on={kindFilter === "image"} onclick={() => (kindFilter = "image")}>Картинки</button>
      <button class="sbtn" class:on={kindFilter === "text"} onclick={() => (kindFilter = "text")}>Текст</button>
    </div>
    <button class="fbtn" class:on={onlyPinned} onclick={() => (onlyPinned = !onlyPinned)} title="Только закреплённые">
      <Icon name="pin" size={14} />
    </button>
    <button class="fbtn danger" onclick={() => api.clearClips()} title="Удалить незакреплённые">
      <Icon name="trash" size={14} />
    </button>
  </div>

  {#if list.length === 0}
    <div class="empty">
      <Icon name="clipboard" size={28} />
      <p>{query ? "Ничего не нашлось" : "История буфера пока пуста. Скопируйте любой текст (Ctrl+C)."}</p>
    </div>
  {:else}
    <div class="list">
      {#each list as clip (clip.id)}
        <div class="item" class:pinned={clip.pinned}>
          <button class="body" onclick={() => paste(clip)} title="Клик — вставить туда, где стоял курсор">
            {#if clip.kind === "image" && clip.preview}
              <div class="img-row">
                <img class="thumb" src={clip.preview} alt="" />
                <div class="img-info">
                  <span class="txt">{clip.text}</span>
                  <span class="meta mono">{timeAgo(clip.ts, ui.now)} · картинка</span>
                </div>
              </div>
            {:else}
              <span class="txt">{clip.text}</span>
              <span class="meta mono">{timeAgo(clip.ts, ui.now)} · {clip.text.length} симв.</span>
            {/if}
          </button>
          <div class="acts">
            {#if clip.kind === "image"}
              <button
                class="ibtn ed"
                onclick={() => openImageEditor(clip)}
                title="Редактировать изображение"
              >
                <Icon name="pencil" size={14} />
              </button>
            {/if}
            <button
              class="ibtn"
              class:on={clip.pinned}
              onclick={() => api.pin(clip.id)}
              title={clip.pinned ? "Открепить" : "Закрепить"}
            >
              <Icon name="pin" size={14} />
            </button>
            <button class="ibtn del" onclick={() => api.remove(clip.id)} title="Удалить">
              <Icon name="trash" size={14} />
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
  }
  .toolbar {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.14);
    border-radius: var(--r-s);
    padding: 0 10px;
    height: 34px;
    color: var(--muted);
  }
  .search:focus-within {
    border-color: rgba(var(--accent-rgb), 0.55);
    color: var(--text);
  }
  .search input {
    flex: 1;
    background: none;
    border: 0;
    outline: none;
    font-size: 12.5px;
  }
  .fbtn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--r-s);
    background: var(--ink-2);
    border: 1px solid var(--line);
    box-shadow: var(--surface-sheen);
    font-size: 12px;
    color: var(--muted);
    transition: all 140ms var(--ease);
  }
  .seg {
    display: flex;
    gap: 2px;
    padding: 3px;
    height: 34px;
    border-radius: var(--r-s);
    background: var(--ink-2);
    border: 1px solid var(--line);
    box-shadow: var(--surface-sheen);
  }
  .sbtn {
    padding: 0 9px;
    border-radius: 7px;
    font-size: 11.5px;
    color: var(--muted);
    transition: background 140ms var(--ease), color 140ms var(--ease);
  }
  .sbtn:hover {
    color: var(--text);
  }
  .sbtn.on {
    background: var(--ink-3);
    color: rgb(var(--accent-rgb));
  }
  .fbtn:hover {
    color: var(--text);
    background: var(--ink-3);
  }
  .fbtn.on {
    color: rgb(var(--accent-rgb));
    border-color: rgba(var(--accent-rgb), 0.45);
    background: rgba(var(--accent-rgb), 0.1);
  }
  .fbtn.danger:hover {
    color: var(--danger);
    border-color: rgba(255, 95, 82, 0.35);
  }
  .list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-right: 4px;
  }
  .item {
    display: flex;
    align-items: stretch;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: var(--surface-sheen);
    border-radius: var(--r-s);
    min-width: 0;
    transition: border-color 140ms var(--ease), background 140ms var(--ease), transform 140ms var(--ease);
  }
  .item.pinned {
    border-color: rgba(var(--accent-rgb), 0.45);
    background: rgba(var(--accent-rgb), 0.06);
  }
  .item:hover {
    background: var(--glass-bg-strong);
    border-color: var(--line-2);
    transform: translateY(-1px);
  }
  .body {
    flex: 1;
    min-width: 0;
    text-align: left;
    padding: 9px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .txt {
    font-size: 12.5px;
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-word;
    overflow-wrap: anywhere;
  }
  .meta {
    font-size: 10.5px;
    color: var(--faint);
  }
  .img-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .thumb {
    width: 48px;
    height: 36px;
    object-fit: cover;
    border-radius: 6px;
    border: 1px solid var(--line);
    flex: none;
    background: var(--ink-4);
  }
  .img-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 6px;
  }
  .ibtn {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--faint);
  }
  .ibtn:hover {
    background: var(--ink-4);
    color: var(--text);
  }
  .ibtn.on {
    color: rgb(var(--accent-rgb));
  }
  .ibtn.ed:hover {
    color: rgb(var(--accent-rgb));
    background: rgba(var(--accent-rgb), 0.14);
  }
  .ibtn.del:hover {
    color: var(--danger);
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--faint);
    text-align: center;
    font-size: 12.5px;
  }
</style>
