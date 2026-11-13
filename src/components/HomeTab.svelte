<script lang="ts">
  import Bars from "./Bars.svelte";
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

  function weatherIconName(kind?: string): "sun" | "cloud" | "bolt" {
    if (kind === "sun") return "sun";
    if (kind === "storm") return "bolt";
    return "cloud";
  }

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
  <!-- Левая часть: стеклянный медиа-хаб или часы + погода -->
  <section class="card stage enter-1">
    {#if ui.media.active}
      <div class="glow" class:live={ui.media.playing}></div>
      {#if ui.media.cover}
        <img class="art-bg" src={ui.media.cover} alt="" aria-hidden="true" />
      {/if}
      <div class="media-head">
        {#if ui.media.cover}
          <div class="art-wrap" class:playing={ui.media.playing}>
            <img class="art" src={ui.media.cover} alt="" />
          </div>
        {:else}
          <div class="art empty"><Icon name="music" size={28} /></div>
        {/if}
        <div class="meta">
          <div class="src">
            <span class="app-tag mono"><span class="live-dot" class:on={ui.media.playing}></span>{ui.media.app || "МЕДИА"}</span>
            <Bars playing={ui.media.playing} height={12} />
          </div>
          <div class="title" title={ui.media.title}>{ui.media.title}</div>
          <div class="artist" title={ui.media.artist}>{ui.media.artist || "Неизвестный исполнитель"}</div>
        </div>
      </div>

      <div class="progress-wrap">
        <div class="bar"><div class="fill" style:width="{progress}%"></div></div>
        <div class="times mono">
          <span>{fmtTime(pos)}</span>
          <span>{ui.media.duration > 0 ? `-${fmtTime(Math.max(0, ui.media.duration - pos))}` : "--:--"}</span>
        </div>
      </div>

      <div class="transport">
        <button class="tbtn" onclick={() => api.media("prev")} aria-label="Предыдущий трек">
          <Icon name="prev" size={18} />
        </button>
        <button class="play" class:playing={ui.media.playing} onclick={() => api.media("toggle")} aria-label={ui.media.playing ? "Пауза" : "Играть"}>
          <Icon name={ui.media.playing ? "pause" : "play"} fill size={20} />
        </button>
        <button class="tbtn" onclick={() => api.media("next")} aria-label="Следующий трек">
          <Icon name="next" size={18} />
        </button>
      </div>

      <div class="wx-mini">
        {#if editingCity}
          <form class="wx-city-form" onsubmit={saveCity}>
            <input
              class="wx-input"
              type="text"
              bind:value={cityDraft}
              placeholder="Город (пусто — авто)"
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
        {:else if ui.weather}
          <div class="wx-mini-left">
            <span class="wx-ico" title={ui.weather.desc}>{weatherEmoji(ui.weather.icon)}</span>
            <span class="wx-temp mono">{ui.weather.temp_c > 0 ? `+${ui.weather.temp_c}` : ui.weather.temp_c}°</span>
            <span class="wx-desc">{ui.weather.desc}</span>
          </div>
          <div class="wx-mini-right">
            <button class="wx-city-btn" onclick={startEditCity} title="Сменить город">
              {ui.weather.city || ui.weatherCity || "Город"}
            </button>
            <button class="wx-ref" onclick={() => loadWeather(true)} disabled={weatherBusy} title="Обновить погоду">
              <Icon name="refresh" size={12} />
            </button>
          </div>
        {:else}
          <button class="wx-load" onclick={() => loadWeather(true)} disabled={weatherBusy}>
            <Icon name="cloud" size={13} />
            <span>{weatherBusy ? "Загружаю погоду…" : weatherErr || "Показать погоду"}</span>
          </button>
        {/if}
      </div>
    {:else}
      <div class="quiet">
        <div class="quiet-top">
          <span class="mono quiet-date">{dateLabel}</span>
          <span class="status-pill"><span class="live-dot"></span>Эфир чист</span>
        </div>
        <div class="big-clock mono">{timeLabel}</div>
        <p class="quiet-sub">Включи трек в Spotify, Яндекс Музыке или браузере — обложка и управление появятся здесь.</p>

        <div class="wx-card">
          {#if editingCity}
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
              <button class="wx-act" type="submit">Сохранить</button>
            </form>
          {:else if ui.weather}
            <div class="wx-head">
              <div class="wx-main">
                <span class="wx-big-ico">{weatherEmoji(ui.weather.icon)}</span>
                <div>
                  <div class="wx-temp-row">
                    <span class="wx-big-temp mono">{ui.weather.temp_c > 0 ? `+${ui.weather.temp_c}` : ui.weather.temp_c}°</span>
                    <span class="wx-cond">{ui.weather.desc}</span>
                  </div>
                  <div class="wx-sub mono">
                    Ощущ. {ui.weather.feels_like_c > 0 ? `+${ui.weather.feels_like_c}` : ui.weather.feels_like_c}°{#if ui.weather.min_c != null && ui.weather.max_c != null} · {ui.weather.min_c}..{ui.weather.max_c}°{/if} · ветер {ui.weather.wind_kmph} км/ч · влажн. {ui.weather.humidity}%
                  </div>
                </div>
              </div>
              <div class="wx-actions">
                <button class="wx-city-btn" onclick={startEditCity} title="Настроить город (при VPN может определяться прокси)">
                  {ui.weather.city || ui.weatherCity || "Авто"}
                </button>
                <button class="wx-ref" onclick={() => loadWeather(true)} disabled={weatherBusy} title="Обновить погоду">
                  <Icon name={weatherIconName(ui.weather.icon)} size={12} />
                </button>
              </div>
            </div>
          {:else}
            <div class="wx-empty">
              <button class="wx-load" onclick={() => loadWeather(true)} disabled={weatherBusy}>
                <Icon name="cloud" size={14} />
                <span>{weatherBusy ? "Загружаю погоду…" : weatherErr ? `${weatherErr} — повторить` : "Загрузить погоду"}</span>
              </button>
              <button class="wx-city-btn" onclick={startEditCity}>Город</button>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </section>

  <!-- Правая часть: звук, стеклянный бенто-док и свежий буфер -->
  <div class="side">
    <section class="card vol-card enter-2">
      <div class="card-h">
        <span>Громкость</span>
        <span class="mono val">{ui.sys.muted ? "MUTE" : `${Math.round(ui.sys.volume * 100)}%`}</span>
      </div>
      <div class="vol-row">
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
      </div>
      <div class="chips">
        <span class="chip">Раскладка <b class="mono">{ui.sys.layout || "—"}</b></span>
        {#if ui.sys.battery !== null}
          <span class="chip" class:ok={ui.sys.charging}>
            {#if ui.sys.charging}<Icon name="bolt" size={11} />{/if}
            Заряд <b class="mono">{ui.sys.battery}%</b>
          </span>
        {/if}
        {#if ui.sys.caps}
          <span class="chip warn">CAPS LOCK</span>
        {/if}
        <button
          class="chip chip-btn"
          class:ok={ui.vpn.connected}
          onclick={doVpn}
          title={ui.vpn.selected
            ? `${ui.vpn.selected.name}: клик — ${ui.vpn.connected ? "отключить" : "подключить"}`
            : "Настроить VPN в пульте"}
        >
          <Icon name="shield" size={12} />
          <span>VPN</span>
          <b class="mono">{ui.vpn.configured ? (ui.vpn.connected ? "ВКЛ" : "ВЫКЛ") : "+"}</b>
        </button>
      </div>
    </section>

    <div class="qa-row enter-3">
      <button class="qbtn qa-amber" onclick={doSnip} title="Ножницы Windows (вырезать область)">
        <span class="qbadge"><Icon name="scissors" size={15} /></span>
        <span>Ножницы</span>
      </button>
      <button class="qbtn qa-cyan" onclick={doShot} title="Скриншот всего экрана -> Картинки/Kapla + буфер">
        <span class="qbadge"><Icon name="camera" size={15} /></span>
        <span>Скрин</span>
      </button>
      <button class="qbtn qa-violet" onclick={doColor} title="Пипетка: кликни любой пиксель экрана -> HEX">
        <span class="qbadge"><Icon name="pipette" size={15} /></span>
        <span>HEX</span>
      </button>
      <button class="qbtn qa-mint" class:warn={ui.sys.mic_muted} onclick={doMic} title="Микрофон (Ctrl+Alt+M)">
        <span class="qbadge"><Icon name={ui.sys.mic_muted ? "micOff" : "mic"} size={15} /></span>
        <span>{ui.sys.mic_muted ? "Выкл" : "Мик"}</span>
      </button>
    </div>

    <section class="card recent enter-4">
      <div class="card-h">
        <span>Последнее в буфере</span>
        <button class="link" onclick={() => onTab("clipboard")}>Все ({ui.clips.length}) →</button>
      </div>
      {#if ui.clips.length === 0}
        <div class="empty-clips">Скопируй текст или картинку — клик здесь сразу вставит в активное окно.</div>
      {:else}
        <div class="clip-mini-list">
          {#each ui.clips.slice(0, 2) as clip (clip.id)}
            <div class="clip-row">
              <button
                class="clip-mini"
                onclick={() => pasteLatest(clip.id)}
                title={clip.text}
              >
                {#if clip.kind === "image" && clip.preview}
                  <img class="mini-thumb" src={clip.preview} alt="" />
                {:else}
                  <span class="clip-ic"><Icon name="copy" size={12} /></span>
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
    grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr);
    gap: 12px;
    height: 100%;
    min-width: 0;
  }
  .enter-1 {
    animation: kaplaFadeUp 260ms var(--ease) backwards;
  }
  .enter-2 {
    animation: kaplaFadeUp 280ms var(--ease) 40ms backwards;
  }
  .enter-3 {
    animation: kaplaFadeUp 300ms var(--ease) 80ms backwards;
  }
  .enter-4 {
    animation: kaplaFadeUp 320ms var(--ease) 120ms backwards;
  }
  .card {
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    border-radius: var(--r-m);
    padding: 15px;
    position: relative;
    overflow: hidden;
    min-width: 0;
    box-shadow: var(--surface-sheen);
    transition: border-color 200ms var(--ease), background 200ms var(--ease), transform 220ms var(--ease);
  }
  .card::before {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(115deg, transparent 30%, rgba(255, 255, 255, 0.14) 50%, transparent 70%);
    transform: translateX(-130%);
    transition: transform 650ms var(--ease);
    pointer-events: none;
  }
  .card:hover::before {
    transform: translateX(130%);
  }
  .card:hover {
    border-color: var(--line-2);
  }
  .stage {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
  }
  .art-bg {
    position: absolute;
    inset: -25%;
    width: 150%;
    height: 150%;
    object-fit: cover;
    filter: blur(36px) saturate(165%);
    opacity: 0.28;
    pointer-events: none;
    transform: scale(1.08);
    transition: opacity 400ms var(--ease);
  }
  .glow {
    position: absolute;
    inset: -40px -20px auto -20px;
    height: 185px;
    background: radial-gradient(circle at 25% 20%, rgba(var(--accent-rgb), 0.28), rgba(92, 212, 255, 0.1) 55%, transparent 75%);
    pointer-events: none;
    transition: opacity 300ms var(--ease);
  }
  .glow.live {
    animation: kaplaGlowBreathe 3.2s ease-in-out infinite;
  }
  @keyframes kaplaGlowBreathe {
    0%,
    100% {
      opacity: 0.85;
      transform: scale(1);
    }
    50% {
      opacity: 1;
      transform: scale(1.06);
    }
  }
  .media-head {
    display: flex;
    gap: 14px;
    align-items: center;
    position: relative;
  }
  .art-wrap {
    position: relative;
    flex: none;
    border-radius: 16px;
    padding: 2px;
    background: linear-gradient(135deg, rgba(255, 255, 255, 0.45), rgba(var(--accent-rgb), 0.35), rgba(130, 225, 255, 0.35));
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.42), 0 0 22px rgba(var(--accent-rgb), 0.22);
    transition: transform 280ms var(--ease-spring), box-shadow 280ms var(--ease);
  }
  .art-wrap.playing {
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45), 0 0 28px rgba(var(--accent-rgb), 0.42);
  }
  .art {
    width: 76px;
    height: 76px;
    border-radius: 14px;
    object-fit: cover;
    display: block;
    flex: none;
  }
  .art.empty {
    width: 78px;
    height: 78px;
    border-radius: 15px;
    display: grid;
    place-items: center;
    background: var(--glass-bg-strong);
    border: 1px solid var(--line);
    color: var(--muted);
  }
  .meta {
    min-width: 0;
    flex: 1;
  }
  .src {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 6px;
  }
  .app-tag {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: rgb(var(--accent-rgb));
  }
  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.45);
    display: inline-block;
  }
  .live-dot.on {
    background: var(--ok);
    box-shadow: 0 0 8px rgba(126, 232, 162, 0.85);
    animation: kaplaPulse 2s ease-in-out infinite;
  }
  .title {
    font-size: 16.5px;
    font-weight: 600;
    line-height: 1.25;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .artist {
    font-size: 12.5px;
    color: var(--muted);
    margin-top: 3px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .progress-wrap {
    margin-top: 12px;
    position: relative;
  }
  .bar {
    height: 6px;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.12);
    border: 1px solid rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: 99px;
    background: linear-gradient(90deg, rgba(130, 225, 255, 0.85), rgb(var(--accent-rgb)));
    box-shadow: 0 0 12px rgba(var(--accent-rgb), 0.55);
    transition: width 350ms linear;
  }
  .times {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--muted);
    margin-top: 6px;
  }
  .transport {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 16px;
    margin-top: 8px;
    position: relative;
  }
  .tbtn {
    width: 36px;
    height: 36px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid transparent;
    transition: background 160ms var(--ease), color 160ms var(--ease), transform 180ms var(--ease-spring), border-color 160ms var(--ease);
  }
  .tbtn:hover {
    background: var(--glass-bg-strong);
    border-color: var(--line);
    color: var(--text);
    transform: scale(1.08);
  }
  .play {
    width: 46px;
    height: 46px;
    border-radius: 16px;
    display: grid;
    place-items: center;
    background: linear-gradient(145deg, rgba(255, 255, 255, 0.35) 0%, transparent 45%), rgb(var(--accent-rgb));
    color: #0b0e14;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.5), 0 8px 22px rgba(var(--accent-rgb), 0.42);
    transition: transform 200ms var(--ease-spring), filter 160ms var(--ease), box-shadow 200ms var(--ease);
  }
  .play:hover {
    transform: scale(1.08);
    filter: brightness(1.1);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.6), 0 10px 26px rgba(var(--accent-rgb), 0.58);
  }
  .play:active {
    transform: scale(0.95);
  }

  .quiet {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    height: 100%;
    position: relative;
  }
  .quiet-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .quiet-date {
    font-size: 11.5px;
    color: var(--muted);
    text-transform: capitalize;
  }
  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    padding: 3px 9px;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid var(--line);
    color: var(--muted);
  }
  .big-clock {
    font-size: 46px;
    font-weight: 600;
    letter-spacing: -0.04em;
    line-height: 1;
    background: linear-gradient(135deg, #ffffff 25%, rgba(160, 228, 255, 0.92) 70%, rgba(var(--accent-rgb), 0.9) 105%);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
    filter: drop-shadow(0 6px 18px rgba(var(--accent-rgb), 0.2));
  }
  .quiet-sub {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--muted);
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    min-width: 0;
  }
  .vol-card,
  .recent {
    padding: 11px 13px;
    min-width: 0;
  }
  .qa-row {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    flex: none;
  }
  .qbtn {
    height: 52px;
    padding: 5px 4px;
    border-radius: 14px;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: var(--surface-sheen);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 500;
    color: var(--muted);
    transition: transform 220ms var(--ease-spring), border-color 160ms var(--ease), color 160ms var(--ease), background 160ms var(--ease), box-shadow 180ms var(--ease);
  }
  .qbadge {
    width: 26px;
    height: 26px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.14);
    transition: transform 240ms var(--ease-spring), background 160ms var(--ease), box-shadow 160ms var(--ease);
  }
  .qa-amber .qbadge {
    color: #ffb36b;
    background: rgba(255, 159, 67, 0.16);
    border-color: rgba(255, 159, 67, 0.34);
  }
  .qa-cyan .qbadge {
    color: #7ee2ff;
    background: rgba(86, 204, 242, 0.16);
    border-color: rgba(86, 204, 242, 0.34);
  }
  .qa-violet .qbadge {
    color: #c59cff;
    background: rgba(176, 132, 255, 0.16);
    border-color: rgba(176, 132, 255, 0.34);
  }
  .qa-mint .qbadge {
    color: #7ef0ae;
    background: rgba(126, 232, 162, 0.16);
    border-color: rgba(126, 232, 162, 0.34);
  }
  .qbtn:hover {
    transform: translateY(-2px);
    color: var(--text);
    background: var(--glass-bg-strong);
    border-color: var(--line-2);
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.28), inset 0 1px 0 rgba(255, 255, 255, 0.28);
  }
  .qbtn:hover .qbadge {
    transform: scale(1.14) rotate(-6deg);
  }
  .qbtn.warn {
    color: var(--danger);
    border-color: rgba(255, 107, 107, 0.42);
    background: rgba(255, 107, 107, 0.12);
  }
  .qbtn.warn .qbadge {
    color: var(--danger);
    background: rgba(255, 107, 107, 0.18);
    border-color: rgba(255, 107, 107, 0.4);
  }
  .card-h {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 10px;
  }
  .val {
    color: var(--text);
    font-weight: 600;
  }
  .vol-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .mute-btn {
    width: 32px;
    height: 32px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.09);
    border: 1px solid var(--line);
    color: var(--text);
    flex: none;
  }
  .mute-btn:hover {
    background: var(--glass-bg-strong);
    border-color: var(--line-2);
  }
  .mute-btn.muted {
    color: var(--danger);
    border-color: rgba(255, 107, 107, 0.4);
  }
  .slider {
    flex: 1;
    appearance: none;
    -webkit-appearance: none;
    cursor: pointer;
    height: 7px;
    border-radius: 99px;
    background: linear-gradient(
      90deg,
      rgba(130, 225, 255, 0.9) 0%,
      rgb(var(--accent-rgb)) var(--vol-pct, 50%),
      rgba(255, 255, 255, 0.14) var(--vol-pct, 50%),
      rgba(255, 255, 255, 0.14) 100%
    );
    border: 1px solid rgba(255, 255, 255, 0.14);
    outline: none;
  }
  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: #fff;
    border: 2px solid rgb(var(--accent-rgb));
    box-shadow: 0 0 10px rgba(var(--accent-rgb), 0.65);
    transition: transform 150ms var(--ease-spring);
  }
  .slider:hover::-webkit-slider-thumb {
    transform: scale(1.18);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 12px;
  }
  .chip {
    font-size: 11px;
    padding: 4px 8px;
    border-radius: 8px;
    background: var(--ink-3);
    color: var(--muted);
    display: inline-flex;
    gap: 5px;
    align-items: center;
  }
  .chip b {
    color: var(--text);
    font-weight: 600;
  }
  .chip.ok b {
    color: var(--ok);
  }
  .chip.warn {
    background: rgba(255, 200, 87, 0.14);
    color: var(--warn);
  }
  .chip-btn {
    border: 1px solid var(--line);
    transition: border-color 140ms var(--ease), background 140ms var(--ease);
  }
  .chip-btn:hover {
    border-color: rgba(var(--accent-rgb), 0.45);
    background: var(--ink-4);
  }
  .chip-btn.ok {
    border-color: rgba(74, 214, 152, 0.35);
    background: rgba(74, 214, 152, 0.1);
  }

  .recent {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .link {
    font-size: 11.5px;
    color: rgb(var(--accent-rgb));
  }
  .link:hover {
    text-decoration: underline;
  }
  .empty-clips {
    font-size: 12px;
    color: var(--faint);
    line-height: 1.45;
    margin: auto 0;
  }
  .clip-mini-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .clip-row {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    width: 100%;
  }
  .clip-mini {
    flex: 1;
    min-width: 0;
    max-width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: var(--r-s);
    background: rgba(255, 255, 255, 0.055);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.14);
    text-align: left;
    overflow: hidden;
    transition: border-color 140ms var(--ease), background 140ms var(--ease), transform 140ms var(--ease);
  }
  .clip-mini:hover {
    border-color: rgba(var(--accent-rgb), 0.42);
    background: var(--glass-bg-strong);
    transform: translateY(-1px);
  }
  .clip-ic {
    color: var(--muted);
    display: grid;
    place-items: center;
    flex: none;
  }
  .mini-thumb {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    object-fit: cover;
    flex: none;
    border: 1px solid var(--line);
  }
  .clip-txt {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }
  .clip-ed {
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: var(--ink-3);
    border: 1px solid var(--line);
    color: var(--muted);
    display: grid;
    place-items: center;
    flex: none;
    transition: color 140ms var(--ease), border-color 140ms var(--ease), background 140ms var(--ease);
  }
  .clip-ed:hover {
    color: rgb(var(--accent-rgb));
    border-color: rgba(var(--accent-rgb), 0.5);
    background: rgba(var(--accent-rgb), 0.12);
  }

  .wx-mini,
  .wx-card {
    border-radius: var(--r-s);
    background: rgba(255, 255, 255, 0.055);
    backdrop-filter: blur(12px);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.14);
    padding: 8px 10px;
  }
  .wx-mini {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: auto;
  }
  .wx-mini-left {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .wx-ico {
    font-size: 14px;
    line-height: 1;
  }
  .wx-temp {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .wx-desc {
    font-size: 11.5px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wx-mini-right,
  .wx-actions {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: none;
  }
  .wx-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .wx-main {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .wx-big-ico {
    font-size: 26px;
    line-height: 1;
    display: inline-block;
    animation: kaplaFloat 3.4s ease-in-out infinite;
    filter: drop-shadow(0 4px 10px rgba(0, 0, 0, 0.35));
  }
  .wx-temp-row {
    display: flex;
    align-items: baseline;
    gap: 7px;
  }
  .wx-big-temp {
    font-size: 21px;
    font-weight: 600;
    line-height: 1;
    background: linear-gradient(135deg, #fff 30%, rgba(130, 225, 255, 0.95) 100%);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
  }
  .wx-cond {
    font-size: 12px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wx-sub {
    margin-top: 3px;
    font-size: 10.5px;
    color: var(--muted);
  }
  .wx-city-btn,
  .wx-ref,
  .wx-load,
  .wx-act {
    height: 24px;
    padding: 0 8px;
    border-radius: 7px;
    background: var(--ink-4);
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--muted);
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .wx-ref {
    width: 24px;
    padding: 0;
    justify-content: center;
  }
  .wx-city-btn:hover,
  .wx-ref:hover,
  .wx-load:hover,
  .wx-act:hover {
    color: var(--text);
    border-color: rgba(var(--accent-rgb), 0.45);
  }
  .wx-city-form {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
  }
  .wx-input {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 8px;
    border-radius: 7px;
    background: var(--ink-2);
    border: 1px solid var(--line-2);
    font-size: 11.5px;
    color: var(--text);
    outline: none;
  }
  .wx-input:focus {
    border-color: rgba(var(--accent-rgb), 0.65);
  }
  .wx-empty {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
</style>
