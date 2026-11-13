<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, type Clip } from "../lib/ipc";
  import { flash, saveEditorDir, ui } from "../lib/state.svelte";

  type Tool = "arrow" | "rect" | "brush" | "marker" | "blur" | "text" | "crop";

  let { clip, onClose }: { clip: Clip; onClose: () => void } = $props();

  let canvasEl: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D | null = null;
  let origCanvas: HTMLCanvasElement | null = null;

  let tool = $state<Tool>("arrow");
  let color = $state("#ff5f52");
  let size = $state(4);
  let loading = $state(true);
  let busy = $state(false);
  let imgW = $state(0);
  let imgH = $state(0);
  let undoCount = $state(0);
  let undoStack: ImageData[] = [];

  let saveDir = $state(ui.editorSaveDir);
  let lastSavedPath = $state<string | null>(null);

  // Для интерактивного рисования фигур и обрезки
  let drawing = $state(false);
  let startPt = { x: 0, y: 0 };
  let curPt = $state<{ x: number; y: number } | null>(null);
  let baseSnapshot: ImageData | null = null;
  let cropBox = $state<{ x: number; y: number; w: number; h: number } | null>(null);

  // Для вставки текста
  let textPos = $state<{ x: number; y: number } | null>(null);
  let textVal = $state("");
  let textInputEl = $state<HTMLInputElement | null>(null);
  let textBlurTimer: number | undefined;

  const COLORS = ["#ff5f52", "#ff7a3d", "#ffc857", "#4ad698", "#5ca8ff", "#ffffff", "#111111"];
  const SIZES = [
    { id: 2, label: "S" },
    { id: 4, label: "M" },
    { id: 8, label: "L" },
  ];

  const TOOLS: { id: Tool; label: string; icon: "arrowDiag" | "square" | "pencil" | "text" | "crop" | "pipette" }[] = [
    { id: "arrow", label: "Стрелка", icon: "arrowDiag" },
    { id: "rect", label: "Рамка", icon: "square" },
    { id: "brush", label: "Кисть", icon: "pencil" },
    { id: "marker", label: "Маркер", icon: "pipette" },
    { id: "blur", label: "Блюр", icon: "square" },
    { id: "text", label: "Текст", icon: "text" },
    { id: "crop", label: "Обрезка", icon: "crop" },
  ];

  function pushUndo() {
    if (!ctx || !canvasEl.width || !canvasEl.height) return;
    const snap = ctx.getImageData(0, 0, canvasEl.width, canvasEl.height);
    undoStack.push(snap);
    // Ограничиваем память истории (~96 МБ, но минимум 3 шага) — важно для 4K-скриншотов
    const MAX_BYTES = 96 * 1024 * 1024;
    let bytes = undoStack.reduce((sum, s) => sum + s.data.byteLength, 0);
    while (undoStack.length > 3 && (undoStack.length > 16 || bytes > MAX_BYTES)) {
      const dropped = undoStack.shift();
      if (dropped) bytes -= dropped.data.byteLength;
    }
    undoCount = undoStack.length;
  }

  function undo() {
    if (!ctx || undoStack.length === 0) return;
    const snap = undoStack.pop()!;
    canvasEl.width = snap.width;
    canvasEl.height = snap.height;
    imgW = snap.width;
    imgH = snap.height;
    ctx.putImageData(snap, 0, 0);
    undoCount = undoStack.length;
    cropBox = null;
  }

  function resetOriginal() {
    if (!ctx || !origCanvas) return;
    pushUndo();
    canvasEl.width = origCanvas.width;
    canvasEl.height = origCanvas.height;
    imgW = origCanvas.width;
    imgH = origCanvas.height;
    ctx.drawImage(origCanvas, 0, 0);
    cropBox = null;
  }

  function toCanvasCoords(e: PointerEvent): { x: number; y: number } {
    const r = canvasEl.getBoundingClientRect();
    const sx = canvasEl.width / Math.max(1, r.width);
    const sy = canvasEl.height / Math.max(1, r.height);
    return {
      x: Math.max(0, Math.min(canvasEl.width, (e.clientX - r.left) * sx)),
      y: Math.max(0, Math.min(canvasEl.height, (e.clientY - r.top) * sy)),
    };
  }

  function strokeScale(): number {
    return Math.max(1, Math.min(canvasEl.width, canvasEl.height) / 560);
  }

  function drawArrow(c: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number, col: string, w: number) {
    const angle = Math.atan2(y2 - y1, x2 - x1);
    const len = Math.hypot(x2 - x1, y2 - y1);
    const head = Math.max(w * 3.4, Math.min(28 * strokeScale(), len * 0.35));
    c.save();
    c.strokeStyle = col;
    c.fillStyle = col;
    c.lineWidth = w;
    c.lineCap = "round";
    c.lineJoin = "round";
    c.beginPath();
    c.moveTo(x1, y1);
    c.lineTo(x2 - Math.cos(angle) * head * 0.55, y2 - Math.sin(angle) * head * 0.55);
    c.stroke();

    c.beginPath();
    c.moveTo(x2, y2);
    c.lineTo(x2 - head * Math.cos(angle - Math.PI / 6), y2 - head * Math.sin(angle - Math.PI / 6));
    c.lineTo(x2 - head * Math.cos(angle + Math.PI / 6), y2 - head * Math.sin(angle + Math.PI / 6));
    c.closePath();
    c.fill();
    c.restore();
  }

  function pixelateRect(c: CanvasRenderingContext2D, x: number, y: number, w: number, h: number) {
    const rx = Math.max(0, Math.round(x));
    const ry = Math.max(0, Math.round(y));
    const rw = Math.min(canvasEl.width - rx, Math.round(w));
    const rh = Math.min(canvasEl.height - ry, Math.round(h));
    if (rw < 4 || rh < 4) return;
    const block = Math.max(8, Math.round(Math.min(rw, rh) / 8));
    const tmp = document.createElement("canvas");
    tmp.width = Math.max(2, Math.round(rw / block));
    tmp.height = Math.max(2, Math.round(rh / block));
    const tctx = tmp.getContext("2d")!;
    tctx.drawImage(canvasEl, rx, ry, rw, rh, 0, 0, tmp.width, tmp.height);
    c.save();
    c.imageSmoothingEnabled = false;
    c.drawImage(tmp, 0, 0, tmp.width, tmp.height, rx, ry, rw, rh);
    c.restore();
  }

  function onPointerDown(e: PointerEvent) {
    if (!ctx || loading || e.button !== 0) return;
    const pt = toCanvasCoords(e);

    if (tool === "text") {
      textPos = pt;
      textVal = "";
      ui.inputMode = true;
      api.beginInput().catch(() => {});
      requestAnimationFrame(() => textInputEl?.focus());
      return;
    }

    canvasEl.setPointerCapture(e.pointerId);
    drawing = true;
    startPt = pt;
    curPt = pt;

    if (tool === "brush" || tool === "marker") {
      pushUndo();
      ctx.save();
      ctx.strokeStyle = color;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.lineWidth = size * strokeScale() * (tool === "marker" ? 3.2 : 1);
      ctx.globalAlpha = tool === "marker" ? 0.36 : 1;
      ctx.beginPath();
      ctx.moveTo(pt.x, pt.y);
    } else if (tool === "crop") {
      cropBox = { x: pt.x, y: pt.y, w: 0, h: 0 };
    } else {
      baseSnapshot = ctx.getImageData(0, 0, canvasEl.width, canvasEl.height);
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (!drawing || !ctx) return;
    const pt = toCanvasCoords(e);
    curPt = pt;

    if (tool === "brush" || tool === "marker") {
      ctx.lineTo(pt.x, pt.y);
      ctx.stroke();
      return;
    }

    if (tool === "crop") {
      const x = Math.min(startPt.x, pt.x);
      const y = Math.min(startPt.y, pt.y);
      const w = Math.abs(pt.x - startPt.x);
      const h = Math.abs(pt.y - startPt.y);
      cropBox = { x, y, w, h };
      return;
    }

    if (!baseSnapshot) return;
    ctx.putImageData(baseSnapshot, 0, 0);
    const sw = size * strokeScale();
    if (tool === "arrow") {
      drawArrow(ctx, startPt.x, startPt.y, pt.x, pt.y, color, sw);
    } else if (tool === "rect") {
      ctx.save();
      ctx.strokeStyle = color;
      ctx.lineWidth = sw;
      ctx.strokeRect(
        Math.min(startPt.x, pt.x),
        Math.min(startPt.y, pt.y),
        Math.abs(pt.x - startPt.x),
        Math.abs(pt.y - startPt.y),
      );
      ctx.restore();
    } else if (tool === "blur") {
      ctx.save();
      ctx.strokeStyle = "rgba(255,255,255,0.8)";
      ctx.setLineDash([5, 4]);
      ctx.lineWidth = Math.max(1.5, sw * 0.5);
      ctx.strokeRect(
        Math.min(startPt.x, pt.x),
        Math.min(startPt.y, pt.y),
        Math.abs(pt.x - startPt.x),
        Math.abs(pt.y - startPt.y),
      );
      ctx.restore();
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (!drawing || !ctx) return;
    drawing = false;
    const pt = curPt ?? toCanvasCoords(e);

    if (tool === "brush" || tool === "marker") {
      ctx.restore();
      return;
    }

    if (tool === "crop") {
      if (cropBox && (cropBox.w < 10 || cropBox.h < 10)) {
        cropBox = null;
      }
      return;
    }

    if (baseSnapshot) {
      ctx.putImageData(baseSnapshot, 0, 0);
      const dx = Math.abs(pt.x - startPt.x);
      const dy = Math.abs(pt.y - startPt.y);
      if (dx > 4 || dy > 4) {
        pushUndo();
        const sw = size * strokeScale();
        if (tool === "arrow") {
          drawArrow(ctx, startPt.x, startPt.y, pt.x, pt.y, color, sw);
        } else if (tool === "rect") {
          ctx.save();
          ctx.strokeStyle = color;
          ctx.lineWidth = sw;
          ctx.strokeRect(
            Math.min(startPt.x, pt.x),
            Math.min(startPt.y, pt.y),
            dx,
            dy,
          );
          ctx.restore();
        } else if (tool === "blur") {
          pixelateRect(ctx, Math.min(startPt.x, pt.x), Math.min(startPt.y, pt.y), dx, dy);
        }
      }
      baseSnapshot = null;
    }
  }

  function applyCrop() {
    if (!ctx || !cropBox || cropBox.w < 10 || cropBox.h < 10) return;
    const rx = Math.round(cropBox.x);
    const ry = Math.round(cropBox.y);
    const rw = Math.max(1, Math.round(cropBox.w));
    const rh = Math.max(1, Math.round(cropBox.h));
    pushUndo();
    const region = ctx.getImageData(rx, ry, rw, rh);
    canvasEl.width = rw;
    canvasEl.height = rh;
    imgW = rw;
    imgH = rh;
    ctx.putImageData(region, 0, 0);
    cropBox = null;
    tool = "arrow";
  }

  function rotate90() {
    if (!ctx || !canvasEl.width || !canvasEl.height) return;
    pushUndo();
    const tmp = document.createElement("canvas");
    tmp.width = canvasEl.width;
    tmp.height = canvasEl.height;
    tmp.getContext("2d")!.drawImage(canvasEl, 0, 0);
    canvasEl.width = tmp.height;
    canvasEl.height = tmp.width;
    imgW = canvasEl.width;
    imgH = canvasEl.height;
    ctx.save();
    ctx.translate(canvasEl.width, 0);
    ctx.rotate(Math.PI / 2);
    ctx.drawImage(tmp, 0, 0);
    ctx.restore();
    cropBox = null;
  }

  function flipH() {
    if (!ctx || !canvasEl.width || !canvasEl.height) return;
    pushUndo();
    const tmp = document.createElement("canvas");
    tmp.width = canvasEl.width;
    tmp.height = canvasEl.height;
    tmp.getContext("2d")!.drawImage(canvasEl, 0, 0);
    ctx.save();
    ctx.clearRect(0, 0, canvasEl.width, canvasEl.height);
    ctx.translate(canvasEl.width, 0);
    ctx.scale(-1, 1);
    ctx.drawImage(tmp, 0, 0);
    ctx.restore();
    cropBox = null;
  }

  function commitText(e?: Event) {
    e?.preventDefault();
    const txt = textVal.trim();
    if (ctx && textPos && txt) {
      pushUndo();
      const px = Math.round((15 + size * 2.8) * strokeScale());
      ctx.save();
      ctx.font = `700 ${px}px "Onest Variable", "Segoe UI", sans-serif`;
      ctx.textBaseline = "middle";
      ctx.lineWidth = Math.max(3, px * 0.16);
      ctx.strokeStyle = color === "#111111" ? "rgba(255,255,255,0.85)" : "rgba(10,9,8,0.88)";
      ctx.strokeText(txt, textPos.x, textPos.y);
      ctx.fillStyle = color;
      ctx.fillText(txt, textPos.x, textPos.y);
      ctx.restore();
    }
    textPos = null;
    textVal = "";
    ui.inputMode = false;
    api.endInput().catch(() => {});
  }

  function onTextPointerDown(e: PointerEvent) {
    clearTimeout(textBlurTimer);
    ui.inputMode = true;
    api.beginInput().catch(() => {});
    (e.currentTarget as HTMLElement | null)?.focus();
  }

  function onTextBlur() {
    clearTimeout(textBlurTimer);
    textBlurTimer = window.setTimeout(() => {
      if (!document.hasFocus() || !document.activeElement?.closest("input, textarea, [contenteditable]")) {
        ui.inputMode = false;
        api.endInput().catch(() => {});
      }
    }, 50);
  }

  async function exportPngBytes(): Promise<number[]> {
    const blob = await new Promise<Blob | null>((res) => canvasEl.toBlob(res, "image/png"));
    if (!blob) throw new Error("Не удалось экспортировать PNG");
    return Array.from(new Uint8Array(await blob.arrayBuffer()));
  }

  async function saveToClipboard() {
    if (busy || loading) return;
    busy = true;
    try {
      const bytes = await exportPngBytes();
      const res = await api.clipSaveEdited(bytes, true, null, null);
      if (res.clip_id) {
        const preview = canvasEl.toDataURL("image/png");
        ui.recentShot = {
          id: res.clip_id,
          preview,
          text: `${canvasEl.width}×${canvasEl.height} · PNG`,
          at: Date.now(),
        };
      }
      flash("shot", "Изменено и в буфере");
    } catch (e) {
      flash("copied", typeof e === "string" ? e : "Ошибка копирования");
    } finally {
      busy = false;
    }
  }

  async function saveToDir() {
    if (busy || loading) return;
    busy = true;
    try {
      const targetDir = saveDir || (await api.defaultScreenshotsDir());
      saveDir = targetDir;
      saveEditorDir(targetDir);
      const bytes = await exportPngBytes();
      const res = await api.clipSaveEdited(bytes, false, null, targetDir);
      lastSavedPath = res.saved_path;
      flash("copied", "Сохранено в папку");
    } catch (e) {
      flash("copied", typeof e === "string" ? e : "Ошибка сохранения");
    } finally {
      busy = false;
    }
  }

  async function saveAsPath() {
    if (busy || loading) return;
    busy = true;
    try {
      const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
      const initDir = saveDir || (await api.defaultScreenshotsDir());
      const path = await api.pickSaveImagePath(`kapla_${stamp}.png`, initDir);
      if (!path) return;
      const bytes = await exportPngBytes();
      const res = await api.clipSaveEdited(bytes, false, path, null);
      lastSavedPath = res.saved_path;
      flash("copied", "Файл сохранён");
    } catch (e) {
      flash("copied", typeof e === "string" ? e : "Ошибка сохранения");
    } finally {
      busy = false;
    }
  }

  async function chooseFolder() {
    const picked = await api.pickFolder().catch(() => null);
    if (picked) {
      saveDir = picked;
      saveEditorDir(picked);
      flash("copied", "Папка сохранена");
    }
  }

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && (e.key === "z" || e.key === "Z" || e.key === "я" || e.key === "Я")) {
        e.preventDefault();
        undo();
      } else if (e.ctrlKey && (e.key === "s" || e.key === "S" || e.key === "ы" || e.key === "Ы")) {
        e.preventDefault();
        saveToDir();
      } else if (!textPos && e.ctrlKey && (e.key === "c" || e.key === "C" || e.key === "с" || e.key === "С")) {
        e.preventDefault();
        saveToClipboard();
      } else if (!textPos && !e.ctrlKey && !e.altKey && e.key >= "1" && e.key <= "7") {
        const idx = Number(e.key) - 1;
        if (TOOLS[idx]) {
          tool = TOOLS[idx].id;
          if (tool !== "crop") cropBox = null;
        }
      } else if (e.key === "Enter" && cropBox && cropBox.w >= 10 && cropBox.h >= 10 && !textPos) {
        e.preventDefault();
        applyCrop();
      } else if (e.key === "Escape") {
        if (textPos) {
          textPos = null;
          ui.inputMode = false;
          api.endInput().catch(() => {});
        } else if (cropBox) {
          cropBox = null;
        } else {
          onClose();
        }
      }
    };
    window.addEventListener("keydown", onKey);

    (async () => {
      if (!saveDir) {
        saveDir = await api.defaultScreenshotsDir().catch(() => "");
      }
      const dataUrl = await api
        .clipImageData(clip.id)
        .catch(() => clip.preview ?? "");
      if (!dataUrl) {
        loading = false;
        return;
      }
      const img = new Image();
      img.onload = () => {
        canvasEl.width = img.naturalWidth;
        canvasEl.height = img.naturalHeight;
        imgW = img.naturalWidth;
        imgH = img.naturalHeight;
        ctx = canvasEl.getContext("2d");
        ctx?.drawImage(img, 0, 0);

        origCanvas = document.createElement("canvas");
        origCanvas.width = img.naturalWidth;
        origCanvas.height = img.naturalHeight;
        origCanvas.getContext("2d")?.drawImage(img, 0, 0);
        loading = false;
      };
      img.onerror = () => {
        loading = false;
      };
      img.src = dataUrl;
    })();

    return () => {
      window.removeEventListener("keydown", onKey);
    };
  });
</script>

<div class="ed-wrap">
  <header class="ed-top">
    <div class="ed-title">
      <span class="ed-badge mono">EDITOR</span>
      <span class="ed-dim mono">{imgW && imgH ? `${imgW}×${imgH}` : clip.text}</span>
    </div>

    <div class="ed-tools">
      {#each TOOLS as t}
        <button
          class="ttool"
          class:active={tool === t.id}
          onclick={() => {
            tool = t.id;
            if (t.id !== "crop") cropBox = null;
          }}
          title={t.label}
        >
          {#if t.id === "blur"}
            <span class="blur-ic">▦</span>
          {:else}
            <Icon name={t.icon} size={13} />
          {/if}
          <span>{t.label}</span>
        </button>
      {/each}
    </div>

    <div class="ed-acts">
      <button class="ib" onclick={flipH} title="Отзеркалить по горизонтали">
        <Icon name="convert" size={14} />
      </button>
      <button class="ib" onclick={rotate90} title="Повернуть на 90°">
        <Icon name="rotate" size={14} />
      </button>
      <button class="ib" onclick={undo} disabled={undoCount === 0} title="Отменить (Ctrl+Z)">
        <Icon name="undo" size={14} />
      </button>
      <button class="ib" onclick={resetOriginal} disabled={undoCount === 0} title="Сбросить к оригиналу">
        <Icon name="refresh" size={14} />
      </button>
      <button class="ib close" onclick={onClose} title="Закрыть редактор (Esc)">
        <Icon name="close" size={14} />
      </button>
    </div>
  </header>

  <div class="ed-sub">
    <div class="pal">
      {#each COLORS as c}
        <button
          class="cdot"
          class:active={color === c}
          style:background={c}
          onclick={() => (color = c)}
          aria-label={`Цвет ${c}`}
        ></button>
      {/each}
    </div>

    <div class="sizes">
      {#each SIZES as s}
        <button class="sbtn mono" class:active={size === s.id} onclick={() => (size = s.id)}>{s.label}</button>
      {/each}
    </div>

    {#if cropBox && cropBox.w >= 10 && cropBox.h >= 10}
      <div class="crop-bar">
        <span class="mono">{Math.round(cropBox.w)}×{Math.round(cropBox.h)}</span>
        <button class="crop-ok" onclick={applyCrop}>Обрезать</button>
        <button class="crop-no" onclick={() => (cropBox = null)}>Отмена</button>
      </div>
    {:else if textPos}
      <form class="text-bar" onsubmit={commitText}>
        <input
          bind:this={textInputEl}
          bind:value={textVal}
          type="text"
          placeholder="Текст подписи и Enter…"
          onpointerdown={onTextPointerDown}
          onblur={onTextBlur}
        />
        <button class="crop-ok" type="submit">Вставить</button>
      </form>
    {:else if lastSavedPath}
      <button class="saved-link mono" onclick={() => lastSavedPath && api.revealPath(lastSavedPath).catch(() => {})} title={lastSavedPath}>
        Показать файл ↗
      </button>
    {/if}
  </div>

  <div class="ed-stage">
    {#if loading}
      <div class="ed-loading">Открываю изображение в полном качестве…</div>
    {/if}
    <div class="canvas-box">
      <canvas
        bind:this={canvasEl}
        class="ed-canvas"
        class:hidden={loading}
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
      ></canvas>
      {#if cropBox && cropBox.w > 2 && cropBox.h > 2 && imgW > 0 && imgH > 0}
        <div
          class="crop-rect"
          style:left="{(cropBox.x / imgW) * 100}%"
          style:top="{(cropBox.y / imgH) * 100}%"
          style:width="{(cropBox.w / imgW) * 100}%"
          style:height="{(cropBox.h / imgH) * 100}%"
        ></div>
      {/if}
    </div>
  </div>

  <footer class="ed-foot">
    <button class="dir-pick" onclick={chooseFolder} title={saveDir || "Выбрать папку для быстрого сохранения"}>
      <Icon name="folder" size={13} />
      <span class="mono">{saveDir || "Выбрать папку…"}</span>
    </button>

    <div class="save-btns">
      <button class="sact" onclick={saveAsPath} disabled={busy || loading} title="Выбрать путь и формат (.png / .jpg)">
        <Icon name="save" size={13} />
        <span>Выбрать путь…</span>
      </button>
      <button class="sact" onclick={saveToDir} disabled={busy || loading} title="Быстро сохранить PNG в указанную папку">
        <Icon name="folder" size={13} />
        <span>В папку</span>
      </button>
      <button class="sact pri" onclick={saveToClipboard} disabled={busy || loading}>
        <Icon name="copy" size={13} />
        <span>{busy ? "Сохраняю…" : "В буфер"}</span>
      </button>
    </div>
  </footer>
</div>

<style>
  .ed-wrap {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px 16px;
    animation: edIn 220ms var(--ease);
  }
  @keyframes edIn {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.985);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .ed-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .ed-title {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .ed-badge {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    padding: 2px 7px;
    border-radius: 6px;
    background: rgba(var(--accent-rgb), 0.16);
    color: rgb(var(--accent-rgb));
  }
  .ed-dim {
    font-size: 11px;
    color: var(--muted);
  }
  .ed-tools {
    display: flex;
    gap: 3px;
    padding: 3px;
    border-radius: 11px;
    background: var(--ink-2);
    border: 1px solid var(--line);
  }
  .ttool {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 8px;
    border-radius: 8px;
    font-size: 11.5px;
    color: var(--muted);
    transition: background 130ms var(--ease), color 130ms var(--ease);
  }
  .ttool:hover {
    color: var(--text);
  }
  .ttool.active {
    background: var(--ink-3);
    color: var(--text);
    box-shadow: inset 0 0 0 1px rgba(var(--accent-rgb), 0.45);
  }
  .blur-ic {
    font-size: 12px;
    line-height: 1;
  }
  .ed-acts {
    display: flex;
    gap: 5px;
  }
  .ib {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: var(--ink-2);
    border: 1px solid var(--line);
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .ib:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--line-2);
  }
  .ib.close:hover {
    color: var(--danger);
    border-color: rgba(255, 95, 82, 0.4);
  }
  .ib:disabled {
    opacity: 0.35;
  }

  .ed-sub {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 28px;
  }
  .pal {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 4px 8px;
    border-radius: 99px;
    background: var(--ink-2);
    border: 1px solid var(--line);
  }
  .cdot {
    width: 14px;
    height: 14px;
    border-radius: 99px;
    border: 1px solid rgba(255, 255, 255, 0.22);
    transition: transform 120ms var(--ease);
  }
  .cdot.active {
    transform: scale(1.22);
    box-shadow: 0 0 0 2px #0c0b0a, 0 0 0 3.5px rgb(var(--accent-rgb));
  }
  .sizes {
    display: flex;
    gap: 3px;
    background: var(--ink-2);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 2px;
  }
  .sbtn {
    height: 22px;
    padding: 0 8px;
    border-radius: 6px;
    font-size: 10.5px;
    color: var(--muted);
  }
  .sbtn.active {
    background: var(--ink-3);
    color: rgb(var(--accent-rgb));
  }
  .crop-bar,
  .text-bar {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
  }
  .text-bar input {
    width: 210px;
    height: 26px;
    padding: 0 9px;
    border-radius: 8px;
    background: var(--ink-2);
    border: 1px solid rgba(var(--accent-rgb), 0.55);
    font-size: 12px;
    outline: none;
  }
  .crop-ok,
  .crop-no,
  .saved-link {
    height: 26px;
    padding: 0 10px;
    border-radius: 8px;
    font-size: 11.5px;
  }
  .crop-ok {
    background: rgb(var(--accent-rgb));
    color: #0c0b0a;
    font-weight: 600;
  }
  .crop-no {
    background: var(--ink-3);
    color: var(--muted);
  }
  .saved-link {
    margin-left: auto;
    background: rgba(74, 214, 152, 0.14);
    border: 1px solid rgba(74, 214, 152, 0.35);
    color: var(--ok);
  }

  .ed-stage {
    flex: 1;
    min-height: 0;
    background: radial-gradient(circle at 50% 45%, rgba(255, 255, 255, 0.06), transparent 70%), var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: var(--surface-sheen);
    border-radius: var(--r-m);
    display: grid;
    place-items: center;
    padding: 10px;
    overflow: hidden;
    position: relative;
  }
  .ed-loading {
    font-size: 12.5px;
    color: var(--muted);
  }
  .canvas-box {
    position: relative;
    max-width: 100%;
    max-height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .ed-canvas {
    display: block;
    max-width: 100%;
    max-height: 365px;
    border-radius: 8px;
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.5);
    cursor: crosshair;
    touch-action: none;
    background: #181614;
  }
  .ed-canvas.hidden {
    display: none;
  }
  .crop-rect {
    position: absolute;
    border: 1.5px solid rgb(var(--accent-rgb));
    box-shadow: 0 0 0 9999px rgba(8, 7, 6, 0.56);
    pointer-events: none;
    border-radius: 2px;
  }

  .ed-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .dir-pick {
    flex: 1;
    min-width: 0;
    height: 32px;
    padding: 0 10px;
    border-radius: 9px;
    background: var(--ink-2);
    border: 1px solid var(--line);
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--muted);
    font-size: 11px;
    text-align: left;
  }
  .dir-pick:hover {
    color: var(--text);
    border-color: var(--line-2);
  }
  .dir-pick span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .save-btns {
    display: flex;
    gap: 6px;
    flex: none;
  }
  .sact {
    height: 32px;
    padding: 0 12px;
    border-radius: 9px;
    background: var(--ink-2);
    border: 1px solid var(--line);
    font-size: 12px;
    color: var(--text);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    transition: transform 110ms var(--ease), border-color 140ms var(--ease), background 140ms var(--ease);
  }
  .sact:hover:not(:disabled) {
    background: var(--ink-3);
    border-color: var(--line-2);
  }
  .sact:active:not(:disabled) {
    transform: scale(0.97);
  }
  .sact.pri {
    background: rgb(var(--accent-rgb));
    border-color: transparent;
    color: #0c0b0a;
    font-weight: 600;
  }
  .sact.pri:hover:not(:disabled) {
    filter: brightness(1.07);
  }
  .sact:disabled {
    opacity: 0.45;
  }
</style>
