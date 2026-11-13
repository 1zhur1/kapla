<script lang="ts">
  import Icon from "./Icon.svelte";
  import { api } from "../lib/ipc";
  import { ui, flash, fmtTime, openImageEditor, type Tab } from "../lib/state.svelte";

  let { onTab }: { onTab: (t: Tab) => void } = $props();

  // Живая интерполяция позиции трека, пока он играет.
  let pos = $state(0);
  $effect(() => {
    pos = ui.media.position;
    if (!ui.media.active || !ui.media.playing) return;
    const id = setInterval(() => {
      pos = ui.media.duration > 0 ? Math.min(ui.media.duration, pos + 0.5) : pos + 0.5;
    }, 500);
    return () => clearInterval(id);
  });

  // Локальные тики часов только пока открыт спокойный экран «Главной»
  let quietNow = $state(Date.now());
  $effect(() => {
    if (ui.media.active) return;
    quietNow = Date.now();
    const id = setInterval(() => {
      quietNow = Date.now();
    }, 1000);
    return () => clearInterval(id);
  });

  // Плавный тик для симметричной звуковой волны (waveform)
  let waveTick = $state(0);
  $effect(() => {
    if (!ui.media.active || !ui.media.playing) return;
    const id = setInterval(() => {
      waveTick = (waveTick + 1) % 3600;
    }, 110);
    return () => clearInterval(id);
  });

  const WAVE_BASE = [
    18, 28, 42, 34, 60, 48, 76, 56, 88, 66, 94, 72, 62, 84, 52, 92,
    78, 96, 68, 86, 58, 82, 46, 70, 54, 76, 44, 62, 38, 48, 30, 22,
  ];
  const waveBars = $derived(
    WAVE_BASE.map((base, i) => {
      if (!ui.media.playing) return Math.max(14, Math.round(base * 0.35));
      const lvl = 0.72 + Math.min(0.65, ui.audioLevel * 0.85);
      const osc = Math.sin(waveTick * 0.36 + i * 0.65) * 16 + Math.cos(waveTick * 0.22 - i * 0.4) * 10;
      return Math.max(16, Math.min(100, Math.round(base * lvl + osc)));
    }),
  );

  const progress = $derived(ui.media.duration > 0 ? Math.min(100, (pos / ui.media.duration) * 100) : 0);
  const dateLabel = $derived(
    new Date(quietNow).toLocaleDateString("ru-RU", { weekday: "long", day: "numeric", month: "long" }),
  );
  const timeLabel = $derived(
    new Date(quietNow).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit", second: "2-digit" }),
  );

  function onVolInput(e: Event) {
    const val = Number((e.currentTarget as HTMLInputElement).value) / 100;
    ui.sys = { ...ui.sys, volume: val, muted: val === 0 };
    api.setVolume(val).catch(() => {});
  }

  async function pasteLatest(id: number) {
    try {
      await api.paste(id);
      ui.open = false;
      ui.pinned = false;
      flash("copied", "Вставлено");
    } catch {
      flash("copied", "Ошибка вставки");
    }
  }

  function doSnip() {
    ui.open = false;
    ui.pinned = false;
    api.openSnip().catch(() => {});
  }

  async function doShot() {
    try {
      ui.open = false;
      ui.pinned = false;
      await api.captureScreen();
      const topImg = ui.clips.find((c) => c.kind === "image" && c.preview);
      if (topImg?.preview) {
        ui.recentShot = {
          id: topImg.id,
          preview: topImg.preview,
          text: topImg.text,
          at: Date.now(),
        };
      }
      flash("shot", "Скрин · клик — ред.", 2400);
    } catch {
      flash("shot", "Не удалось сделать скриншот");
    }
  }

  async function doColor() {
    ui.open = false;
    ui.pinned = false;
    flash("color", "Кликни пиксель (Esc — отмена)", 12000);
    try {
      const hex = await api.pickColor();
      if (hex) flash("color", `${hex} скопирован`, 2200);
      else ui.flash = null;
    } catch {
      flash("color", "Ошибка пипетки");
    }
  }

  async function doMic() {
    try {
      const muted = await api.toggleMic();
      ui.sys = { ...ui.sys, mic_muted: muted };
      flash("mic", muted ? "Микрофон выкл" : "Микрофон вкл");
    } catch {
      flash("mic", "Нет активного микрофона");
    }
  }

  async function doVpn() {
    if (!ui.vpn.configured) {
      onTab("controls");
      return;
    }
    try {
      const next = await api.vpnToggle();
      ui.vpn = next;
      const label = next.selected?.name ?? "VPN";
      flash("vpn", next.connected ? `${label}: вкл` : `${label}: выкл`);
    } catch (e) {
      flash("vpn", typeof e === "string" ? e : "Ошибка VPN", 2600);
    }
  }

  let editingCity = $state(false);
  let cityDraft = $state(ui.weatherCity);
  let weatherBusy = $state(false);
  let weatherErr = $state("");
  let cityBlurTimer: number | undefined;

  function weatherEmoji(kind?: string): string {
    switch (kind) {
      case "sun":
        return "☀️";
      case "partly":
        return "⛅";
      case "rain":
        return "🌧️";
      case "snow":
        return "❄️";
      case "storm":
        return "⛈️";
      case "fog":
        return "🌫️";
      default:
        return "☁️";
    }
  }

  async function loadWeather(force = false) {
    if (weatherBusy) return;
    weatherBusy = true;
    weatherErr = "";
    try {
      ui.weather = await api.weatherNow(ui.weatherCity || null, force);
    } catch (e) {
      weatherErr = typeof e === "string" ? e : "Не удалось загрузить погоду";
    } finally {
      weatherBusy = false;
    }
  }

  function startEditCity() {
    cityDraft = ui.weatherCity || ui.weather?.city || "";
    editingCity = true;
  }

  function onCityPointerDown(e: PointerEvent) {
    clearTimeout(cityBlurTimer);
    ui.inputMode = true;
    api.beginInput().catch(() => {});
    (e.currentTarget as HTMLElement | null)?.focus();
  }

  function onCityFocus() {
    clearTimeout(cityBlurTimer);
    ui.inputMode = true;
    api.beginInput().catch(() => {});
  }

  function onCityBlur() {
    clearTimeout(cityBlurTimer);
    cityBlurTimer = window.setTimeout(() => {
      if (!document.hasFocus() || !document.activeElement?.closest("input, textarea, [contenteditable]")) {
        ui.inputMode = false;
        api.endInput().catch(() => {});
      }
    }, 50);
  }

  async function saveCity(e?: Event) {
    e?.preventDefault();
    const cleaned = cityDraft.trim();
    ui.weatherCity = cleaned;
    try {
      localStorage.setItem("kapla.weatherCity", JSON.stringify(cleaned));
    } catch {}
    editingCity = false;
    ui.inputMode = false;
    api.endInput().catch(() => {});
    await loadWeather(true);
  }
</script>

<div class="grid">
  <!-- Левая колонка: плеер/часы сверху + разделённая погода снизу -->
  <div class="left-col">
    <section class="card stage">
      {#if ui.media.active}
        <div class="media-top">
          {#if ui.media.cover}
            <img class="art" src={ui.media.cover} alt="" />
          {:else}
            <div class="art empty"><Icon name="music" size={24} /></div>
          {/if}

          <div class="meta">
            <div class="title-row">
              <div class="title" title={ui.media.title}>{ui.media.title}</div>
              <span class="app-pill" title={ui.media.app || "Медиа"}>
                <span class="live-dot" class:on={ui.media.playing}></span>
                <span class="app-txt">{ui.media.app || "Музыка"}</span>
              </span>
            </div>
            <div class="artist" title={ui.media.artist}>{ui.media.artist || "Неизвестный исполнитель"}</div>
            <div class="transport">
              <button class="tbtn" onclick={() => api.media("prev")} aria-label="Предыдущий трек">
                <Icon name="prev" size={15} />
              </button>
              <button
                class="tbtn play"
                class:playing={ui.media.playing}
                onclick={() => api.media("toggle")}
                aria-label={ui.media.playing ? "Пауза" : "Играть"}
              >
                <Icon name={ui.media.playing ? "pause" : "play"} fill size={16} />
              </button>
              <button class="tbtn" onclick={() => api.media("next")} aria-label="Следующий трек">
                <Icon name="next" size={15} />
              </button>
            </div>
          </div>
        </div>

        <div class="waveform" aria-hidden="true">
          {#each waveBars as h, i}
            <span
              class="wbar"
              class:played={i / waveBars.length <= progress / 100}
              style:height="{h}%"
            ></span>
          {/each}
        </div>

        <div class="progress-wrap">
          <div class="pbar"><div class="pfill" style:width="{progress}%"></div></div>
          <div class="times mono">
            <span>{fmtTime(pos)}</span>
            <span>{ui.media.duration > 0 ? `-${fmtTime(Math.max(0, ui.media.duration - pos))}` : "--:--"}</span>
          </div>
        </div>
      {:else}
        <div class="quiet">
          <div class="quiet-top">
            <span class="quiet-date">{dateLabel}</span>
            <span class="status-pill"><span class="live-dot"></span>Эфир чист</span>
          </div>
          <div class="big-clock mono">{timeLabel}</div>
          <p class="quiet-sub">Включи музыку в плеере или браузере — здесь появятся обложка, волна и управление.</p>
          <div class="quiet-chips">
            <span class="qchip">Раскладка <b class="mono">{ui.sys.layout || "—"}</b></span>
            <button
              class="qchip qchip-btn"
              class:ok={ui.vpn.connected}
              onclick={doVpn}
              title={ui.vpn.selected ? `${ui.vpn.selected.name}` : "Настроить VPN в Пульте"}
            >
              <Icon name="shield" size={11} />
              <span>VPN</span>
              <b class="mono">{ui.vpn.configured ? (ui.vpn.connected ? "ВКЛ" : "ВЫКЛ") : "+"}</b>
            </button>
          </div>
        </div>
      {/if}
    </section>

    <!-- Погода: 2-секционный виджет как на hero-preview -->
    <div class="wx-row">
      {#if editingCity}
        <section class="card wx-edit-card">
          <form class="wx-city-form" onsubmit={saveCity}>
            <input
              class="wx-input"
              type="text"
              bind:value={cityDraft}
              placeholder="Город (например: Moscow или пусто — авто)"
              onpointerdown={onCityPointerDown}
              onfocus={onCityFocus}
              onblur={onCityBlur}
              onkeydown={(e) => {
                if (e.key === "Escape") {
                  editingCity = false;
                  ui.inputMode = false;
                  api.endInput().catch(() => {});
                }
              }}
            />
            <button class="wx-act" type="submit">OK</button>
          </form>
        </section>
      {:else if ui.weather}
        <section class="card wx-main-card">
          <div class="wx-top">
            <span class="wx-ico">{weatherEmoji(ui.weather.icon)}</span>
            <span class="wx-big-temp mono">{ui.weather.temp_c > 0 ? `+${ui.weather.temp_c}` : ui.weather.temp_c}°</span>
          </div>
          <div class="wx-label">Погода</div>
          <div class="wx-cond" title={ui.weather.desc}>{ui.weather.desc}</div>
        </section>

        <section class="card wx-detail-card">
          <div class="wx-dhead">
            <button class="wx-city-btn" onclick={startEditCity} title="Сменить город">
              {ui.weather.city || ui.weatherCity || "Город"}
            </button>
            <button class="wx-ref" onclick={() => loadWeather(true)} disabled={weatherBusy} title="Обновить погоду">
              <Icon name="refresh" size={12} />
            </button>
          </div>
          <div class="wx-drows mono">
            <div class="wx-drow">
              <span>Ощущается</span>
              <b>{ui.weather.feels_like_c > 0 ? `+${ui.weather.feels_like_c}` : ui.weather.feels_like_c}°</b>
            </div>
            {#if ui.weather.min_c != null && ui.weather.max_c != null}
              <div class="wx-drow">
                <span>Диапазон</span>
                <b>{ui.weather.min_c}..{ui.weather.max_c}°</b>
              </div>
            {/if}
            <div class="wx-drow">
              <span>Ветер</span>
              <b>{ui.weather.wind_kmph} км/ч</b>
            </div>
            <div class="wx-drow">
              <span>Влажность</span>
              <b>{ui.weather.humidity}%</b>
            </div>
          </div>
        </section>
      {:else}
        <section class="card wx-empty-card">
          <button class="wx-load" onclick={() => loadWeather(true)} disabled={weatherBusy}>
            <Icon name="cloud" size={14} />
            <span>{weatherBusy ? "Загружаю погоду…" : weatherErr ? `${weatherErr} — повторить` : "Показать погоду"}</span>
          </button>
          <button class="wx-city-btn" onclick={startEditCity}>Город</button>
        </section>
      {/if}
    </div>
  </div>

  <!-- Правая колонка: пилюля громкости, 2x2 быстрые кнопки, буфер обмена -->
  <div class="side">
    <section class="card vol-card">
      <button class="mute-btn" class:muted={ui.sys.muted} onclick={() => api.toggleMute()} aria-label="Выключить звук">
        <Icon name={ui.sys.muted || ui.sys.volume === 0 ? "mute" : "volume"} size={16} />
      </button>
      <input
        class="slider"
        type="range"
        min="0"
        max="100"
        value={Math.round(ui.sys.volume * 100)}
        style:--vol-pct="{ui.sys.muted ? 0 : Math.round(ui.sys.volume * 100)}%"
        oninput={onVolInput}
        aria-label="Громкость"
      />
      <span class="mono val">{ui.sys.muted ? "MUTE" : `${Math.round(ui.sys.volume * 100)}%`}</span>
    </section>

    <section class="card qa-card">
      <div class="qa-grid">
        <button class="qbtn" onclick={doSnip} title="Ножницы Windows (вырезать область)">
          <Icon name="scissors" size={14} />
          <span>Ножницы</span>
        </button>
        <button class="qbtn" onclick={doShot} title="Скриншот всего экрана -> Картинки/Kapla + буфер">
          <Icon name="camera" size={14} />
          <span>Скриншот</span>
        </button>
        <button class="qbtn" onclick={doColor} title="Пипетка: кликни любой пиксель экрана -> HEX">
          <Icon name="pipette" size={14} />
          <span>Пипетка</span>
        </button>
        <button class="qbtn" class:warn={ui.sys.mic_muted} onclick={doMic} title="Микрофон (Ctrl+Alt+M)">
          <Icon name={ui.sys.mic_muted ? "micOff" : "mic"} size={14} />
          <span>{ui.sys.mic_muted ? "Мик выкл" : "Микрофон"}</span>
        </button>
      </div>
    </section>

    <section class="card recent">
      <div class="card-h">
        <span>Буфер обмена</span>
        <button class="link" onclick={() => onTab("clipboard")}>Все ({ui.clips.length}) →</button>
      </div>
      {#if ui.clips.length === 0}
        <div class="empty-clips">Скопируй текст или картинку — клик здесь сразу вставит в активное окно.</div>
      {:else}
        <div class="clip-mini-list">
          {#each ui.clips.slice(0, 3) as clip, idx (clip.id)}
            <div class="clip-row">
              <button
                class="clip-mini"
                class:first={idx === 0}
                onclick={() => pasteLatest(clip.id)}
                title={clip.text}
              >
                {#if clip.kind === "image" && clip.preview}
                  <img class="mini-thumb" src={clip.preview} alt="" />
                {:else}
                  <span class="clip-ic"><Icon name="file" size={12} /></span>
                {/if}
                <span class="clip-txt">{clip.text}</span>
              </button>
              {#if clip.kind === "image"}
                <button
                  class="clip-ed"
                  onclick={() => openImageEditor(clip)}
                  title="Редактировать картинку"
                >
                  <Icon name="pencil" size={12} />
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1.02fr;
    gap: 11px;
    height: 100%;
    min-height: 0;
  }

  .card {
    position: relative;
    border-radius: 16px;
    background: rgba(31, 34, 41, 0.72);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.08),
      0 8px 20px rgba(0, 0, 0, 0.22);
    padding: 12px 14px;
    min-height: 0;
    min-width: 0;
  }

  .left-col {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    min-width: 0;
  }

  /* ── Левый верх: Плеер / Часы ── */
  .stage {
    flex: 1.32;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 8px;
    overflow: hidden;
  }

  .media-top {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .art {
    width: 56px;
    height: 56px;
    border-radius: 10px;
    object-fit: cover;
    flex: none;
    border: 1px solid rgba(255, 255, 255, 0.14);
    box-shadow: 0 6px 16px rgba(0, 0, 0, 0.45);
  }
  .art.empty {
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.06);
    color: var(--muted);
  }
  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .app-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 7px;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.08);
    font-size: 10px;
    color: var(--muted);
    flex: none;
  }
  .app-txt {
    max-width: 68px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .live-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--faint);
  }
  .live-dot.on {
    background: rgb(var(--accent-rgb));
    box-shadow: 0 0 6px rgba(var(--accent-rgb), 0.9);
  }
  .artist {
    font-size: 11.5px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .transport {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 3px;
  }
  .tbtn {
    width: 26px;
    height: 24px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    color: var(--text);
    background: rgba(255, 255, 255, 0.05);
    transition: background 140ms var(--ease), transform 140ms var(--ease);
  }
  .tbtn:hover {
    background: rgba(255, 255, 255, 0.13);
  }
  .tbtn.play {
    width: 28px;
    background: rgba(255, 255, 255, 0.14);
  }
  .tbtn.play.playing {
    color: rgb(var(--accent-rgb));
  }

  /* ── Янтарная звуковая волна (waveform) ── */
  .waveform {
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 3px;
    padding: 0 2px;
  }
  .wbar {
    flex: 1;
    min-width: 2px;
    max-width: 4px;
    border-radius: 99px;
    background: rgba(var(--accent-rgb), 0.34);
    transition: height 130ms ease-out, background 180ms ease;
  }
  .wbar.played {
    background: linear-gradient(180deg, #ffb478 0%, rgb(var(--accent-rgb)) 60%, #e05328 100%);
    box-shadow: 0 0 6px rgba(var(--accent-rgb), 0.32);
  }

  .progress-wrap {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pbar {
    height: 3px;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.1);
    overflow: hidden;
  }
  .pfill {
    height: 100%;
    border-radius: 99px;
    background: linear-gradient(90deg, rgba(var(--accent-rgb), 0.8), #ffb067);
  }
  .times {
    display: flex;
    justify-content: space-between;
    font-size: 10.5px;
    color: var(--faint);
  }

  /* ── Спокойный экран без музыки ── */
  .quiet {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 6px;
  }
  .quiet-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .quiet-date {
    font-size: 11px;
    color: var(--muted);
    text-transform: capitalize;
  }
  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 10.5px;
    color: var(--muted);
    padding: 2px 8px;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }
  .big-clock {
    font-size: 38px;
    font-weight: 650;
    line-height: 1;
    letter-spacing: -0.04em;
    color: var(--text);
  }
  .quiet-sub {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.38;
    color: var(--faint);
  }
  .quiet-chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .qchip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    border-radius: 99px;
    font-size: 10.5px;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }
  .qchip b {
    color: var(--text);
    font-weight: 600;
  }
  .qchip-btn:hover {
    color: var(--text);
    border-color: rgba(255, 255, 255, 0.2);
  }
  .qchip.ok {
    color: var(--ok);
    border-color: rgba(126, 232, 162, 0.35);
    background: rgba(126, 232, 162, 0.1);
  }
  .qchip.ok b {
    color: var(--ok);
  }

  /* ── Левый низ: Разделённый виджет погоды ── */
  .wx-row {
    flex: 0.92;
    display: flex;
    gap: 10px;
    min-height: 0;
  }
  .wx-main-card {
    flex: 0.95;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 3px;
    padding: 10px 13px;
  }
  .wx-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .wx-ico {
    font-size: 24px;
    line-height: 1;
  }
  .wx-big-temp {
    font-size: 26px;
    font-weight: 650;
    line-height: 1;
    color: var(--text);
  }
  .wx-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text);
    margin-top: 2px;
  }
  .wx-cond {
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .wx-detail-card {
    flex: 1.15;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 9px 12px;
    gap: 5px;
  }
  .wx-dhead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .wx-city-btn {
    font-size: 11px;
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .wx-city-btn:hover {
    color: rgb(var(--accent-rgb));
  }
  .wx-ref {
    width: 22px;
    height: 22px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.06);
  }
  .wx-ref:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.12);
  }
  .wx-drows {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 10.5px;
  }
  .wx-drow {
    display: flex;
    justify-content: space-between;
    color: var(--muted);
  }
  .wx-drow b {
    color: var(--text);
    font-weight: 500;
  }

  .wx-edit-card,
  .wx-empty-card {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .wx-city-form {
    display: flex;
    gap: 6px;
    width: 100%;
  }
  .wx-input {
    flex: 1;
    min-width: 0;
    padding: 6px 10px;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.14);
    background: rgba(15, 17, 22, 0.7);
    font-size: 12px;
    outline: none;
  }
  .wx-act,
  .wx-load {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    font-size: 11.5px;
    color: var(--text);
  }
  .wx-act:hover,
  .wx-load:hover {
    background: rgba(255, 255, 255, 0.14);
  }

  /* ── Правая колонка ── */
  .side {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    min-width: 0;
  }

  /* 1) Пилюля громкости с янтарным градиентом */
  .vol-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 13px;
    flex: none;
  }
  .mute-btn {
    width: 28px;
    height: 28px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    color: var(--text);
    background: rgba(255, 255, 255, 0.05);
    flex: none;
  }
  .mute-btn:hover {
    background: rgba(255, 255, 255, 0.12);
  }
  .mute-btn.muted {
    color: var(--danger);
  }
  .slider {
    flex: 1;
    min-width: 0;
    -webkit-appearance: none;
    appearance: none;
    height: 14px;
    border-radius: 999px;
    background: linear-gradient(
      to right,
      rgb(var(--accent-rgb)) 0%,
      #ffb067 var(--vol-pct, 50%),
      rgba(255, 255, 255, 0.08) var(--vol-pct, 50%)
    );
    outline: none;
    cursor: pointer;
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.35);
  }
  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #ffffff;
    box-shadow: 0 1px 6px rgba(0, 0, 0, 0.5);
    border: 1.5px solid rgba(var(--accent-rgb), 0.7);
    transition: transform 140ms var(--ease-spring);
  }
  .slider:hover::-webkit-slider-thumb {
    transform: scale(1.12);
  }
  .val {
    font-size: 11px;
    color: var(--muted);
    min-width: 34px;
    text-align: right;
  }

  /* 2) Сетка 2x2 быстрых кнопок-пилюль */
  .qa-card {
    padding: 9px 10px;
    flex: none;
  }
  .qa-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
  }
  .qbtn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.055);
    border: 1px solid rgba(255, 255, 255, 0.08);
    font-size: 12px;
    font-weight: 500;
    color: var(--text);
    white-space: nowrap;
  }
  .qbtn:hover {
    background: rgba(255, 255, 255, 0.11);
    border-color: rgba(255, 255, 255, 0.2);
  }
  .qbtn.warn {
    color: #ff9c9c;
    border-color: rgba(255, 107, 107, 0.38);
    background: rgba(255, 107, 107, 0.12);
  }

  /* 3) Буфер обмена */
  .recent {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 13px;
  }
  .card-h {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11.5px;
    font-weight: 550;
    color: var(--muted);
  }
  .link {
    font-size: 11px;
    color: var(--faint);
  }
  .link:hover {
    color: var(--text);
  }
  .empty-clips {
    flex: 1;
    display: grid;
    place-items: center;
    text-align: center;
    font-size: 11.5px;
    color: var(--faint);
    padding: 0 8px;
  }
  .clip-mini-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-height: 0;
    overflow: hidden;
  }
  .clip-row {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }
  .clip-mini {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 9px;
    border-radius: 9px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid transparent;
    text-align: left;
    transition: background 140ms var(--ease), border-color 140ms var(--ease);
  }
  .clip-mini.first {
    background: rgba(255, 255, 255, 0.075);
    border-color: rgba(255, 255, 255, 0.08);
  }
  .clip-mini:hover {
    background: rgba(255, 255, 255, 0.11);
    border-color: rgba(255, 255, 255, 0.16);
  }
  .clip-ic {
    color: var(--muted);
    display: grid;
    place-items: center;
    flex: none;
  }
  .mini-thumb {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    object-fit: cover;
    flex: none;
  }
  .clip-txt {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11.5px;
    color: var(--text);
  }
  .clip-ed {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.05);
    flex: none;
  }
  .clip-ed:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.14);
  }
</style>
