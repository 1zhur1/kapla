<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Pill from "./components/Pill.svelte";
  import Panel from "./components/Panel.svelte";
  import { Liquid } from "./lib/liquid";
  import { dominantColor } from "./lib/color";
  import { api, isTauri, on, WIN_H, WIN_W, type Clip, type ClipCapturedEvt, type CursorEvt, type MediaInfo, type Sys, type VpnStatus } from "./lib/ipc";
  import { ACCENT_PRESETS, flash, saveShelf, ui } from "./lib/state.svelte";

  const CX = WIN_W / 2;
  const panelSize = $derived(
    ui.editorClip
      ? { w: 844, h: 522 }
      : ui.tab === "browser"
        ? { w: 824, h: 520 }
        : ui.tab === "controls"
          ? { w: 760, h: 438 }
          : { w: 754, h: 392 }
  );

  let canvas: HTMLCanvasElement;
  let liq: Liquid | undefined;

  // Размер и положение формы на текущем кадре — синхронизированы с физикой капли.
  let boxW = $state(176);
  let boxH = $state(34);
  let timerLeft = $state<number | null>(null);
  let timerId: ReturnType<typeof setInterval> | undefined;
  let musicFolded = $state(false);

  const showMusicPill = $derived(ui.media.active && !musicFolded);

  const targetShape = $derived.by(() => {
    const vpnPad = ui.vpn.connected ? 38 : 0;
    const shotPad = !ui.tiny && ui.recentShot ? 24 : 0;
    if (ui.open) return { w: panelSize.w, h: panelSize.h, r: 26 };
    if (ui.flash) {
      return ui.flash.kind === "shot" && ui.recentShot
        ? { w: 264, h: 38, r: 18 }
        : { w: 238, h: 36, r: 17 };
    }
    if (ui.tiny) return { w: 72, h: 20, r: 10 };
    if (showMusicPill) return { w: 286 + shotPad, h: 38, r: 18 };
    if (timerLeft !== null) return { w: 196 + vpnPad + shotPad, h: 34, r: 16 };
    if (ui.sys.battery !== null) return { w: 188 + vpnPad + shotPad, h: 34, r: 16 };
    return { w: 136 + vpnPad + shotPad, h: 34, r: 16 };
  });

  function startTimer(sec: number) {
    clearInterval(timerId);
    timerLeft = sec;
    liq?.drop();
    timerId = setInterval(() => {
      if (timerLeft === null) return;
      if (timerLeft <= 1) {
        clearInterval(timerId);
        timerLeft = null;
        liq?.drop();
        flash("copied", "Таймер готов!", 3500);
        return;
      }
      timerLeft -= 1;
    }, 1000);
  }

  function stopTimer() {
    clearInterval(timerId);
    timerLeft = null;
  }

  let leaveTimer: ReturnType<typeof setTimeout> | undefined;
  let lastCursor: CursorEvt | null = null;

  function isInsidePanel(cur: CursorEvt | null) {
    return (
      !!cur &&
      cur.near &&
      cur.x >= CX - panelSize.w / 2 - 12 &&
      cur.x <= CX + panelSize.w / 2 + 12 &&
      cur.y >= 0 &&
      cur.y <= panelSize.h + 14
    );
  }

  function scheduleLeave() {
    if (ui.open && !ui.pinned && !ui.inputMode && !leaveTimer) {
      leaveTimer = setTimeout(() => {
        leaveTimer = undefined;
        if (ui.open && !ui.pinned && !ui.inputMode) {
          ui.open = false;
        }
      }, 280);
    }
  }

  $effect(() => {
    const _tab = ui.tab;
    untrack(() => {
      if (ui.open) {
        liq?.pulseRipple(CX, 24);
        liq?.flare(0.85);
      }
      if (ui.inputMode) {
        ui.inputMode = false;
        api.endInput().catch(() => {});
      }
    });
    void _tab;
  });

  $effect(() => {
    if (ui.open && !ui.pinned && !ui.inputMode && lastCursor && !isInsidePanel(lastCursor)) {
      scheduleLeave();
    }
  });

  let prevOpen = false;
  $effect(() => {
    liq?.setShape(targetShape);
    if (liq) liq.open = ui.open;
    if (ui.open && !prevOpen) {
      liq?.pulseRipple(CX, 22);
      liq?.flare(1.1);
    }
    prevOpen = ui.open;
    if (ui.open) {
      ui.now = Date.now();
      if (ui.editorClip || ui.tab !== "browser") {
        api.browserHide().catch(() => {});
      }
      api
        .setHitRect({
          x: CX - panelSize.w / 2 - 8,
          y: 0,
          w: panelSize.w + 16,
          h: panelSize.h + 14,
        })
        .catch(() => {});
    } else {
      ui.dragOver = false;
      ui.editorClip = null;
      untrack(() => {
        if (ui.inputMode) {
          ui.inputMode = false;
          api.endInput().catch(() => {});
        }
      });
      api.browserHide().catch(() => {});
    }
  });

  // Сворачиваем музыкальную пилюлю через 8 секунд после паузы, чтобы не занимала место.
  $effect(() => {
    if (!ui.media.active || ui.media.playing) {
      musicFolded = false;
      return;
    }
    const id = setTimeout(() => {
      musicFolded = true;
    }, 8000);
    return () => clearTimeout(id);
  });

  $effect(() => {
    liq?.setMusic(ui.media.active && ui.media.playing, ui.accent);
    document.documentElement.style.setProperty("--accent-rgb", ui.accent.join(","));
  });

  // Деликатная вспышка кромки при уведомлениях в капсуле
  $effect(() => {
    if (ui.flash) untrack(() => liq?.flare(ui.flash?.kind === "shot" ? 0.85 : 0.45));
  });

  $effect(() => {
    if (ui.accentPreset !== "auto") {
      ui.accent = ACCENT_PRESETS[ui.accentPreset];
      return;
    }
    const src = ui.media.cover;
    if (!src) {
      ui.accent = [255, 122, 61];
      return;
    }
    dominantColor(src).then((c) => {
      if (ui.accentPreset === "auto" && c) ui.accent = c;
    });
  });

  onMount(() => {
    liq = new Liquid(canvas, CX, WIN_W, WIN_H);
    let lastHitKey = "";
    liq.onFrame = (w, h) => {
      if (Math.abs(w - boxW) > 0.25 || Math.abs(h - boxH) > 0.25) {
        boxW = w;
        boxH = h;
      }
      if (ui.open) return;
      const key = `${Math.round(w / 6)}:${Math.round(h / 6)}`;
      if (key !== lastHitKey) {
        lastHitKey = key;
        api
          .setHitRect({
            x: CX - w / 2 - 6,
            y: 0,
            w: w + 12,
            h: h + 12,
          })
          .catch(() => {});
      }
    };
    liq.setShape(targetShape);

    const clockId = setInterval(() => (ui.now = Date.now()), 10000);
    let recentShotTimer: ReturnType<typeof setTimeout> | undefined;
    let hoverTimer: ReturnType<typeof setTimeout> | undefined;
    let prevSys: Sys | null = null;
    let unsubs: (() => void)[] = [];

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && ui.open && !ui.inputMode && !ui.editorClip) {
        ui.open = false;
        ui.pinned = false;
      }
    };
    const onPointerDownCapture = (e: PointerEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, textarea, [contenteditable]")) {
        ui.inputMode = true;
        api.beginInput().catch(() => {});
      } else if (ui.inputMode) {
        ui.inputMode = false;
        api.endInput().catch(() => {});
      }
    };
    const onDocMouseLeave = () => {
      if (hoverTimer) {
        clearTimeout(hoverTimer);
        hoverTimer = undefined;
      }
      scheduleLeave();
    };

    const onWindowBlur = () => {
      if (ui.inputMode && ui.tab !== "browser") {
        ui.inputMode = false;
        api.endInput().catch(() => {});
      }
      if (ui.open && !ui.pinned && !ui.inputMode && !isInsidePanel(lastCursor)) {
        scheduleLeave();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("pointerdown", onPointerDownCapture, true);
    window.addEventListener("blur", onWindowBlur);
    document.addEventListener("mouseleave", onDocMouseLeave);

    async function addPathsToShelf(paths: string[]) {
      if (!paths || paths.length === 0) return;
      ui.open = true;
      ui.pinned = true;
      ui.tab = "shelf";
      ui.dragOver = false;
      liq?.drop();
      const metas = await api.shelfMeta(paths).catch(() => []);
      const existing = new Set(ui.shelf.map((s) => s.path));
      const fresh = metas.filter((m) => !existing.has(m.path));
      if (fresh.length > 0) {
        ui.shelf = [...fresh, ...ui.shelf].slice(0, 80);
        saveShelf();
        flash("copied", `На полку: +${fresh.length}`);
      }
    }

    (async () => {
      const dragUnsub = isTauri
        ? await getCurrentWebview()
            .onDragDropEvent(async (ev) => {
              const p = ev.payload;
              if (p.type === "enter" || p.type === "over") {
                ui.open = true;
                ui.tab = "shelf";
                ui.dragOver = true;
              } else if (p.type === "leave") {
                ui.dragOver = false;
              } else if (p.type === "drop" && p.paths.length > 0) {
                await addPathsToShelf(p.paths);
              }
            })
            .catch(() => () => {})
        : () => {};

      unsubs = await Promise.all([
        on<string>("shelf_drag", (kind) => {
          if (kind === "enter") {
            ui.open = true;
            ui.tab = "shelf";
            ui.dragOver = true;
          } else {
            ui.dragOver = false;
          }
        }),
        on<string[]>("shelf_drop", (paths) => {
          addPathsToShelf(paths);
        }),
        on<MediaInfo>("media", (info) => {
          const wasActive = ui.media.active;
          ui.media = info;
          if (!wasActive && info.active) liq?.drop();
        }),
        on<number>("audio_level", (lvl) => {
          ui.audioLevel = lvl;
          liq?.setAudioLevel(lvl);
        }),
        on<boolean>("mic_toggled", (muted) => {
          ui.sys = { ...ui.sys, mic_muted: muted };
          flash("mic", muted ? "Микрофон выкл" : "Микрофон вкл");
        }),
        on<Sys>("sys", (sys) => {
          if (prevSys) {
            if (sys.layout && sys.layout !== prevSys.layout) flash("layout", sys.layout);
            else if (sys.caps !== prevSys.caps) flash("caps", sys.caps ? "CAPS ON" : "caps off");
            else if (Math.abs(sys.volume - prevSys.volume) >= 0.01 || sys.muted !== prevSys.muted) {
              flash("volume", `${Math.round(sys.volume * 100)}%`);
            } else if (sys.mic_muted !== prevSys.mic_muted) {
              flash("mic", sys.mic_muted ? "Микрофон выкл" : "Микрофон вкл");
            }
          }
          prevSys = sys;
          ui.sys = sys;
        }),
        on<Clip[]>("clips", (clips) => {
          ui.clips = clips;
          if (ui.recentShot && !clips.some((c) => c.id === ui.recentShot?.id)) {
            ui.recentShot = null;
          }
        }),
        on<ClipCapturedEvt>("clip_captured", (ev) => {
          liq?.drop();
          if (ev.kind === "image") {
            const img = ui.clips.find((c) => c.kind === "image" && c.preview);
            if (img && img.preview) {
              ui.recentShot = {
                id: img.id,
                preview: img.preview,
                text: img.text,
                at: Date.now(),
              };
              clearTimeout(recentShotTimer);
              recentShotTimer = setTimeout(() => {
                ui.recentShot = null;
              }, 90000);
            }
            flash("shot", "Скрин · клик — ред.", 2500);
          } else {
            const snippet = ev.text.replace(/\s+/g, " ").trim().slice(0, 34);
            flash("copied", snippet ? `Копия: ${snippet}` : "Скопировано");
          }
        }),
        on<VpnStatus>("vpn", (v) => {
          ui.vpn = v;
        }),
        on<string>("panel", (cmd) => {
          if (cmd === "toggle") {
            ui.open = !ui.open;
            ui.pinned = ui.open;
          } else if (cmd === "open") {
            ui.open = true;
            ui.pinned = true;
          } else if (cmd === "clipboard") {
            ui.tab = "clipboard";
            ui.open = true;
            ui.pinned = true;
          }
        }),
        on<CursorEvt>("cursor", (cur) => {
          lastCursor = cur;
          liq?.setCursor(cur.x, cur.y, cur.near);
          const half = boxW / 2;
          const inPill =
            cur.near && cur.x >= CX - half - 10 && cur.x <= CX + half + 10 && cur.y >= 0 && cur.y <= boxH + 12;

          if (!ui.open) {
            if (leaveTimer) {
              clearTimeout(leaveTimer);
              leaveTimer = undefined;
            }
            if (cur.dragging && cur.near && cur.y >= -4 && cur.y <= 85 && Math.abs(cur.x - CX) <= 260) {
              clearTimeout(hoverTimer);
              hoverTimer = undefined;
              ui.open = true;
              ui.tab = "shelf";
              ui.dragOver = true;
              return;
            }
            if (ui.through) {
              if (hoverTimer) {
                clearTimeout(hoverTimer);
                hoverTimer = undefined;
              }
              return;
            }
            if (!ui.tiny && (inPill || (cur.edge && Math.abs(cur.x - CX) < half + 20))) {
              if (!hoverTimer) {
                hoverTimer = setTimeout(() => {
                  hoverTimer = undefined;
                  if (
                    lastCursor &&
                    lastCursor.near &&
                    (Math.abs(lastCursor.x - CX) <= boxW / 2 + 20 && lastCursor.y <= boxH + 16)
                  ) {
                    ui.open = true;
                  }
                }, 170);
              }
            } else if (hoverTimer) {
              clearTimeout(hoverTimer);
              hoverTimer = undefined;
            }
          } else if (!ui.pinned && !ui.inputMode) {
            if (!isInsidePanel(cur)) {
              scheduleLeave();
            } else if (leaveTimer) {
              clearTimeout(leaveTimer);
              leaveTimer = undefined;
            }
          }
        }),
      ]);
      unsubs.push(dragUnsub);

      const [m, s, c, v, w] = await Promise.all([
        api.mediaNow().catch(() => null),
        api.sysNow().catch(() => null),
        api.clips().catch(() => []),
        api.vpnStatus().catch(() => null),
        api.weatherNow(ui.weatherCity || null).catch(() => null),
      ]);
      if (m) ui.media = m;
      if (s) {
        ui.sys = s;
        prevSys = s;
      }
      ui.clips = c;
      if (v) ui.vpn = v;
      if (w) ui.weather = w;
    })();

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("pointerdown", onPointerDownCapture, true);
      window.removeEventListener("blur", onWindowBlur);
      document.removeEventListener("mouseleave", onDocMouseLeave);
      clearInterval(clockId);
      clearTimeout(recentShotTimer);
      clearTimeout(hoverTimer);
      clearTimeout(leaveTimer);
      clearInterval(timerId);
      unsubs.forEach((u) => u());
    };
  });
</script>

<div class="stage">
  <canvas bind:this={canvas}></canvas>

  <div
    class="shell"
    class:glass-open={ui.open}
    style:left="{CX - boxW / 2}px"
    style:width="{boxW}px"
    style:height="{boxH}px"
    style:border-radius="0 0 {ui.open ? 26 : 18}px {ui.open ? 26 : 18}px"
  >
    {#if ui.open}
      <div
        class="panel-frame"
        style:width="{panelSize.w}px"
        style:height="{panelSize.h}px"
        style:opacity={Math.min(1, Math.max(0, (boxW - 260) / 300))}
      >
        <Panel
          {timerLeft}
          onStartTimer={startTimer}
          onStopTimer={stopTimer}
          onClose={() => {
            ui.open = false;
            ui.pinned = false;
          }}
        />
      </div>
    {:else}
      <Pill {timerLeft} showMusic={showMusicPill} onOpen={() => ((ui.open = true), (ui.pinned = true))} />
    {/if}
  </div>
</div>

<style>
  .stage {
    position: relative;
    width: 860px;
    height: 540px;
  }
  canvas {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .shell {
    position: absolute;
    top: 0;
    overflow: hidden;
    background: transparent;
  }
  .shell.glass-open {
    background:
      radial-gradient(110% 70% at 15% 0%, rgba(255, 255, 255, 0.08) 0%, transparent 55%),
      radial-gradient(90% 60% at 85% 100%, rgba(var(--accent-rgb), 0.09) 0%, transparent 65%);
    backdrop-filter: blur(22px) saturate(160%);
  }
  .panel-frame {
    position: absolute;
    top: 0;
    left: 50%;
    transform: translateX(-50%);
  }
</style>
