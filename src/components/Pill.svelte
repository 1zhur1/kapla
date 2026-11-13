<script lang="ts">
  import Bars from "./Bars.svelte";
  import Icon from "./Icon.svelte";
  import { api } from "../lib/ipc";
  import { ui, flash, fmtTime, openImageEditor, savePrefs } from "../lib/state.svelte";

  function openRecentShot(e: MouseEvent) {
    e.stopPropagation();
    const s = ui.recentShot;
    if (!s) return;
    const clip = ui.clips.find((c) => c.id === s.id) ?? {
      id: s.id,
      kind: "image" as const,
      text: s.text,
      preview: s.preview,
      ts: s.at,
      pinned: false,
    };
    ui.flash = null;
    openImageEditor(clip);
  }

  function dismissRecentShot(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    ui.recentShot = null;
  }

  let {
    timerLeft = null,
    showMusic = true,
    onOpen,
  }: {
    timerLeft: number | null;
    showMusic?: boolean;
    onOpen: () => void;
  } = $props();

  const timeStr = $derived(
    new Date(ui.now).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" }),
  );

  let clickTimer: ReturnType<typeof setTimeout> | undefined;

  function onClickPill() {
    clearTimeout(clickTimer);
    clickTimer = setTimeout(() => {
      onOpen();
    }, 210);
  }

  function onDblClickPill(e: MouseEvent) {
    e.preventDefault();
    clearTimeout(clickTimer);
    ui.tiny = !ui.tiny;
    savePrefs();
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const step = e.deltaY < 0 ? 0.04 : -0.04;
    const next = Math.min(1, Math.max(0, Math.round((ui.sys.volume + step) * 100) / 100));
    ui.sys = { ...ui.sys, volume: next, muted: next === 0 };
    api.setVolume(next).catch(() => {});
    flash("volume", `${Math.round(next * 100)}%`);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="pill"
  class:is-tiny={ui.tiny && !ui.flash}
  role="button"
  tabindex="0"
  onclick={onClickPill}
  ondblclick={onDblClickPill}
  onwheel={onWheel}
  title={ui.tiny ? "Клик — открыть · Двойной клик — обычный размер" : "Клик — открыть · Двойной клик — мини-режим (Ctrl+Alt+I)"}
>
  {#if ui.flash}
    {#key ui.flash.at}
      <span class="sweep" aria-hidden="true"></span>
    {/key}
    <div class="flash">
      {#if ui.flash.kind === "volume"}
        <Icon name={ui.sys.muted || ui.sys.volume === 0 ? "mute" : "volume"} size={14} />
        <div class="vtrack"><div class="vfill" style:width="{Math.round(ui.sys.volume * 100)}%"></div></div>
        <span class="mono">{Math.round(ui.sys.volume * 100)}%</span>
      {:else if ui.flash.kind === "layout"}
        <Icon name="keyboard" size={14} />
        <span class="mono tag">{ui.flash.text}</span>
      {:else if ui.flash.kind === "caps"}
        <span class="dot" class:on={ui.sys.caps}></span>
        <span class="mono">{ui.flash.text}</span>
      {:else if ui.flash.kind === "mic"}
        <Icon name={ui.sys.mic_muted ? "micOff" : "mic"} size={14} />
        <span>{ui.flash.text}</span>
      {:else if ui.flash.kind === "color"}
        <Icon name="pipette" size={14} />
        <span class="mono">{ui.flash.text}</span>
      {:else if ui.flash.kind === "shot"}
        {#if ui.recentShot?.preview}
          <button class="shot-chip" onclick={openRecentShot} title="Нажми, чтобы редактировать скриншот">
            <img class="shot-mini" src={ui.recentShot.preview} alt="" />
          </button>
        {:else}
          <Icon name="camera" size={14} />
        {/if}
        <span class="flash-txt">{ui.flash.text}</span>
      {:else if ui.flash.kind === "vpn"}
        <Icon name="shield" size={14} />
        <span>{ui.flash.text}</span>
      {:else}
        <span class="dot on"></span>
        <span>{ui.flash.text ?? "Готово"}</span>
      {/if}
    </div>
  {:else if ui.tiny}
    <div class="tiny">
      <span class="amber-drop"></span>
      <span class="mono tiny-clk">{timeStr}</span>
      {#if ui.media.playing}
        <span class="tiny-play"></span>
      {/if}
    </div>
  {:else if ui.media.active && showMusic}
    <div class="music">
      {#if ui.media.cover}
        <img class="cov" src={ui.media.cover} alt="" />
      {:else}
        <span class="cov empty"><Icon name="music" size={13} /></span>
      {/if}
      <span class="track">
        <span class="t">{ui.media.title}</span>
      </span>
      {#if timerLeft !== null}
        <span class="mono tmr">{fmtTime(timerLeft)}</span>
      {:else}
        <span class="mono clk">{timeStr}</span>
      {/if}
      {#if ui.recentShot?.preview}
        <button class="shot-chip" onclick={openRecentShot} title="Открыть в редакторе">
          <img class="shot-mini" src={ui.recentShot.preview} alt="" />
        </button>
      {/if}
      <Bars playing={ui.media.playing} height={12} />
    </div>
  {:else}
    <div class="idle">
      {#if ui.recentShot?.preview}
        <button
          class="shot-chip"
          onclick={openRecentShot}
          oncontextmenu={dismissRecentShot}
          title="Клик — редактировать картинку · ПКМ — скрыть"
        >
          <img class="shot-mini" src={ui.recentShot.preview} alt="" />
        </button>
      {:else}
        <span class="amber-drop"></span>
      {/if}
      <span class="mono clk">{timeStr}</span>
      {#if timerLeft !== null}
        <span class="sep">·</span>
        <span class="mono tmr">{fmtTime(timerLeft)}</span>
      {:else if ui.sys.battery !== null}
        <span class="sep">·</span>
        <span class="bat" class:chg={ui.sys.charging}>
          {#if ui.sys.charging}<Icon name="bolt" size={11} />{/if}
          <span class="mono">{ui.sys.battery}%</span>
        </span>
      {/if}
      {#if ui.vpn.connected}
        <span class="vpn-pill mono">VPN</span>
      {/if}
      {#if ui.sys.caps}
        <span class="caps mono">CAPS</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .pill {
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 16px;
    color: var(--text);
    cursor: pointer;
  }
  .pill:hover .amber-drop {
    animation-duration: 0.9s;
  }
  .sweep {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 55%;
    pointer-events: none;
    background: linear-gradient(100deg, transparent 0%, rgba(255, 255, 255, 0.16) 50%, transparent 100%);
    transform: translateX(-120%) skewX(-18deg);
    animation: sweep 650ms var(--ease) 40ms forwards;
  }
  @keyframes sweep {
    to {
      transform: translateX(240%) skewX(-18deg);
    }
  }
  .flash {
    animation: flashIn 240ms var(--ease) !important;
  }
  .flash > :global(svg.i:first-child),
  .flash > .dot:first-child,
  .flash > .shot-chip:first-child {
    animation: iconBounce 280ms var(--ease-spring);
    color: rgb(var(--accent-rgb));
  }
  @keyframes flashIn {
    from {
      opacity: 0;
      transform: scale(0.95);
    }
  }
  @keyframes iconBounce {
    0% {
      transform: scale(0.75);
    }
    65% {
      transform: scale(1.08);
    }
    100% {
      transform: scale(1);
    }
  }
  .pill.is-tiny {
    padding: 0 10px;
  }
  .tiny {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    width: 100%;
    font-size: 10.5px;
    line-height: 1;
  }
  .tiny .amber-drop {
    width: 5px;
    height: 5px;
  }
  .tiny-clk {
    color: var(--text);
    font-weight: 500;
  }
  .tiny-play {
    width: 4px;
    height: 4px;
    border-radius: 99px;
    background: var(--ok);
    flex: none;
  }
  .idle,
  .music,
  .flash {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    justify-content: center;
    font-size: 12.5px;
    animation: kaplaPopIn 180ms var(--ease);
  }
  .flash-txt {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .shot-chip {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    padding: 0;
    overflow: hidden;
    border: 1px solid rgba(var(--accent-rgb), 0.7);
    box-shadow: 0 0 10px rgba(var(--accent-rgb), 0.35);
    flex: none;
    transition: transform 150ms var(--ease-spring), box-shadow 150ms var(--ease);
  }
  .shot-chip:hover {
    transform: scale(1.12);
    box-shadow: 0 0 14px rgba(var(--accent-rgb), 0.6);
  }
  .shot-mini {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .amber-drop {
    width: 7px;
    height: 7px;
    border-radius: 50% 50% 50% 15%;
    transform: rotate(-45deg);
    background: linear-gradient(135deg, #fff 0%, rgb(var(--accent-rgb)) 60%);
    box-shadow: 0 0 10px rgba(var(--accent-rgb), 0.85);
    animation: kaplaDropBeat 2.8s ease-in-out infinite;
    flex: none;
  }
  @keyframes kaplaDropBeat {
    0%,
    100% {
      transform: rotate(-45deg) scale(1);
      opacity: 0.92;
      box-shadow: 0 0 6px rgba(var(--accent-rgb), 0.7), 0 0 0 0 rgba(var(--accent-rgb), 0.45);
    }
    50% {
      transform: rotate(-45deg) scale(1.22);
      opacity: 1;
      box-shadow: 0 0 12px rgba(var(--accent-rgb), 0.95), 0 0 0 5px rgba(var(--accent-rgb), 0);
    }
  }
  .clk {
    font-weight: 500;
    letter-spacing: 0.01em;
  }
  .sep {
    color: var(--faint);
  }
  .bat {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--muted);
    font-size: 11.5px;
  }
  .bat.chg {
    color: var(--ok);
  }
  .caps {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 6px;
    background: rgba(var(--accent-rgb), 0.2);
    border: 1px solid rgba(var(--accent-rgb), 0.36);
    color: rgb(var(--accent-rgb));
  }
  .vpn-pill {
    font-size: 9.5px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 6px;
    background: rgba(126, 232, 162, 0.16);
    border: 1px solid rgba(126, 232, 162, 0.34);
    color: var(--ok);
  }
  .tmr {
    color: rgb(var(--accent-rgb));
    font-weight: 600;
    font-size: 12px;
  }
  .music {
    justify-content: space-between;
    gap: 9px;
  }
  .cov {
    width: 20px;
    height: 20px;
    border-radius: 6px;
    object-fit: cover;
    border: 1px solid rgba(255, 255, 255, 0.2);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.28);
    flex: none;
  }
  .cov.empty {
    display: grid;
    place-items: center;
    background: var(--glass-bg);
    color: var(--muted);
  }
  .track {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }
  .t {
    display: block;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
    font-weight: 500;
  }
  .vtrack {
    flex: 1;
    max-width: 90px;
    height: 5px;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.14);
    overflow: hidden;
  }
  .vfill {
    height: 100%;
    border-radius: 99px;
    background: linear-gradient(90deg, rgba(var(--accent-rgb), 0.75), rgb(var(--accent-rgb)));
    box-shadow: 0 0 8px rgba(var(--accent-rgb), 0.45);
    transition: width 80ms linear;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 99px;
    background: var(--faint);
  }
  .dot.on {
    background: rgb(var(--accent-rgb));
    box-shadow: 0 0 8px rgba(var(--accent-rgb), 0.65);
  }
  .tag {
    padding: 1px 7px;
    border-radius: 7px;
    background: var(--glass-bg-strong);
    border: 1px solid var(--line-2);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.2);
  }
</style>
