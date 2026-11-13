<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, type ConvertProbe, type ConvertResult, type ShelfItem } from "../lib/ipc";
  import { ui, flash, fmtSize, saveShelf } from "../lib/state.svelte";

  let justDragged = false;
  let lastAutoAttempt = "";

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

  const DEMO_ROUTES = [
    { src: "PNG", dst: "JPG" },
    { src: "MD", dst: "HTML" },
    { src: "CSV", dst: "JSON" },
  ];

  onMount(() => {
    api.shelfPrepareDrop().catch(() => {});
  });

  $effect(() => {
    const items = ui.shelf;
    if (items.length === 0) {
      lastAutoAttempt = "";
      if (convProbe) closeConverter();
      return;
    }
    const stillPresent = convProbe && items.some((s) => s.path === convProbe?.path);
    if (!stillPresent && lastAutoAttempt !== items[0].path) {
      lastAutoAttempt = items[0].path;
      selectItem(items[0], true);
    }
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
      if (items.length > 0) {
        mergeItems(items);
        selectItem(items[0], true);
      }
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

  async function selectItem(item: ShelfItem, silent = false) {
    if (justDragged) {
      justDragged = false;
      return;
    }
    if (convProbe?.path === item.path) return;
    convErr = null;
    convLast = null;
    convMaxSide = null;
    try {
      const probe = await api.convertProbe(item.path);
      convProbe = probe;
      convTargetId = probe.targets[0]?.id ?? "";
    } catch (e) {
      if (!silent) flash("copied", typeof e === "string" ? e : "Файл недоступен");
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

  async function runConvert(targetId?: string) {
    if (targetId) convTargetId = targetId;
    const target = convProbe?.targets.find((t) => t.id === convTargetId) ?? selectedTarget;
    if (!convProbe || !target || convBusy) return;
    convBusy = true;
    convErr = null;
    convLast = null;
    try {
      const res = target.via_canvas
        ? await convertViaCanvas(convProbe.path, target.id, convQuality, convMaxSide)
        : await api.convertFile(
            convProbe.path,
            target.id,
            target.supports_quality ? convQuality : null,
            target.supports_resize ? convMaxSide : null,
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

  function fileExt(item: ShelfItem): string {
    if (item.is_dir) return "DIR";
    const dot = item.name.lastIndexOf(".");
    if (dot < 0 || dot === item.name.length - 1) return "FILE";
    return item.name.slice(dot + 1, dot + 5).toUpperCase();
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
    <div class="title-row">
      <span class="badge-pill mono">{ui.shelf.length} {ui.shelf.length === 1 ? "файл" : "файлов"}</span>
      <span class="desc">Перетяни сюда, вынеси на рабочий стол или конвертируй в 1 клик</span>
    </div>
    <div class="hbtns">
      {#if ui.shelf.length >= 2}
        <button class="pill-btn" onclick={zipAllShelf} disabled={convBusy} title="Упаковать всю полку в один ZIP">
          <Icon name="convert" size={12} />
          <span>В ZIP</span>
        </button>
      {/if}
      {#if ui.shelf.length > 0}
        <button
          class="pill-btn"
          onclick={() => api.shelfCopyFiles(ui.shelf.map((s) => s.path)).then(() => flash("copied", "Файлы скопированы")).catch(() => {})}
          title="Скопировать все файлы (Ctrl+V)"
        >
          <Icon name="copy" size={12} />
          <span>Копир. все</span>
        </button>
      {/if}
      <button class="pill-btn accent" onclick={addFiles}>
        <Icon name="plus" size={12} />
        <span>Добавить</span>
      </button>
      {#if ui.shelf.length > 0}
        <button class="pill-btn danger" onclick={clearAll} title="Очистить полку">
          <Icon name="trash" size={12} />
        </button>
      {/if}
    </div>
  </div>

  <div class="bento">
    <!-- Left column: Dashed drag-and-drop holding zone + file cards -->
    <div class="drop-pane" class:over={ui.dragOver}>
      {#if ui.shelf.length === 0}
        <button class="empty-zone" onclick={addFiles}>
          <div class="glyph-trio">
            <div class="g-doc"><Icon name="file" size={22} /></div>
            <div class="g-doc mid"><Icon name="clipboard" size={22} /></div>
            <div class="g-doc"><Icon name="note" size={22} /></div>
          </div>
          <div class="zone-title">
            {ui.dragOver ? "Отпустите файлы прямо здесь" : "Зона перетаскивания файлов"}
          </div>
          <div class="zone-sub">Перетяните файлы сюда или нажмите для выбора</div>
        </button>
      {:else}
        <div class="list-scroll">
          {#each ui.shelf as item (item.path)}
            <div
              class="scard"
              class:sel={convProbe?.path === item.path}
              onpointerdown={(e) => startCardDrag(e, item)}
            >
              {#if convProbe?.path === item.path}
                <span class="sel-bar" aria-hidden="true"></span>
              {/if}
              <button
                class="smain"
                draggable="false"
                onclick={() => selectItem(item)}
                ondblclick={() => openItem(item)}
                title="{item.path}&#10;Клик — выбрать для конвертации · Двойной клик — открыть · Потяните — вынести"
              >
                <span class="ext-pill mono" class:dir={item.is_dir}>{fileExt(item)}</span>
                <div class="smeta">
                  <div class="sname">{item.name}</div>
                  <div class="ssub mono">{fmtSize(item.size)}</div>
                </div>
              </button>
              <div class="acts">
                <button class="ibtn" onclick={() => copyFile(item)} title="Копировать файл (Ctrl+V)">
                  <Icon name="copy" size={13} />
                </button>
                <button class="ibtn" onclick={() => copyPath(item)} title="Скопировать путь">
                  <Icon name="clipboard" size={13} />
                </button>
                <button class="ibtn" onclick={() => revealItem(item)} title="Показать в папке">
                  <Icon name="folder" size={13} />
                </button>
                <button class="ibtn" onclick={() => openItem(item)} title="Открыть файл">
                  <Icon name="external" size={13} />
                </button>
                <button class="ibtn del" onclick={() => removeItem(item.path)} title="Убрать с полки">
                  <Icon name="close" size={13} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Right column: Persistent Quick Converter Bento Panel -->
    <div class="conv-pane">
      {#if convProbe && convProbe.targets.length > 0}
        {@const srcExt = (convProbe.ext || convProbe.kind).toUpperCase()}
        <div class="conv-head">
          <div class="conv-file">
            <span class="src-tag mono">{srcExt}</span>
            <span class="cname">{convProbe.name}</span>
          </div>
          <span class="cmeta mono">
            {fmtSize(convProbe.size)}{convProbe.width && convProbe.height ? ` · ${convProbe.width}×${convProbe.height}` : ""}
          </span>
        </div>

        <div class="trows">
          {#each convProbe.targets as t}
            <div class="trow" class:active={selectedTarget?.id === t.id}>
              <button
                class="trow-sel"
                onclick={() => {
                  convTargetId = t.id;
                  convErr = null;
                  convLast = null;
                }}
                title={t.hint}
              >
                <span class="mono from-ext">{srcExt}</span>
                <span class="arrow">→</span>
                <span class="mono to-ext">{t.label}</span>
              </button>
              <button
                class="conv-cta"
                onclick={() => runConvert(t.id)}
                disabled={convBusy}
                title={`Конвертировать в ${t.label}`}
              >
                {convBusy && selectedTarget?.id === t.id ? "..." : "Конверт"}
              </button>
            </div>
          {/each}
        </div>

        {#if selectedTarget && (selectedTarget.supports_resize || selectedTarget.supports_quality)}
          <div class="conv-opts">
            {#if selectedTarget.supports_resize}
              <div class="sizes">
                {#each [null, 1024, 512, 256, 128] as s}
                  <button
                    class="schip mono"
                    class:active={convMaxSide === s}
                    onclick={() => (convMaxSide = s)}
                  >
                    {s === null ? "Ориг" : `${s}`}
                  </button>
                {/each}
              </div>
            {/if}
            {#if selectedTarget.supports_quality}
              <label class="qual mono">
                <span>{convQuality}%</span>
                <input type="range" min="45" max="100" step="1" bind:value={convQuality} />
              </label>
            {/if}
          </div>
        {/if}

        <div class="conv-foot">
          {#if convLast}
            <div class="cdone mono">
              <span>{fmtSize(convLast.src_size)} → {fmtSize(convLast.out_size)}</span>
              <span class="delta" class:smaller={convLast.out_size < convLast.src_size}>
                {formatDelta(convLast.src_size, convLast.out_size)}
              </span>
              <span class="tms">{convLast.ms} мс</span>
              <button class="ibtn" onclick={() => copyFile(convLast!.item)} title="Копировать результат">
                <Icon name="copy" size={12} />
              </button>
            </div>
          {:else if convErr}
            <span class="cerr">{convErr}</span>
          {:else if selectedTarget}
            <span class="chint">{selectedTarget.hint}</span>
          {/if}
        </div>
      {:else if convProbe}
        <div class="conv-empty">
          <div class="conv-head">
            <div class="conv-file">
              <span class="src-tag mono">{(convProbe.ext || convProbe.kind).toUpperCase()}</span>
              <span class="cname">{convProbe.name}</span>
            </div>
          </div>
          <div class="no-targets">
            <Icon name="file" size={20} />
            <span>Нет прямых целей конвертации для этого типа. Вы можете перетащить его на рабочий стол или упаковать полку в ZIP.</span>
          </div>
        </div>
      {:else}
        <div class="demo-rows">
          <div class="demo-cap">Быстрая конвертация форматов</div>
          {#each DEMO_ROUTES as d}
            <div class="trow muted">
              <div class="trow-sel static">
                <span class="mono from-ext">{d.src}</span>
                <span class="arrow">→</span>
                <span class="mono to-ext">{d.dst}</span>
              </div>
              <button class="conv-cta ghost" onclick={addFiles}>Конверт</button>
            </div>
          {/each}
          <div class="demo-hint">Добавьте PNG, JPG, WEBP, SVG, MD, CSV или JSON слева</div>
        </div>
      {/if}
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

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex: none;
  }

  .title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .badge-pill {
    font-size: 11px;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--glass-bg-strong);
    border: 1px solid var(--line);
    color: var(--fg);
    flex: none;
  }

  .desc {
    font-size: 11.5px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .hbtns {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: none;
  }

  .pill-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid var(--line);
    color: var(--fg);
    font-size: 11.5px;
    font-weight: 500;
    transition: background var(--t-fast), border-color var(--t-fast), transform var(--t-fast);
  }
  .pill-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.09);
    border-color: var(--line-strong);
  }
  .pill-btn.accent {
    background: rgba(var(--accent-rgb), 0.16);
    border-color: rgba(var(--accent-rgb), 0.42);
    color: var(--fg);
  }
  .pill-btn.accent:hover {
    background: rgba(var(--accent-rgb), 0.25);
    border-color: rgba(var(--accent-rgb), 0.65);
  }
  .pill-btn.danger:hover {
    background: rgba(255, 90, 90, 0.14);
    border-color: rgba(255, 90, 90, 0.35);
    color: #ff9c9c;
  }
  .pill-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .bento {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1.08fr 1fr;
    gap: 10px;
  }

  /* Left dashed drop holding zone */
  .drop-pane {
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-m);
    background: rgba(22, 24, 30, 0.64);
    border: 1.5px dashed rgba(255, 255, 255, 0.16);
    padding: 8px;
    transition: border-color var(--t-fast), background var(--t-fast), box-shadow var(--t-fast);
  }
  .drop-pane.over {
    border-color: rgba(var(--accent-rgb), 0.8);
    background: rgba(var(--accent-rgb), 0.08);
    box-shadow: inset 0 0 24px rgba(var(--accent-rgb), 0.12);
  }

  .empty-zone {
    flex: 1;
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    text-align: center;
    padding: 12px;
    border-radius: 12px;
    transition: background var(--t-fast);
  }
  .empty-zone:hover {
    background: rgba(255, 255, 255, 0.03);
  }

  .glyph-trio {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    color: rgba(255, 255, 255, 0.42);
    margin-bottom: 2px;
  }
  .g-doc {
    width: 38px;
    height: 44px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.11);
  }
  .g-doc.mid {
    width: 42px;
    height: 48px;
    color: rgba(var(--accent-rgb), 0.85);
    border-color: rgba(var(--accent-rgb), 0.35);
    background: rgba(var(--accent-rgb), 0.08);
  }

  .zone-title {
    font-size: 13px;
    font-weight: 500;
    color: var(--fg);
  }
  .zone-sub {
    font-size: 11px;
    color: var(--muted);
    max-width: 240px;
  }

  .list-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding-right: 2px;
  }

  .scard {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px 6px 10px;
    border-radius: 11px;
    background: var(--glass-bg);
    border: 1px solid var(--line);
    cursor: grab;
    transition: background var(--t-fast), border-color var(--t-fast), transform var(--t-fast);
  }
  .scard:hover {
    background: rgba(255, 255, 255, 0.06);
    border-color: var(--line-strong);
  }
  .scard.sel {
    background: rgba(255, 255, 255, 0.075);
    border-color: rgba(var(--accent-rgb), 0.36);
  }
  .sel-bar {
    position: absolute;
    left: 4px;
    top: 7px;
    bottom: 7px;
    width: 3px;
    border-radius: 99px;
    background: rgb(var(--accent-rgb));
    box-shadow: 0 0 8px rgba(var(--accent-rgb), 0.7);
  }

  .smain {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
  }

  .ext-pill {
    min-width: 36px;
    height: 28px;
    padding: 0 6px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.03em;
    background: rgba(255, 255, 255, 0.055);
    border: 1px solid var(--line);
    color: var(--fg);
    flex: none;
  }
  .ext-pill.dir {
    background: rgba(var(--accent-rgb), 0.14);
    border-color: rgba(var(--accent-rgb), 0.35);
    color: rgb(var(--accent-rgb));
  }

  .smeta {
    flex: 1;
    min-width: 0;
  }
  .sname {
    font-size: 12px;
    font-weight: 500;
    color: var(--fg);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ssub {
    font-size: 10.5px;
    color: var(--muted);
  }

  .acts {
    display: flex;
    align-items: center;
    gap: 2px;
    opacity: 0.35;
    transition: opacity var(--t-fast);
  }
  .scard:hover .acts,
  .scard.sel .acts {
    opacity: 1;
  }

  .ibtn {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    color: var(--muted);
    transition: background var(--t-fast), color var(--t-fast);
  }
  .ibtn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: var(--fg);
  }
  .ibtn.del:hover {
    background: rgba(255, 90, 90, 0.16);
    color: #ff9c9c;
  }

  /* Right Converter Bento Pane */
  .conv-pane {
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 11px;
    border-radius: var(--r-m);
    background: var(--glass-bg);
    border: 1px solid var(--line);
    box-shadow: var(--shadow-soft);
  }

  .conv-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex: none;
  }
  .conv-file {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .src-tag {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 6px;
    background: rgba(var(--accent-rgb), 0.16);
    border: 1px solid rgba(var(--accent-rgb), 0.38);
    color: rgb(var(--accent-rgb));
    flex: none;
  }
  .cname {
    font-size: 12px;
    font-weight: 500;
    color: var(--fg);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cmeta {
    font-size: 10.5px;
    color: var(--muted);
    flex: none;
  }

  .trows {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .trow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 8px 6px 11px;
    border-radius: 11px;
    background: rgba(18, 20, 26, 0.62);
    border: 1px solid var(--line);
    transition: border-color var(--t-fast), background var(--t-fast);
  }
  .trow:hover:not(.muted),
  .trow.active {
    border-color: rgba(var(--accent-rgb), 0.38);
    background: rgba(255, 255, 255, 0.045);
  }

  .trow-sel {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    text-align: left;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--fg);
  }
  .trow-sel.static {
    cursor: default;
  }
  .from-ext {
    color: var(--muted);
    font-weight: 500;
  }
  .arrow {
    color: rgba(255, 255, 255, 0.32);
    font-size: 12px;
  }
  .to-ext {
    color: var(--fg);
  }

  .conv-cta {
    height: 26px;
    padding: 0 11px;
    border-radius: 8px;
    background: rgba(var(--accent-rgb), 0.24);
    border: 1px solid rgba(var(--accent-rgb), 0.5);
    color: #ffd5bc;
    font-size: 11.5px;
    font-weight: 600;
    flex: none;
    transition: background var(--t-fast), border-color var(--t-fast), transform var(--t-fast);
  }
  .conv-cta:hover:not(:disabled) {
    background: rgba(var(--accent-rgb), 0.36);
    border-color: rgba(var(--accent-rgb), 0.8);
    color: #ffffff;
  }
  .conv-cta:active:not(:disabled) {
    transform: scale(0.96);
  }
  .conv-cta.ghost {
    opacity: 0.72;
  }

  .conv-opts {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding-top: 2px;
    flex: none;
  }
  .sizes {
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .schip {
    font-size: 10px;
    padding: 3px 6px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid var(--line);
    color: var(--muted);
  }
  .schip.active {
    background: rgba(var(--accent-rgb), 0.16);
    border-color: rgba(var(--accent-rgb), 0.45);
    color: var(--fg);
  }
  .qual {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10.5px;
    color: var(--muted);
  }
  .qual input[type="range"] {
    width: 68px;
    accent-color: rgb(var(--accent-rgb));
  }

  .conv-foot {
    flex: none;
    min-height: 22px;
    display: flex;
    align-items: center;
  }
  .chint {
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cdone {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--fg);
  }
  .delta {
    padding: 1px 6px;
    border-radius: 5px;
    background: rgba(255, 255, 255, 0.08);
    font-weight: 600;
  }
  .delta.smaller {
    background: rgba(72, 213, 151, 0.18);
    color: #8ce9b5;
  }
  .tms {
    color: var(--muted);
  }
  .cerr {
    font-size: 11px;
    color: #ff9c9c;
  }

  .conv-empty,
  .demo-rows {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .no-targets {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    text-align: center;
    font-size: 11.5px;
    color: var(--muted);
    padding: 12px;
  }
  .demo-cap {
    font-size: 12px;
    font-weight: 500;
    color: var(--fg);
  }
  .demo-hint {
    margin-top: auto;
    font-size: 11px;
    color: var(--muted);
    text-align: center;
  }
</style>
