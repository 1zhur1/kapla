<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, type ConvertProbe, type ConvertResult, type ShelfItem } from "../lib/ipc";
  import { ui, flash, fmtSize, saveShelf } from "../lib/state.svelte";

  let justDragged = false;

  let convProbe = $state<ConvertProbe | null>(null);
  let convTargetId = $state("");
  let convQuality = $state(88);
  let convMaxSide = $state<number | null>(null);
  let convBusy = $state(false);
  let convErr = $state<string | null>(null);
  let convLast = $state<ConvertResult | null>(null);

  const selectedTarget = $derived(
    convProbe?.targets.find((t) => t.id === convTargetId) ?? convProbe?.targets[0] ?? null,
  );

  onMount(() => {
    api.shelfPrepareDrop().catch(() => {});
  });

  function mergeItems(items: ShelfItem[]) {
    const existing = new Set(ui.shelf.map((s) => s.path));
    const fresh = items.filter((m) => !existing.has(m.path));
    if (fresh.length > 0) {
      ui.shelf = [...fresh, ...ui.shelf].slice(0, 80);
      saveShelf();
      flash("copied", `На полку: +${fresh.length}`);
    }
  }

  async function addFiles() {
    ui.pinned = true;
    try {
      const items = await api.pickFiles();
      if (items.length > 0) mergeItems(items);
    } catch {
      flash("copied", "Не удалось выбрать файлы");
    }
  }

  async function openItem(item: ShelfItem) {
    if (justDragged) {
      justDragged = false;
      return;
    }
    try {
      await api.openPath(item.path);
    } catch {
      flash("copied", "Файл недоступен");
    }
  }

  function startCardDrag(e: PointerEvent, item: ShelfItem) {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement)?.closest(".acts")) return;
    const sx = e.clientX;
    const sy = e.clientY;
    let started = false;

    const cleanup = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };

    const onMove = (ev: PointerEvent) => {
      if (started || (ev.buttons & 1) === 0) return;
      const dx = ev.clientX - sx;
      const dy = ev.clientY - sy;
      if (dx * dx + dy * dy < 36) return;
      started = true;
      justDragged = true;
      cleanup();
      api
        .shelfStartDrag(item.path)
        .then((dropped) => {
          if (dropped) flash("copied", "Перетащен");
        })
        .catch(() => {
          flash("copied", "Файл недоступен");
        })
        .finally(() => {
          setTimeout(() => {
            justDragged = false;
          }, 120);
        });
    };

    const onUp = () => cleanup();

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  }

  async function revealItem(item: ShelfItem) {
    try {
      await api.revealPath(item.path);
    } catch {
      flash("copied", "Не удалось открыть папку");
    }
  }

  async function copyFile(item: ShelfItem) {
    try {
      await api.shelfCopyFiles([item.path]);
      flash("copied", "Файл скопирован");
    } catch {
      flash("copied", "Файл недоступен");
    }
  }

  async function copyPath(item: ShelfItem) {
    await api.setClip(item.path).catch(() => {});
    flash("copied", "Путь скопирован");
  }

  function removeItem(path: string) {
    ui.shelf = ui.shelf.filter((s) => s.path !== path);
    if (convProbe?.path === path) closeConverter();
    saveShelf();
  }

  function clearAll() {
    ui.shelf = [];
    closeConverter();
    saveShelf();
  }

  function closeConverter() {
    convProbe = null;
    convTargetId = "";
    convErr = null;
    convLast = null;
  }

  async function openConverter(item: ShelfItem) {
    if (convProbe?.path === item.path) {
      closeConverter();
      return;
    }
    convErr = null;
    convLast = null;
    convMaxSide = null;
    try {
      const probe = await api.convertProbe(item.path);
      convProbe = probe;
      convTargetId = probe.targets[0]?.id ?? "";
    } catch (e) {
      flash("copied", typeof e === "string" ? e : "Файл недоступен");
    }
  }

  async function convertViaCanvas(
    srcPath: string,
    target: string,
    quality: number,
    maxSide: number | null,
  ): Promise<ConvertResult> {
    const dataUrl = await api.convertReadB64(srcPath);
    const img = await new Promise<HTMLImageElement>((res, rej) => {
      const el = new Image();
      el.onload = () => res(el);
      el.onerror = () => rej("Не удалось декодировать изображение");
      el.src = dataUrl;
    });

    let w = img.naturalWidth || 512;
    let h = img.naturalHeight || 512;
    const limit = target === "ico" ? (maxSide ?? 256) : maxSide;
    if (limit && (w > limit || h > limit)) {
      const k = Math.min(limit / w, limit / h);
      w = Math.max(1, Math.round(w * k));
      h = Math.max(1, Math.round(h * k));
    }

    const canvas = document.createElement("canvas");
    if (target === "ico") {
      const side = Math.max(w, h);
      canvas.width = side;
      canvas.height = side;
      const ctx = canvas.getContext("2d")!;
      ctx.drawImage(img, (side - w) / 2, (side - h) / 2, w, h);
    } else {
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext("2d")!;
      if (target === "jpg") {
        ctx.fillStyle = "#ffffff";
        ctx.fillRect(0, 0, w, h);
      }
      ctx.drawImage(img, 0, 0, w, h);
    }

    const mime =
      target === "webp"
        ? "image/webp"
        : target === "jpg"
          ? "image/jpeg"
          : "image/png";
    const blob = await new Promise<Blob>((res, rej) => {
      canvas.toBlob(
        (b) => (b ? res(b) : rej("Не удалось закодировать изображение")),
        mime,
        quality / 100,
      );
    });
    const bytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
    return await api.convertSaveCanvas(srcPath, target, bytes);
  }

  async function runConvert() {
    if (!convProbe || !selectedTarget || convBusy) return;
    convBusy = true;
    convErr = null;
    convLast = null;
    try {
      const res = selectedTarget.via_canvas
        ? await convertViaCanvas(convProbe.path, selectedTarget.id, convQuality, convMaxSide)
        : await api.convertFile(
            convProbe.path,
            selectedTarget.id,
            selectedTarget.supports_quality ? convQuality : null,
            selectedTarget.supports_resize ? convMaxSide : null,
          );
      convLast = res;
      mergeItems([res.item]);
      flash("copied", `Готово: ${res.item.name}`);
    } catch (e) {
      convErr = typeof e === "string" ? e : "Ошибка конвертации";
    } finally {
      convBusy = false;
    }
  }

  async function zipAllShelf() {
    if (ui.shelf.length < 2 || convBusy) return;
    convBusy = true;
    try {
      const res = await api.convertZipBundle(ui.shelf.map((s) => s.path));
      mergeItems([res.item]);
      flash("copied", `ZIP готов (${fmtSize(res.out_size)})`);
    } catch (e) {
      flash("copied", typeof e === "string" ? e : "Не удалось создать ZIP");
    } finally {
      convBusy = false;
    }
  }

  function formatDelta(src: number, out: number) {
    if (src <= 0) return "";
    const pct = Math.round(((out - src) / src) * 100);
    if (pct === 0) return "±0%";
    return pct < 0 ? `${pct}%` : `+${pct}%`;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="wrap"
  class:dragover={ui.dragOver}
  ondragover={(e) => {
    e.preventDefault();
    ui.dragOver = true;
  }}
  ondragleave={() => (ui.dragOver = false)}
  ondrop={(e) => {
    e.preventDefault();
    ui.dragOver = false;
  }}
>
    <div class="head">
    <span class="desc">Тяни сюда, вытягивай на рабочий стол или конвертируй.</span>
    <div class="hbtns">
      {#if ui.shelf.length >= 2}
        <button class="clr" onclick={zipAllShelf} disabled={convBusy} title="Упаковать всю полку в один ZIP">
          <Icon name="convert" size={13} />
          <span>Всё в ZIP</span>
        </button>
      {/if}
      {#if ui.shelf.length > 0}
        <button
          class="clr"
          onclick={() => api.shelfCopyFiles(ui.shelf.map((s) => s.path)).then(() => flash("copied", "Файлы скопированы")).catch(() => {})}
          title="Скопировать все файлы (Ctrl+V)"
        >
          <Icon name="copy" size={13} />
          <span>Копировать все</span>
        </button>
      {/if}
      <button class="add" onclick={addFiles}>
        <Icon name="plus" size={13} />
        <span>Добавить файлы</span>
      </button>
      {#if ui.shelf.length > 0}
        <button class="clr" onclick={clearAll} title="Очистить полку">
          <Icon name="trash" size={13} />
          <span>Очистить ({ui.shelf.length})</span>
        </button>
      {/if}
    </div>
  </div>

  {#if convProbe}
    <section class="conv">
      <div class="conv-top">
        <div class="conv-src">
          <span class="ext mono">{(convProbe.ext || convProbe.kind).toUpperCase()}</span>
          <span class="cname">{convProbe.name}</span>
          <span class="cmeta mono">
            {fmtSize(convProbe.size)}{convProbe.width && convProbe.height ? ` · ${convProbe.width}×${convProbe.height}` : ""}
          </span>
        </div>
        <button class="ibtn" onclick={closeConverter} title="Закрыть конвертор">
          <Icon name="close" size={14} />
        </button>
      </div>

      <div class="conv-targets">
        <span class="carrow mono">→</span>
        {#each convProbe.targets as t}
          <button
            class="tchip"
            class:active={selectedTarget?.id === t.id}
            onclick={() => {
              convTargetId = t.id;
              convErr = null;
              convLast = null;
            }}
            title={t.hint}
          >
            {t.label}
          </button>
        {/each}
      </div>

      {#if selectedTarget}
        <div class="conv-opts">
          <span class="chint">{selectedTarget.hint}</span>
          {#if selectedTarget.supports_resize}
            <div class="sizes">
              {#each [null, 1024, 512, 256, 128] as s}
                <button
                  class="schip mono"
                  class:active={convMaxSide === s}
                  onclick={() => (convMaxSide = s)}
                >
                  {s === null ? "Ориг" : `${s}px`}
                </button>
              {/each}
            </div>
          {/if}
          {#if selectedTarget.supports_quality}
            <label class="qual mono">
              <span>Кач. {convQuality}%</span>
              <input type="range" min="45" max="100" step="1" bind:value={convQuality} />
            </label>
          {/if}
        </div>
      {/if}

      <div class="conv-foot">
        <button class="add go" onclick={runConvert} disabled={convBusy || !selectedTarget}>
          <Icon name="convert" size={13} />
          <span>{convBusy ? "Конвертирую..." : `Конвертировать в ${selectedTarget?.label ?? ""}`}</span>
        </button>
        {#if convLast}
          <div class="cdone mono">
            <span>{fmtSize(convLast.src_size)} → {fmtSize(convLast.out_size)}</span>
            <span class="delta" class:smaller={convLast.out_size < convLast.src_size}>
              {formatDelta(convLast.src_size, convLast.out_size)}
            </span>
            <span class="tms">{convLast.ms} мс</span>
            <button class="ibtn" onclick={() => copyFile(convLast!.item)} title="Копировать результат (Ctrl+V)">
              <Icon name="copy" size={13} />
            </button>
          </div>
        {:else if convErr}
          <span class="cerr">{convErr}</span>
        {/if}
      </div>
    </section>
  {/if}

  {#if ui.shelf.length === 0}
    <div class="empty" class:over={ui.dragOver}>
      <div class="drop-badge">
        <Icon name="shelf" size={28} />
      </div>
      <p>
        {ui.dragOver
          ? "Отпусти файлы прямо здесь — они сохранятся на полке!"
          : "Перетащи файл с рабочего стола на верхний край экрана или нажми «Добавить файлы»."}
      </p>
    </div>
  {:else}
    <div class="grid">
      {#each ui.shelf as item (item.path)}
        <div class="card" onpointerdown={(e) => startCardDrag(e, item)}>
          <button class="main" draggable="false" onclick={() => openItem(item)} title="{item.path}&#10;Тяни карточку, чтобы вынести файл">
            <div class="ico" class:dir={item.is_dir}>
              <Icon name={item.is_dir ? "folder" : "file"} size={18} />
            </div>
            <div class="meta">
              <div class="name">{item.name}</div>
              <div class="sub mono">{fmtSize(item.size)}</div>
            </div>
          </button>
          <div class="acts">
            <button
              class="ibtn"
              class:conv-on={convProbe?.path === item.path}
              onclick={() => openConverter(item)}
              title="Конвертировать файл"
            >
              <Icon name="convert" size={14} />
            </button>
            <button class="ibtn" onclick={() => copyFile(item)} title="Копировать файл (Ctrl+V)">
              <Icon name="copy" size={14} />
            </button>
            <button class="ibtn" onclick={() => copyPath(item)} title="Скопировать путь">
              <Icon name="clipboard" size={14} />
            </button>
            <button class="ibtn" onclick={() => revealItem(item)} title="Показать в проводнике">
              <Icon name="folder" size={14} />
            </button>
            <button class="ibtn del" onclick={() => removeItem(item.path)} title="Убрать с полки">
              <Icon name="close" size={14} />
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
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .desc {
    font-size: 12px;
    color: var(--muted);
  }
  .hbtns {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 11px;
    border-radius: 8px;
    background: rgba(var(--accent-rgb), 0.16);
    border: 1px solid rgba(var(--accent-rgb), 0.45);
    font-size: 11.5px;
    font-weight: 500;
    color: rgb(var(--accent-rgb));
  }
  .add:hover {
    background: rgba(var(--accent-rgb), 0.24);
  }
  .clr {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border-radius: 8px;
    background: var(--ink-2);
    border: 1px solid var(--line);
    font-size: 11.5px;
    color: var(--muted);
    flex: none;
  }
  .clr:hover {
    color: var(--danger);
    border-color: rgba(255, 95, 82, 0.35);
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px dashed var(--line-2);
    box-shadow: var(--surface-sheen);
    border-radius: var(--r-m);
    color: var(--muted);
    text-align: center;
    padding: 20px;
    font-size: 12.5px;
    transition: border-color 140ms var(--ease), background 140ms var(--ease);
  }
  .empty.over,
  .wrap.dragover .empty {
    border-color: rgb(var(--accent-rgb));
    background: rgba(var(--accent-rgb), 0.1);
    color: var(--text);
  }
  .empty p {
    margin: 0;
    max-width: 390px;
    line-height: 1.5;
  }
  .drop-badge {
    width: 52px;
    height: 52px;
    border-radius: 16px;
    display: grid;
    place-items: center;
    background: rgba(var(--accent-rgb), 0.14);
    border: 1px solid rgba(var(--accent-rgb), 0.35);
    color: rgb(var(--accent-rgb));
  }
  .grid {
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
    align-content: start;
    padding-right: 4px;
  }
  .card {
    display: flex;
    align-items: center;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: var(--surface-sheen);
    border-radius: var(--r-s);
    transition: border-color 140ms var(--ease), background 140ms var(--ease), transform 140ms var(--ease);
    min-width: 0;
  }
  .card:hover {
    background: var(--glass-bg-strong);
    border-color: var(--line-2);
    transform: translateY(-1px);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    text-align: left;
  }
  .ico {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: var(--ink-3);
    color: var(--muted);
    display: grid;
    place-items: center;
    flex: none;
  }
  .ico.dir {
    color: rgb(var(--accent-rgb));
    background: rgba(var(--accent-rgb), 0.12);
  }
  .meta {
    min-width: 0;
  }
  .name {
    font-size: 12.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 10.5px;
    color: var(--faint);
    margin-top: 2px;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 6px;
  }
  .ibtn {
    width: 26px;
    height: 26px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    color: var(--faint);
  }
  .ibtn:hover {
    background: var(--ink-4);
    color: var(--text);
  }
  .ibtn.conv-on {
    color: rgb(var(--accent-rgb));
    background: rgba(var(--accent-rgb), 0.16);
  }
  .ibtn.del:hover {
    color: var(--danger);
  }
  .conv {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--r-m);
    background: linear-gradient(135deg, rgba(var(--accent-rgb), 0.1), var(--ink-2) 60%);
    border: 1px solid rgba(var(--accent-rgb), 0.32);
  }
  .conv-top,
  .conv-targets,
  .conv-opts,
  .conv-foot {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .conv-top {
    justify-content: space-between;
  }
  .conv-src {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
  }
  .ext {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 5px;
    background: rgba(var(--accent-rgb), 0.2);
    color: rgb(var(--accent-rgb));
    flex: none;
  }
  .cname {
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cmeta,
  .chint {
    font-size: 11px;
    color: var(--muted);
  }
  .carrow {
    color: rgb(var(--accent-rgb));
    font-size: 13px;
  }
  .tchip,
  .schip {
    padding: 3px 9px;
    border-radius: 7px;
    background: var(--ink-3);
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--muted);
  }
  .tchip:hover,
  .schip:hover {
    color: var(--text);
    border-color: var(--line-2);
  }
  .tchip.active,
  .schip.active {
    background: rgba(var(--accent-rgb), 0.2);
    border-color: rgba(var(--accent-rgb), 0.6);
    color: rgb(var(--accent-rgb));
    font-weight: 600;
  }
  .sizes {
    display: flex;
    gap: 4px;
  }
  .qual {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--muted);
    margin-left: auto;
  }
  .qual input {
    width: 82px;
    accent-color: rgb(var(--accent-rgb));
  }
  .go:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .cdone {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text);
    padding: 3px 8px;
    border-radius: 7px;
    background: var(--ink-3);
  }
  .delta {
    color: var(--muted);
    font-weight: 600;
  }
  .delta.smaller {
    color: var(--ok);
  }
  .tms {
    color: var(--faint);
  }
  .cerr {
    font-size: 11.5px;
    color: var(--danger);
  }
</style>
