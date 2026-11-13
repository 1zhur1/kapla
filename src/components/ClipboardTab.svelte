<script lang="ts">
  import Icon from "./Icon.svelte";
  import { api, type Clip } from "../lib/ipc";
  import { ui, flash, openImageEditor, timeAgo } from "../lib/state.svelte";

  let query = $state("");
  let onlyPinned = $state(false);
  let kindFilter = $state<"all" | "image" | "text">("all");
  let selectedId = $state<number | null>(null);

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

  const selected = $derived(list.find((c) => c.id === selectedId) ?? list[0] ?? null);

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

  async function copyOnly(c: Clip) {
    try {
      await api.setClip(c.text);
      flash("copied", "Скопировано");
    } catch {
      flash("copied", "Ошибка копирования");
    }
  }
</script>

<div class="wrap">
  <div class="toolbar">
    <label class="search">
      <Icon name="search" size={14} />
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
      <p>{query ? "Ничего не нашлось" : "История буфера пока пуста. Скопируйте текст или картинку (Ctrl+C)."}</p>
    </div>
  {:else}
    <div class="split">
      <!-- Левая колонка: список буфера -->
      <div class="card list-card">
        <div class="list">
          {#each list as clip (clip.id)}
            {@const isSel = selected?.id === clip.id}
            <div
              class="item"
              class:sel={isSel}
              class:pinned={clip.pinned}
              role="button"
              tabindex="0"
              onclick={() => (selectedId = clip.id)}
              ondblclick={() => paste(clip)}
              onkeydown={(e) => e.key === "Enter" && paste(clip)}
              title="Клик — предпросмотр · Двойной клик — вставить"
            >
              <span class="ic">
                {#if clip.kind === "image" && clip.preview}
                  <img class="mini-thumb" src={clip.preview} alt="" />
                {:else}
                  <Icon name={clip.pinned ? "pin" : "file"} size={14} />
                {/if}
              </span>
              <div class="it-meta">
                <span class="txt">{clip.text}</span>
                <span class="meta mono">{timeAgo(clip.ts, ui.now)}</span>
              </div>
              <button
                class="quick-paste"
                onclick={(e) => {
                  e.stopPropagation();
                  paste(clip);
                }}
                title="Вставить в активное окно"
              >
                <Icon name="copy" size={12} />
              </button>
            </div>
          {/each}
        </div>
      </div>

      <!-- Правая колонка: предпросмотр и панель инструментов редактора -->
      {#if selected}
        <div class="card preview-card">
          {#if selected.kind === "image" && selected.preview}
            <div class="img-stage">
              <img class="full-img" src={selected.preview} alt="" />
              <span class="img-tag mono">{timeAgo(selected.ts, ui.now)}</span>
            </div>

            <!-- Панель инструментов редактора в янтарной рамке (как на features-preview) -->
            <div class="ed-dock">
              <button class="ed-t" onclick={() => openImageEditor(selected, "crop")} title="Обрезать">
                <Icon name="crop" size={16} />
              </button>
              <button class="ed-t" onclick={() => openImageEditor(selected, "arrow")} title="Стрелка">
                <Icon name="arrowDiag" size={16} />
              </button>
              <button class="ed-t" onclick={() => openImageEditor(selected, "rect")} title="Рамка">
                <Icon name="square" size={16} />
              </button>
              <button class="ed-t active" onclick={() => openImageEditor(selected, "brush")} title="Открыть редактор">
                <Icon name="pencil" size={16} />
              </button>
              <button class="ed-t" onclick={() => openImageEditor(selected, "text")} title="Текст">
                <Icon name="text" size={16} />
              </button>
              <span class="ed-sep"></span>
              <button class="ed-paste" onclick={() => paste(selected)} title="Вставить картинку">
                Вставить
              </button>
            </div>
          {:else}
            <div class="txt-head">
              <span class="meta mono">
                {timeAgo(selected.ts, ui.now)} · {selected.text.length} симв.
              </span>
              <div class="txt-tools">
                <button
                  class="ibtn"
                  class:on={selected.pinned}
                  onclick={() => api.pin(selected.id)}
                  title={selected.pinned ? "Открепить" : "Закрепить"}
                >
                  <Icon name="pin" size={13} />
                </button>
                <button class="ibtn del" onclick={() => api.remove(selected.id)} title="Удалить">
                  <Icon name="trash" size={13} />
                </button>
              </div>
            </div>

            <div class="txt-body">{selected.text}</div>

            <div class="txt-foot">
              <button class="act-btn pri" onclick={() => paste(selected)}>
                <Icon name="clipboard" size={13} />
                <span>Вставить в окно</span>
              </button>
              <button class="act-btn" onclick={() => copyOnly(selected)}>
                <Icon name="copy" size={13} />
                <span>Копировать</span>
              </button>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
    min-height: 0;
  }
  .toolbar {
    display: flex;
    gap: 7px;
    align-items: center;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--glass-bg);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
    border-radius: 999px;
    padding: 0 12px;
    height: 32px;
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
    font-size: 12px;
  }
  .seg {
    display: flex;
    gap: 2px;
    padding: 3px;
    height: 32px;
    border-radius: 999px;
    background: var(--glass-bg);
    border: 1px solid var(--line);
  }
  .sbtn {
    padding: 0 10px;
    border-radius: 999px;
    font-size: 11px;
    color: var(--muted);
    transition: background 140ms var(--ease), color 140ms var(--ease);
  }
  .sbtn:hover {
    color: var(--text);
  }
  .sbtn.on {
    background: rgba(255, 255, 255, 0.14);
    color: var(--text);
    font-weight: 500;
  }
  .fbtn {
    width: 32px;
    height: 32px;
    border-radius: 999px;
    display: grid;
    place-items: center;
    background: var(--glass-bg);
    border: 1px solid var(--line);
    color: var(--muted);
  }
  .fbtn:hover {
    color: var(--text);
    background: var(--glass-bg-strong);
  }
  .fbtn.on {
    color: rgb(var(--accent-rgb));
    border-color: rgba(var(--accent-rgb), 0.48);
    background: rgba(var(--accent-rgb), 0.14);
  }
  .fbtn.danger:hover {
    color: var(--danger);
    border-color: rgba(255, 107, 107, 0.4);
  }

  /* ── 2-колонный макет (список + превью с инструментами) ── */
  .split {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 0.94fr 1.06fr;
    gap: 10px;
  }
  .card {
    border-radius: 16px;
    background: var(--glass-bg);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
    padding: 8px;
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-right: 2px;
  }
  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px 7px 11px;
    border-radius: 10px;
    border: 1px solid transparent;
    cursor: pointer;
    min-width: 0;
    transition: background 140ms var(--ease), border-color 140ms var(--ease);
  }
  .item:hover {
    background: rgba(255, 255, 255, 0.05);
  }
  .item.sel {
    background: rgba(255, 255, 255, 0.09);
    border-color: rgba(255, 255, 255, 0.12);
    box-shadow: inset 2.5px 0 0 rgb(var(--accent-rgb));
  }
  .item.pinned .ic {
    color: rgb(var(--accent-rgb));
  }
  .ic {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.05);
    flex: none;
    overflow: hidden;
  }
  .mini-thumb {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .it-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .txt {
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }
  .meta {
    font-size: 10px;
    color: var(--faint);
  }
  .quick-paste {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    color: var(--faint);
    opacity: 0;
    transition: opacity 120ms var(--ease), background 120ms var(--ease), color 120ms var(--ease);
    flex: none;
  }
  .item:hover .quick-paste,
  .item.sel .quick-paste {
    opacity: 1;
  }
  .quick-paste:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.14);
  }

  /* ── Правая панель превью ── */
  .preview-card {
    padding: 10px;
    gap: 9px;
  }
  .img-stage {
    position: relative;
    flex: 1;
    min-height: 0;
    border-radius: 11px;
    background: rgba(14, 16, 21, 0.72);
    border: 1px solid rgba(255, 255, 255, 0.09);
    display: grid;
    place-items: center;
    padding: 8px;
    overflow: hidden;
  }
  .full-img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: 6px;
  }
  .img-tag {
    position: absolute;
    right: 8px;
    bottom: 8px;
    padding: 2px 7px;
    border-radius: 99px;
    font-size: 10px;
    background: rgba(15, 17, 22, 0.78);
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: var(--muted);
  }

  /* Док-панель инструментов (features-preview) */
  .ed-dock {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 7px;
    border-radius: 13px;
    background: linear-gradient(180deg, rgba(46, 36, 32, 0.55), rgba(28, 25, 24, 0.75));
    border: 1px solid rgba(var(--accent-rgb), 0.45);
    box-shadow: 0 0 18px rgba(var(--accent-rgb), 0.14);
  }
  .ed-t {
    position: relative;
    width: 32px;
    height: 30px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .ed-t:hover {
    color: #fff;
    background: rgba(255, 255, 255, 0.08);
  }
  .ed-t.active {
    color: #fff;
    background: rgba(var(--accent-rgb), 0.28);
    border: 1px solid rgba(var(--accent-rgb), 0.55);
  }
  .ed-t.active::after {
    content: "";
    position: absolute;
    left: 7px;
    right: 7px;
    bottom: 2px;
    height: 2px;
    border-radius: 2px;
    background: rgb(var(--accent-rgb));
  }
  .ed-sep {
    flex: 1;
  }
  .ed-paste {
    height: 28px;
    padding: 0 12px;
    border-radius: 8px;
    background: rgb(var(--accent-rgb));
    color: #101217;
    font-size: 11.5px;
    font-weight: 600;
  }

  .txt-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .txt-tools {
    display: flex;
    gap: 4px;
  }
  .ibtn {
    width: 26px;
    height: 26px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.05);
  }
  .ibtn:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.12);
  }
  .ibtn.on {
    color: rgb(var(--accent-rgb));
  }
  .ibtn.del:hover {
    color: var(--danger);
  }
  .txt-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 10px;
    border-radius: 10px;
    background: rgba(14, 16, 21, 0.65);
    border: 1px solid rgba(255, 255, 255, 0.07);
    font-size: 12px;
    line-height: 1.5;
    color: var(--text);
    white-space: pre-wrap;
    word-break: break-word;
    user-select: text;
  }
  .txt-foot {
    display: flex;
    gap: 6px;
  }
  .act-btn {
    flex: 1;
    height: 31px;
    border-radius: 9px;
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid var(--line);
    font-size: 11.5px;
    color: var(--text);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .act-btn:hover {
    background: rgba(255, 255, 255, 0.13);
  }
  .act-btn.pri {
    background: rgb(var(--accent-rgb));
    border-color: transparent;
    color: #101217;
    font-weight: 600;
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
