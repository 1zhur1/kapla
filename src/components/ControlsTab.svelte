<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, type ConnectivityState, type VpnProfile } from "../lib/ipc";
  import { ACCENT_PRESETS, ui, flash, savePrefs, type AccentPreset } from "../lib/state.svelte";

  type Section = "wifi" | "bt" | "audio" | "power" | "vpn";

  let openSection = $state<Section | null>(null);
  let conn = $state<ConnectivityState | null>(null);
  let connBusy = $state(false);
  let netBusy = $state<string | null>(null);
  let btBusy = $state<string | null>(null);
  let powerBusy = $state(false);

  let vpnBusy = $state(false);
  let discovered = $state<VpnProfile[]>([]);
  let showFound = $state(false);

  const volPct = $derived(Math.round((ui.sys.muted ? 0 : ui.sys.volume) * 100));

  const statusLabel = $derived.by(() => {
    if (ui.vpn.tunnel_up) return ui.vpn.tunnel_name || "Туннель";
    if (ui.vpn.app_running) return "Запущен";
    if (ui.vpn.configured) return "Выкл";
    return "Не выбран";
  });

  const wifiSub = $derived.by(() => {
    if (!conn) return "Проверка…";
    if (!conn.wifi_has_radio) return "Нет адаптера";
    if (!conn.wifi_on) return "Выкл";
    if (conn.wifi_ssid) {
      return conn.wifi_signal != null ? `${conn.wifi_ssid} · ${conn.wifi_signal}%` : conn.wifi_ssid;
    }
    return "Вкл";
  });

  const btConnectedCount = $derived(conn ? conn.bt_devices.filter((d) => d.connected).length : 0);

  const btSub = $derived.by(() => {
    if (!conn) return "Проверка…";
    if (!conn.bt_has_radio) return "Нет адаптера";
    if (!conn.bt_on) return "Выкл";
    const active = conn.bt_devices.find((d) => d.connected);
    if (active) {
      return btConnectedCount > 1 ? `${active.name} (+${btConnectedCount - 1})` : active.name;
    }
    return `${conn.bt_devices.length} устр.`;
  });

  const powerLabel = $derived.by(() => {
    const p = conn?.power_plan;
    if (p === "saver") return "Экономия";
    if (p === "performance") return "Максимум";
    return "Баланс";
  });

  async function refreshConn(scan = false) {
    connBusy = true;
    try {
      conn = await api.connectivityStatus(scan);
    } catch {
      // Игнорируем временную ошибку опроса
    } finally {
      connBusy = false;
    }
  }

  function toggleSection(sec: Section) {
    const next = openSection === sec ? null : sec;
    openSection = next;
    if (next === "wifi") {
      refreshConn(true);
    } else if (next === "bt" || next === "power" || next === "audio") {
      refreshConn(false);
    }
  }

  async function toggleWifiRadio(e: MouseEvent) {
    e.stopPropagation();
    if (!conn || connBusy) return;
    const target = !conn.wifi_on;
    connBusy = true;
    try {
      conn = await api.wifiToggle(target);
      flash("copied", target ? "Wi-Fi включён" : "Wi-Fi выключен");
      if (target) {
        await refreshConn(true);
      }
    } catch (err) {
      flash("copied", typeof err === "string" ? err : "Не удалось переключить Wi-Fi");
    } finally {
      connBusy = false;
    }
  }

  async function connectWifi(ssid: string, connected: boolean) {
    if (netBusy) return;
    netBusy = ssid;
    try {
      conn = connected ? await api.wifiDisconnect() : await api.wifiConnect(ssid);
      flash("copied", connected ? `Отключено: ${ssid}` : `Wi-Fi: ${ssid}`);
    } catch (err) {
      flash("copied", typeof err === "string" ? err : `Ошибка подключения к ${ssid}`, 2600);
    } finally {
      netBusy = null;
    }
  }

  async function toggleBtRadio(e: MouseEvent) {
    e.stopPropagation();
    if (!conn || connBusy) return;
    const target = !conn.bt_on;
    connBusy = true;
    try {
      conn = await api.btToggle(target);
      flash("copied", target ? "Bluetooth включён" : "Bluetooth выключен");
    } catch (err) {
      flash("copied", typeof err === "string" ? err : "Не удалось переключить Bluetooth");
    } finally {
      connBusy = false;
    }
  }

  async function toggleBtDevice(addr: string, name: string, currentlyConnected: boolean) {
    if (btBusy) return;
    btBusy = addr;
    try {
      conn = await api.btConnect(addr, !currentlyConnected);
      flash("copied", !currentlyConnected ? `Подключено: ${name}` : `Отключено: ${name}`);
    } catch (err) {
      flash("copied", typeof err === "string" ? err : `Не удалось переключить ${name}`, 2600);
    } finally {
      btBusy = null;
    }
  }

  async function setPower(mode: "saver" | "balanced" | "performance") {
    if (powerBusy) return;
    powerBusy = true;
    try {
      const plan = await api.powerSet(mode);
      if (conn) conn = { ...conn, power_plan: plan };
      const label = plan === "saver" ? "Экономия" : plan === "performance" ? "Максимум" : "Баланс";
      flash("copied", `Питание: ${label}`);
    } catch (err) {
      flash("copied", typeof err === "string" ? err : "Не удалось сменить режим питания");
    } finally {
      powerBusy = false;
    }
  }

  async function pickVpnApp() {
    ui.pinned = true;
    try {
      const st = await api.vpnPickApp();
      ui.vpn = st;
      showFound = false;
      if (st.selected) flash("vpn", `Выбран: ${st.selected.name}`);
    } catch {
      flash("vpn", "Не удалось открыть выбор файла");
    }
  }

  async function scanVpns() {
    if (showFound) {
      showFound = false;
      return;
    }
    try {
      discovered = await api.vpnDiscover();
      showFound = true;
      if (discovered.length === 0) flash("vpn", "Автопоиск ничего не нашёл — выбери .exe");
    } catch {
      flash("vpn", "Ошибка поиска VPN");
    }
  }

  async function addDiscovered(p: VpnProfile) {
    try {
      ui.vpn = await api.vpnAddProfile(p.name, p.kind, p.path);
      showFound = false;
      flash("vpn", `Добавлен: ${p.name}`);
    } catch {
      flash("vpn", "Не удалось добавить профиль");
    }
  }

  async function toggleVpn(e?: MouseEvent) {
    e?.stopPropagation();
    if (vpnBusy) return;
    if (!ui.vpn.configured) {
      openSection = "vpn";
      await pickVpnApp();
      return;
    }
    vpnBusy = true;
    try {
      const st = await api.vpnToggle();
      ui.vpn = st;
      const name = st.selected?.name ?? "VPN";
      flash("vpn", st.connected ? `${name}: подключён` : `${name}: отключён`);
    } catch (err) {
      flash("vpn", typeof err === "string" ? err : "Не удалось переключить VPN", 2800);
    } finally {
      vpnBusy = false;
    }
  }

  async function selectProfile(id: string) {
    ui.vpn = await api.vpnSelect(id).catch(() => ui.vpn);
  }

  async function removeProfile(id: string) {
    ui.vpn = await api.vpnRemove(id).catch(() => ui.vpn);
  }

  async function openVpnWindow() {
    await api.vpnOpenApp().catch(() => flash("vpn", "Не удалось открыть приложение"));
  }

  function onVolInput(e: Event) {
    const val = Number((e.currentTarget as HTMLInputElement).value) / 100;
    ui.sys = { ...ui.sys, volume: val, muted: val === 0 };
    api.setVolume(val).catch(() => {});
  }

  async function toggleMute() {
    try {
      await api.toggleMute();
      ui.sys = { ...ui.sys, muted: !ui.sys.muted };
    } catch {}
  }

  async function toggleMic() {
    try {
      const muted = await api.toggleMic();
      ui.sys = { ...ui.sys, mic_muted: muted };
      flash("mic", muted ? "Микрофон выкл" : "Микрофон вкл");
    } catch {
      flash("mic", "Нет активного микрофона");
    }
  }

  async function toggleTheme() {
    try {
      const dark = await api.toggleTheme();
      ui.sys = { ...ui.sys, dark_theme: dark };
      flash("copied", dark ? "Тёмная тема Windows" : "Светлая тема Windows");
    } catch {
      flash("copied", "Не удалось сменить тему");
    }
  }

  async function toggleThrough() {
    const next = !ui.through;
    ui.through = next;
    await api.setThrough(next).catch(() => {});
    flash("copied", next ? "Сквозной режим: ВКЛ" : "Сквозной режим: ВЫКЛ");
  }

  async function toggleCapture() {
    const next = !ui.captureVisible;
    ui.captureVisible = next;
    await api.setCaptureVisible(next).catch(() => {});
    flash("copied", next ? "Видна на записи" : "Скрыта от захвата экрана");
  }

  function pickAccent(p: AccentPreset) {
    ui.accentPreset = p;
    if (p !== "auto") {
      ui.accent = ACCENT_PRESETS[p];
    }
    savePrefs();
  }

  function doLock() {
    ui.open = false;
    ui.pinned = false;
    api.lockPc().catch(() => {});
  }

  const swatches: { id: AccentPreset; label: string; color: string }[] = [
    { id: "auto", label: "Авто", color: "linear-gradient(135deg,#ff7a3d,#6cc6ff)" },
    { id: "amber", label: "Янтарь", color: "#ff7a3d" },
    { id: "mint", label: "Мята", color: "#4ad698" },
    { id: "sky", label: "Небо", color: "#5ca8ff" },
    { id: "rose", label: "Роза", color: "#f26694" },
  ];

  onMount(() => {
    refreshConn(false);
  });
</script>

<div class="ctrl-scroll">
  <!-- 3×2 Bento Grid matching System & VPN Controls -->
  <div class="bento-grid">
    <!-- 1. Wi-Fi -->
    <div
      class="btile"
      class:lit={conn?.wifi_on}
      class:open={openSection === "wifi"}
      role="button"
      tabindex="0"
      onclick={() => toggleSection("wifi")}
      onkeydown={(e) => e.key === "Enter" && toggleSection("wifi")}
    >
      <div class="t-icon" class:on={conn?.wifi_on && !!conn?.wifi_ssid}>
        <Icon name="wifi" size={16} />
      </div>
      <div class="t-meta">
        <span class="t-name">Wi-Fi</span>
        <span class="t-sub">{wifiSub}</span>
      </div>
      <button
        class="t-pill mono"
        class:on={conn?.wifi_on}
        disabled={!conn || !conn.wifi_has_radio || connBusy}
        onclick={toggleWifiRadio}
        title="Вкл/Выкл Wi-Fi"
      >
        {conn?.wifi_on ? "ВКЛ" : "ВЫКЛ"}
      </button>
    </div>

    <!-- 2. Bluetooth -->
    <div
      class="btile"
      class:open={openSection === "bt"}
      role="button"
      tabindex="0"
      onclick={() => toggleSection("bt")}
      onkeydown={(e) => e.key === "Enter" && toggleSection("bt")}
    >
      <div class="t-icon" class:on={conn?.bt_on && btConnectedCount > 0}>
        <Icon name="bluetooth" size={16} />
      </div>
      <div class="t-meta">
        <span class="t-name">Bluetooth</span>
        <span class="t-sub">{btSub}</span>
      </div>
      <button
        class="t-pill mono"
        class:on={conn?.bt_on}
        disabled={!conn || !conn.bt_has_radio || connBusy}
        onclick={toggleBtRadio}
        title="Вкл/Выкл Bluetooth"
      >
        {conn?.bt_on ? "ВКЛ" : "ВЫКЛ"}
      </button>
    </div>

    <!-- 3. Устройства и звук / Микрофон -->
    <div
      class="btile"
      class:open={openSection === "audio"}
      role="button"
      tabindex="0"
      onclick={() => toggleSection("audio")}
      onkeydown={(e) => e.key === "Enter" && toggleSection("audio")}
    >
      <div class="t-icon" class:danger={ui.sys.mic_muted}>
        <Icon name={ui.sys.mic_muted ? "micOff" : "volume"} size={16} />
      </div>
      <div class="t-meta">
        <span class="t-name">Аудио и ОС</span>
        <span class="t-sub">{ui.sys.mic_muted ? "Мик выкл" : "Мик активен"} · {ui.sys.dark_theme ? "Тёмная" : "Светлая"}</span>
      </div>
      <button
        class="t-pill mono"
        class:danger={ui.sys.mic_muted}
        class:on={!ui.sys.mic_muted}
        onclick={(e) => {
          e.stopPropagation();
          toggleMic();
        }}
        title="Переключить микрофон (Ctrl+Alt+M)"
      >
        {ui.sys.mic_muted ? "MUTE" : "MIC"}
      </button>
    </div>

    <!-- 4. Питание -->
    <div
      class="btile"
      class:open={openSection === "power"}
      role="button"
      tabindex="0"
      onclick={() => toggleSection("power")}
      onkeydown={(e) => e.key === "Enter" && toggleSection("power")}
    >
      <div class="t-icon" class:on={conn?.power_plan === "performance"}>
        <Icon name="bolt" size={16} />
      </div>
      <div class="t-meta">
        <span class="t-name">Питание</span>
        <span class="t-sub">{powerLabel} · {ui.captureVisible ? "REC" : "Скрыт"}</span>
      </div>
      <span class="t-chev" class:rot={openSection === "power"}>
        <Icon name="chevronDown" size={13} />
      </span>
    </div>

    <!-- 5. Inline Amber Volume Slider Tile -->
    <div class="btile vol-tile">
      <button
        class="t-icon vol-btn"
        class:danger={ui.sys.muted}
        onclick={toggleMute}
        title={ui.sys.muted ? "Включить звук" : "Выключить звук"}
      >
        <Icon name={ui.sys.muted || volPct === 0 ? "mute" : "volume"} size={16} />
      </button>
      <div class="vol-track-wrap">
        <input
          class="vol-range"
          type="range"
          min="0"
          max="100"
          step="1"
          value={volPct}
          style:--pct="{volPct}%"
          oninput={onVolInput}
          aria-label="Громкость системы"
        />
      </div>
      <span class="vol-pct mono">{volPct}%</span>
    </div>

    <!-- 6. Amber-Glowing VPN Bento Tile with Toggle Switch -->
    <div
      class="btile vpn-tile"
      class:connected={ui.vpn.connected}
      class:open={openSection === "vpn"}
      role="button"
      tabindex="0"
      onclick={() => toggleSection("vpn")}
      onkeydown={(e) => e.key === "Enter" && toggleSection("vpn")}
    >
      <div class="t-icon vpn-ico" class:on={ui.vpn.connected}>
        <Icon name="shield" size={16} />
      </div>
      <div class="t-meta">
        <span class="t-name">VPN</span>
        <span class="t-sub">{ui.vpn.selected ? `${ui.vpn.selected.name} · ${statusLabel}` : statusLabel}</span>
      </div>
      <button
        class="ios-switch"
        class:on={ui.vpn.connected}
        class:busy={vpnBusy}
        disabled={vpnBusy}
        onclick={toggleVpn}
        title={ui.vpn.connected ? "Отключить VPN" : "Подключить VPN"}
        aria-label="Переключатель VPN"
      >
        <span class="knob"></span>
      </button>
    </div>
  </div>

  <!-- Expandable Section Drawer -->
  {#if openSection === "wifi"}
    <section class="drawer">
      <div class="dtop">
        <span class="dtitle">Сети Wi-Fi</span>
        <div class="dacts">
          <button class="dmini" onclick={() => refreshConn(true)} disabled={connBusy}>
            <Icon name="refresh" size={12} />
            <span>{connBusy ? "Поиск…" : "Обновить"}</span>
          </button>
          <button class="dmini" onclick={() => api.openWinSettings("ms-settings:network-wifi").catch(() => {})}>
            <Icon name="external" size={12} />
            <span>Windows</span>
          </button>
        </div>
      </div>
      {#if !conn?.wifi_on}
        <div class="dempty">Включите Wi-Fi тумблером на плитке сверху, чтобы увидеть сети.</div>
      {:else if connBusy && conn.wifi_nets.length === 0}
        <div class="dempty">Сканирую доступные сети…</div>
      {:else if conn.wifi_nets.length === 0}
        <div class="dempty">Сети не найдены — нажмите «Обновить».</div>
      {:else}
        <div class="dlist">
          {#each conn.wifi_nets as net (net.ssid)}
            <div class="drow" class:live={net.connected}>
              <div class="drow-l">
                <span class="drow-name">{net.ssid}</span>
                <span class="drow-tags mono">
                  {net.signal}%{net.secured ? " · защищ." : ""}{net.saved ? " · сохр." : ""}
                </span>
              </div>
              <button
                class="dbtn"
                class:live={net.connected}
                disabled={netBusy === net.ssid}
                onclick={() => connectWifi(net.ssid, net.connected)}
              >
                {netBusy === net.ssid ? "…" : net.connected ? "Откл" : net.saved ? "Подключить" : "Войти"}
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {:else if openSection === "bt"}
    <section class="drawer">
      <div class="dtop">
        <span class="dtitle">Сопряжённые Bluetooth-устройства</span>
        <div class="dacts">
          <button class="dmini" onclick={() => refreshConn(false)} disabled={connBusy}>
            <Icon name="refresh" size={12} />
            <span>Обновить</span>
          </button>
          <button class="dmini" onclick={() => api.openWinSettings("ms-settings:bluetooth").catch(() => {})}>
            <Icon name="external" size={12} />
            <span>Windows</span>
          </button>
        </div>
      </div>
      {#if !conn?.bt_on}
        <div class="dempty">Bluetooth выключен. Включите его на плитке сверху.</div>
      {:else if conn.bt_devices.length === 0}
        <div class="dempty">Сопряжённых устройств не найдено. Добавьте их в настройках Windows.</div>
      {:else}
        <div class="dlist">
          {#each conn.bt_devices as dev (dev.addr)}
            <div class="drow" class:live={dev.connected}>
              <div class="drow-l">
                <span class="drow-name">{dev.name}</span>
                <span class="drow-tags mono">
                  {dev.connected ? "подключено" : "пара"}{dev.is_audio ? " · аудио" : ""}
                </span>
              </div>
              {#if dev.is_audio}
                <button
                  class="dbtn"
                  class:live={dev.connected}
                  disabled={btBusy === dev.addr}
                  onclick={() => toggleBtDevice(dev.addr, dev.name, dev.connected)}
                >
                  {btBusy === dev.addr ? "…" : dev.connected ? "Откл" : "Подключить"}
                </button>
              {:else}
                <button
                  class="dbtn"
                  onclick={() => api.openWinSettings("ms-settings:bluetooth").catch(() => {})}
                >
                  В Windows
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {:else if openSection === "audio"}
    <section class="drawer">
      <div class="dtop">
        <span class="dtitle">Система, звук и быстрый доступ</span>
        <div class="dacts">
          <button class="dmini" onclick={() => api.openWinSettings("ms-settings:sound").catch(() => {})}>
            <Icon name="volume" size={12} />
            <span>Звук Windows</span>
          </button>
          <button class="dmini" onclick={() => api.openWinSettings("ms-settings:display").catch(() => {})}>
            <Icon name="external" size={12} />
            <span>Экран</span>
          </button>
        </div>
      </div>
      <div class="sys-pills">
        <button class="spick" class:danger={ui.sys.mic_muted} onclick={toggleMic}>
          <Icon name={ui.sys.mic_muted ? "micOff" : "mic"} size={14} />
          <span>{ui.sys.mic_muted ? "Микрофон: ВЫКЛ" : "Микрофон: ВКЛ"}</span>
        </button>
        <button class="spick" onclick={toggleTheme}>
          <Icon name={ui.sys.dark_theme ? "moon" : "sun"} size={14} />
          <span>{ui.sys.dark_theme ? "Тема: Тёмная" : "Тема: Светлая"}</span>
        </button>
        <button class="spick warn" onclick={doLock}>
          <Icon name="lock" size={14} />
          <span>Блокировка ПК</span>
        </button>
      </div>
    </section>
  {:else if openSection === "power"}
    <section class="drawer">
      <div class="dtop">
        <span class="dtitle">Схема питания и приватность</span>
        <button class="dmini" onclick={() => api.openWinSettings("ms-settings:powersleep").catch(() => {})}>
          <Icon name="external" size={12} />
          <span>Windows</span>
        </button>
      </div>
      <div class="pwr-row">
        <button
          class="pwr-btn"
          class:active={conn?.power_plan === "saver"}
          disabled={powerBusy}
          onclick={() => setPower("saver")}
        >
          Экономия
        </button>
        <button
          class="pwr-btn"
          class:active={!conn || conn.power_plan === "balanced"}
          disabled={powerBusy}
          onclick={() => setPower("balanced")}
        >
          Баланс
        </button>
        <button
          class="pwr-btn"
          class:active={conn?.power_plan === "performance"}
          disabled={powerBusy}
          onclick={() => setPower("performance")}
        >
          Максимум
        </button>
      </div>
      <div class="pwr-toggles">
        <button class="trow-sw" class:on={ui.captureVisible} onclick={toggleCapture}>
          <span class="trl">Показывать на записи экрана (OBS / трансляции)</span>
          <span class="pill-sw mono">{ui.captureVisible ? "ДА" : "СКРЫТА"}</span>
        </button>
        <button class="trow-sw" class:on={ui.through} onclick={toggleThrough}>
          <span class="trl">Сквозной режим (клики сквозь каплю)</span>
          <span class="pill-sw mono">{ui.through ? "ВКЛ" : "ВЫКЛ"}</span>
        </button>
      </div>
    </section>
  {:else if openSection === "vpn"}
    <section class="drawer">
      <div class="dtop">
        <span class="dtitle">VPN-профили и туннель</span>
        <div class="dacts">
          <button class="dmini" onclick={pickVpnApp}>
            <Icon name="plus" size={12} />
            <span>Выбрать .exe</span>
          </button>
          <button class="dmini" class:on={showFound} onclick={scanVpns}>
            <Icon name="search" size={12} />
            <span>Найти на ПК</span>
          </button>
          {#if ui.vpn.configured}
            <button class="dmini" onclick={openVpnWindow} title="Открыть окно VPN">
              <Icon name="external" size={12} />
            </button>
          {/if}
        </div>
      </div>

      {#if showFound && discovered.length > 0}
        <div class="dlist">
          {#each discovered as p}
            <button class="drow" onclick={() => addDiscovered(p)}>
              <div class="drow-l">
                <span class="drow-name">{p.name}</span>
                <span class="drow-tags mono">{p.kind}</span>
              </div>
              <span class="dbtn">Добавить</span>
            </button>
          {/each}
        </div>
      {:else if ui.vpn.profiles.length > 0}
        <div class="dlist">
          {#each ui.vpn.profiles as p (p.id)}
            <div class="drow" class:live={ui.vpn.selected?.id === p.id}>
              <button class="ppick" onclick={() => selectProfile(p.id)}>
                <span class="pdot" class:on={ui.vpn.selected?.id === p.id}></span>
                <span class="drow-name">{p.name}</span>
                <span class="drow-tags mono">{p.kind}</span>
              </button>
              <button class="pdel" onclick={() => removeProfile(p.id)} title="Удалить профиль">
                <Icon name="close" size={12} />
              </button>
            </div>
          {/each}
        </div>
      {:else}
        <div class="dempty">Выберите VPN-клиент (.exe / ярлык) или нажмите «Найти на ПК».</div>
      {/if}
    </section>
  {:else}
    <!-- Compact quick hardware strip shown when no drawer is open -->
    <div class="quick-strip">
      <button class="qtile" class:danger={ui.sys.mic_muted} onclick={toggleMic}>
        <Icon name={ui.sys.mic_muted ? "micOff" : "mic"} size={14} />
        <span>Микрофон</span>
        <strong class="mono">{ui.sys.mic_muted ? "ВЫКЛ" : "ВКЛ"}</strong>
      </button>
      <button class="qtile" onclick={toggleTheme}>
        <Icon name={ui.sys.dark_theme ? "moon" : "sun"} size={14} />
        <span>Тема Windows</span>
        <strong class="mono">{ui.sys.dark_theme ? "Тёмная" : "Светлая"}</strong>
      </button>
      <button class="qtile" class:on={!ui.captureVisible} onclick={toggleCapture}>
        <Icon name="eye" size={14} />
        <span>Запись OBS</span>
        <strong class="mono">{ui.captureVisible ? "Видна" : "Скрыта"}</strong>
      </button>
      <button class="qtile warn" onclick={doLock}>
        <Icon name="lock" size={14} />
        <span>Блокировка</span>
        <strong class="mono">Win+L</strong>
      </button>
    </div>
  {/if}

  <!-- Bottom footer: Accent swatches & Global Hotkeys -->
  <div class="bottom-bar">
    <div class="swatches">
      <span class="ch">Акцент</span>
      {#each swatches as sw}
        <button
          class="sw"
          class:active={ui.accentPreset === sw.id}
          onclick={() => pickAccent(sw.id)}
          title={sw.label}
        >
          <span class="sdot" style:background={sw.color}></span>
          <span>{sw.label}</span>
        </button>
      {/each}
    </div>

    <div class="keys mono">
      <span><b>Ctrl+Alt+I</b> панель</span>
      <span><b>Ctrl+Alt+V</b> буфер</span>
      <span><b>Ctrl+Alt+M</b> мик</span>
    </div>
  </div>
</div>

<style>
  .ctrl-scroll {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* 3×2 Bento Grid matching System & VPN Controls */
  .bento-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    flex: none;
  }

  .btile {
    position: relative;
    height: 58px;
    padding: 0 11px;
    border-radius: 14px;
    background: var(--glass-bg);
    border: 1px solid var(--line);
    box-shadow: var(--shadow-soft), inset 0 1px 0 rgba(255, 255, 255, 0.05);
    display: flex;
    align-items: center;
    gap: 9px;
    cursor: pointer;
    user-select: none;
    transition: background var(--t-fast), border-color var(--t-fast), box-shadow var(--t-fast), transform var(--t-fast);
  }
  .btile:hover {
    background: var(--glass-bg-strong);
    border-color: var(--line-strong);
    transform: translateY(-1px);
  }
  .btile.lit {
    background: rgba(255, 255, 255, 0.13);
    border-color: rgba(255, 255, 255, 0.22);
  }
  .btile.open {
    border-color: rgba(var(--accent-rgb), 0.45);
  }

  .t-icon {
    width: 30px;
    height: 30px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    background: rgba(255, 255, 255, 0.055);
    border: 1px solid var(--line);
    color: var(--muted);
    flex: none;
    transition: color var(--t-fast), background var(--t-fast), border-color var(--t-fast);
  }
  .t-icon.on {
    color: var(--fg);
    background: rgba(255, 255, 255, 0.11);
    border-color: rgba(255, 255, 255, 0.22);
  }
  .t-icon.danger {
    color: #ff9c9c;
    background: rgba(255, 90, 90, 0.14);
    border-color: rgba(255, 90, 90, 0.34);
  }

  .t-meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .t-name {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--fg);
  }
  .t-sub {
    font-size: 10.5px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .t-pill {
    font-size: 9.5px;
    font-weight: 600;
    padding: 3px 7px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid var(--line);
    color: var(--muted);
    flex: none;
  }
  .t-pill.on {
    background: rgba(var(--accent-rgb), 0.18);
    border-color: rgba(var(--accent-rgb), 0.45);
    color: #ffd5bc;
  }
  .t-pill.danger {
    background: rgba(255, 90, 90, 0.16);
    border-color: rgba(255, 90, 90, 0.36);
    color: #ff9c9c;
  }

  .t-chev {
    color: var(--muted);
    transition: transform var(--t-fast);
    display: grid;
    place-items: center;
  }
  .t-chev.rot {
    transform: rotate(180deg);
  }

  /* Tile 5: Inline Amber Volume Bar */
  .vol-tile {
    cursor: default;
    gap: 8px;
  }
  .vol-btn {
    cursor: pointer;
  }
  .vol-btn:hover {
    color: var(--fg);
    border-color: var(--line-strong);
  }
  .vol-track-wrap {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
  }
  .vol-range {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 4px;
    border-radius: 999px;
    background: linear-gradient(
      90deg,
      rgb(var(--accent-rgb)) 0%,
      rgb(var(--accent-rgb)) var(--pct),
      rgba(255, 255, 255, 0.14) var(--pct),
      rgba(255, 255, 255, 0.14) 100%
    );
    outline: none;
    cursor: pointer;
  }
  .vol-range::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #ffffff;
    border: 2px solid rgb(var(--accent-rgb));
    box-shadow: 0 0 8px rgba(var(--accent-rgb), 0.7);
    cursor: pointer;
  }
  .vol-pct {
    font-size: 10.5px;
    color: var(--muted);
    min-width: 28px;
    text-align: right;
  }

  /* Tile 6: Glowing Amber VPN Tile + iOS Toggle Switch */
  .vpn-tile {
    border-color: rgba(var(--accent-rgb), 0.56);
    background: linear-gradient(135deg, rgba(var(--accent-rgb), 0.14), rgba(31, 34, 41, 0.78));
    box-shadow: 0 0 20px rgba(var(--accent-rgb), 0.16), inset 0 1px 0 rgba(255, 190, 145, 0.14);
  }
  .vpn-tile:hover {
    border-color: rgba(var(--accent-rgb), 0.8);
    background: linear-gradient(135deg, rgba(var(--accent-rgb), 0.2), rgba(38, 41, 49, 0.85));
  }
  .vpn-tile.connected {
    border-color: rgba(var(--accent-rgb), 0.9);
    box-shadow: 0 0 26px rgba(var(--accent-rgb), 0.28), inset 0 1px 0 rgba(255, 200, 160, 0.2);
  }
  .vpn-ico {
    color: rgb(var(--accent-rgb));
    background: rgba(var(--accent-rgb), 0.14);
    border-color: rgba(var(--accent-rgb), 0.34);
  }
  .vpn-ico.on {
    color: #fff;
    background: rgb(var(--accent-rgb));
    border-color: transparent;
  }

  .ios-switch {
    position: relative;
    width: 38px;
    height: 22px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.14);
    border: 1px solid rgba(255, 255, 255, 0.16);
    padding: 2px;
    flex: none;
    transition: background var(--t-fast), border-color var(--t-fast), box-shadow var(--t-fast);
  }
  .ios-switch .knob {
    display: block;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.88);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.45);
    transition: transform var(--t-fast) var(--ease), background var(--t-fast);
  }
  .ios-switch.on {
    background: rgb(var(--accent-rgb));
    border-color: rgba(255, 210, 180, 0.4);
    box-shadow: 0 0 12px rgba(var(--accent-rgb), 0.55);
  }
  .ios-switch.on .knob {
    transform: translateX(16px);
    background: #ffffff;
  }
  .ios-switch.busy {
    opacity: 0.6;
  }

  /* Expandable Drawer below the 3x2 grid */
  .drawer {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--r-m);
    background: var(--glass-bg);
    border: 1px solid var(--line);
    box-shadow: var(--shadow-soft);
    overflow-y: auto;
    animation: fadeUp 160ms var(--ease);
  }
  @keyframes fadeUp {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .dtop {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex: none;
  }
  .dtitle {
    font-size: 12px;
    font-weight: 600;
    color: var(--fg);
  }
  .dacts {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .dmini {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    padding: 0 9px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid var(--line);
    color: var(--muted);
    font-size: 11px;
  }
  .dmini:hover,
  .dmini.on {
    color: var(--fg);
    border-color: rgba(var(--accent-rgb), 0.45);
    background: rgba(var(--accent-rgb), 0.14);
  }

  .dempty {
    font-size: 11.5px;
    color: var(--muted);
    padding: 10px 0;
    text-align: center;
  }

  .dlist {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .drow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 9px;
    border-radius: 10px;
    background: rgba(18, 20, 26, 0.56);
    border: 1px solid var(--line);
  }
  .drow.live {
    border-color: rgba(var(--accent-rgb), 0.4);
    background: rgba(var(--accent-rgb), 0.08);
  }
  .drow-l {
    display: flex;
    flex-direction: column;
    min-width: 0;
    text-align: left;
  }
  .drow-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--fg);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .drow-tags {
    font-size: 10px;
    color: var(--muted);
  }
  .dbtn {
    height: 24px;
    padding: 0 10px;
    border-radius: 7px;
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid var(--line);
    color: var(--fg);
    font-size: 11px;
    flex: none;
  }
  .dbtn:hover,
  .dbtn.live {
    background: rgba(var(--accent-rgb), 0.22);
    border-color: rgba(var(--accent-rgb), 0.5);
    color: #fff;
  }

  .ppick {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
  }
  .pdot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.2);
    flex: none;
  }
  .pdot.on {
    background: rgb(var(--accent-rgb));
    box-shadow: 0 0 8px rgba(var(--accent-rgb), 0.7);
  }
  .pdel {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .pdel:hover {
    background: rgba(255, 90, 90, 0.16);
    color: #ff9c9c;
  }

  .sys-pills {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 7px;
  }
  .spick {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 36px;
    border-radius: 10px;
    background: rgba(18, 20, 26, 0.56);
    border: 1px solid var(--line);
    font-size: 11.5px;
    color: var(--fg);
  }
  .spick:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: var(--line-strong);
  }
  .spick.danger {
    color: #ff9c9c;
    border-color: rgba(255, 90, 90, 0.35);
    background: rgba(255, 90, 90, 0.1);
  }
  .spick.warn:hover {
    color: #ffd085;
    border-color: rgba(255, 195, 90, 0.4);
  }

  .pwr-row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .pwr-btn {
    height: 30px;
    border-radius: 9px;
    background: rgba(18, 20, 26, 0.58);
    border: 1px solid var(--line);
    color: var(--muted);
    font-size: 11.5px;
    font-weight: 500;
  }
  .pwr-btn.active {
    background: rgba(var(--accent-rgb), 0.22);
    border-color: rgba(var(--accent-rgb), 0.55);
    color: #fff;
  }

  .pwr-toggles {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .trow-sw {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 10px;
    background: rgba(18, 20, 26, 0.56);
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--fg);
    text-align: left;
  }
  .pill-sw {
    font-size: 9.5px;
    padding: 2px 7px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--muted);
    flex: none;
  }
  .trow-sw.on .pill-sw {
    background: rgba(var(--accent-rgb), 0.2);
    color: #ffd5bc;
  }

  /* Quick strip visible when no drawer expanded */
  .quick-strip {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 7px;
    margin-top: auto;
  }
  .qtile {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 10px;
    border-radius: 11px;
    background: rgba(22, 24, 30, 0.68);
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--muted);
  }
  .qtile span {
    flex: 1;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .qtile strong {
    font-size: 10px;
    color: var(--fg);
  }
  .qtile:hover {
    background: rgba(255, 255, 255, 0.07);
    border-color: var(--line-strong);
    color: var(--fg);
  }
  .qtile.danger {
    border-color: rgba(255, 90, 90, 0.35);
    color: #ff9c9c;
  }
  .qtile.on {
    border-color: rgba(var(--accent-rgb), 0.4);
  }

  /* Bottom bar: Swatches & Hotkeys */
  .bottom-bar {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding-top: 4px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }
  .swatches {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .ch {
    font-size: 11px;
    color: var(--muted);
    margin-right: 2px;
  }
  .sw {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid transparent;
    font-size: 10.5px;
    color: var(--muted);
  }
  .sw:hover {
    color: var(--fg);
    background: rgba(255, 255, 255, 0.08);
  }
  .sw.active {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(var(--accent-rgb), 0.45);
    color: var(--fg);
  }
  .sdot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }

  .keys {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 10px;
    color: var(--muted);
  }
  .keys b {
    padding: 1px 5px;
    border-radius: 5px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid var(--line);
    color: var(--fg);
    font-weight: 500;
  }
</style>
