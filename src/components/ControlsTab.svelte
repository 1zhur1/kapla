<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { api, type ConnectivityState, type VpnProfile } from "../lib/ipc";
  import { ACCENT_PRESETS, ui, flash, savePrefs, type AccentPreset } from "../lib/state.svelte";

  type Section = "wifi" | "bt" | "vpn" | "power";

  let openSection = $state<Section | null>(null);
  let conn = $state<ConnectivityState | null>(null);
  let connBusy = $state(false);
  let netBusy = $state<string | null>(null);
  let btBusy = $state<string | null>(null);
  let powerBusy = $state(false);

  let vpnBusy = $state(false);
  let discovered = $state<VpnProfile[]>([]);
  let showFound = $state(false);

  const statusLabel = $derived.by(() => {
    if (ui.vpn.tunnel_up) return ui.vpn.tunnel_name || "Туннель";
    if (ui.vpn.app_running) return "Запущен";
    if (ui.vpn.configured) return "Выкл";
    return "Не выбран";
  });

  const wifiSub = $derived.by(() => {
    if (!conn) return "Проверка…";
    if (!conn.wifi_has_radio) return "Адаптер не найден";
    if (!conn.wifi_on) return "Выключен";
    if (conn.wifi_ssid) {
      return conn.wifi_signal != null ? `${conn.wifi_ssid} · ${conn.wifi_signal}%` : conn.wifi_ssid;
    }
    return "Не подключён";
  });

  const btConnectedCount = $derived(conn ? conn.bt_devices.filter((d) => d.connected).length : 0);

  const btSub = $derived.by(() => {
    if (!conn) return "Проверка…";
    if (!conn.bt_has_radio) return "Адаптер не найден";
    if (!conn.bt_on) return "Выключен";
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
    } else if (next === "bt" || next === "power") {
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
  <!-- Раскрываемые контейнеры -->
  <div class="containers">
    <!-- Wi-Fi контейнер -->
    <section class="ccard" class:open={openSection === "wifi"} class:active={conn?.wifi_on}>
      <div class="chead" role="button" tabindex="0" onclick={() => toggleSection("wifi")} onkeydown={(e) => e.key === "Enter" && toggleSection("wifi")}>
        <div class="cicon" class:on={conn?.wifi_on && !!conn?.wifi_ssid}>
          <Icon name="wifi" size={16} />
        </div>
        <div class="cmeta">
          <span class="cname">Wi-Fi</span>
          <span class="csub">{wifiSub}</span>
        </div>
        <button
          class="cpill mono"
          class:on={conn?.wifi_on}
          disabled={!conn || !conn.wifi_has_radio || connBusy}
          onclick={toggleWifiRadio}
        >
          {conn?.wifi_on ? "ВКЛ" : "ВЫКЛ"}
        </button>
        <span class="chev" class:rot={openSection === "wifi"}><Icon name="chevronDown" size={14} /></span>
      </div>

      {#if openSection === "wifi"}
        <div class="cbody">
          <div class="cbody-top">
            <span class="cbody-title">Доступные сети</span>
            <div class="cbody-acts">
              <button class="cmini" onclick={() => refreshConn(true)} disabled={connBusy}>
                <Icon name="refresh" size={12} />
                <span>{connBusy ? "Поиск…" : "Обновить"}</span>
              </button>
              <button class="cmini" onclick={() => api.openWinSettings("ms-settings:network-wifi").catch(() => {})}>
                <Icon name="external" size={12} />
                <span>Windows</span>
              </button>
            </div>
          </div>

          {#if !conn?.wifi_on}
            <div class="cempty">Включи Wi-Fi тумблером сверху, чтобы увидеть сети.</div>
          {:else if connBusy && conn.wifi_nets.length === 0}
            <div class="cempty">Сканирую доступные сети…</div>
          {:else if conn.wifi_nets.length === 0}
            <div class="cempty">Сети не найдены — попробуй обновить.</div>
          {:else}
            <div class="clist">
              {#each conn.wifi_nets as net (net.ssid)}
                <div class="crow" class:live={net.connected}>
                  <div class="crow-l">
                    <span class="crow-name">{net.ssid}</span>
                    <span class="crow-tags mono">
                      {net.signal}%{net.secured ? " · защищ." : ""}{net.saved ? " · сохр." : ""}
                    </span>
                  </div>
                  <button
                    class="cbtn"
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
        </div>
      {/if}
    </section>

    <!-- Bluetooth контейнер -->
    <section class="ccard" class:open={openSection === "bt"} class:active={conn?.bt_on}>
      <div class="chead" role="button" tabindex="0" onclick={() => toggleSection("bt")} onkeydown={(e) => e.key === "Enter" && toggleSection("bt")}>
        <div class="cicon" class:on={conn?.bt_on && btConnectedCount > 0}>
          <Icon name="bluetooth" size={16} />
        </div>
        <div class="cmeta">
          <span class="cname">Bluetooth</span>
          <span class="csub">{btSub}</span>
        </div>
        <button
          class="cpill mono"
          class:on={conn?.bt_on}
          disabled={!conn || !conn.bt_has_radio || connBusy}
          onclick={toggleBtRadio}
        >
          {conn?.bt_on ? "ВКЛ" : "ВЫКЛ"}
        </button>
        <span class="chev" class:rot={openSection === "bt"}><Icon name="chevronDown" size={14} /></span>
      </div>

      {#if openSection === "bt"}
        <div class="cbody">
          <div class="cbody-top">
            <span class="cbody-title">Сопряжённые устройства</span>
            <div class="cbody-acts">
              <button class="cmini" onclick={() => refreshConn(false)} disabled={connBusy}>
                <Icon name="refresh" size={12} />
                <span>Обновить</span>
              </button>
              <button class="cmini" onclick={() => api.openWinSettings("ms-settings:bluetooth").catch(() => {})}>
                <Icon name="external" size={12} />
                <span>Windows</span>
              </button>
            </div>
          </div>

          {#if !conn?.bt_on}
            <div class="cempty">Bluetooth выключен. Нажми «ВКЛ» для подключения устройств.</div>
          {:else if conn.bt_devices.length === 0}
            <div class="cempty">Сопряжённых устройств не найдено. Добавь их через параметры Windows.</div>
          {:else}
            <div class="clist">
              {#each conn.bt_devices as dev (dev.addr)}
                <div class="crow" class:live={dev.connected}>
                  <div class="crow-l">
                    <span class="crow-name">{dev.name}</span>
                    <span class="crow-tags mono">
                      {dev.connected ? "подключено" : "рядом / пара"}{dev.is_audio ? " · аудио" : ""}
                    </span>
                  </div>
                  {#if dev.is_audio}
                    <button
                      class="cbtn"
                      class:live={dev.connected}
                      disabled={btBusy === dev.addr}
                      onclick={() => toggleBtDevice(dev.addr, dev.name, dev.connected)}
                    >
                      {btBusy === dev.addr ? "…" : dev.connected ? "Откл" : "Подключить"}
                    </button>
                  {:else}
                    <button
                      class="cbtn"
                      onclick={() => api.openWinSettings("ms-settings:bluetooth").catch(() => {})}
                    >
                      В Windows
                    </button>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </section>

    <!-- VPN контейнер -->
    <section class="ccard" class:open={openSection === "vpn"} class:active={ui.vpn.connected}>
      <div class="chead" role="button" tabindex="0" onclick={() => toggleSection("vpn")} onkeydown={(e) => e.key === "Enter" && toggleSection("vpn")}>
        <div class="cicon" class:on={ui.vpn.connected}>
          <Icon name="shield" size={16} />
        </div>
        <div class="cmeta">
          <span class="cname">VPN</span>
          <span class="csub">{ui.vpn.selected ? `${ui.vpn.selected.name} · ${statusLabel}` : statusLabel}</span>
        </div>
        <button class="cpill mono" class:on={ui.vpn.connected} disabled={vpnBusy} onclick={toggleVpn}>
          {vpnBusy ? "…" : ui.vpn.connected ? "ВКЛ" : "ВЫКЛ"}
        </button>
        <span class="chev" class:rot={openSection === "vpn"}><Icon name="chevronDown" size={14} /></span>
      </div>

      {#if openSection === "vpn"}
        <div class="cbody">
          <div class="vpn-actions">
            <button class="vpn-main" class:on={ui.vpn.connected} disabled={vpnBusy} onclick={() => toggleVpn()}>
              <Icon name="shield" size={15} />
              <span>
                {#if vpnBusy}
                  Переключаю…
                {:else if !ui.vpn.configured}
                  Выбрать VPN-приложение…
                {:else if ui.vpn.connected}
                  Отключить {ui.vpn.selected?.name ?? "VPN"}
                {:else}
                  Подключить {ui.vpn.selected?.name ?? "VPN"}
                {/if}
              </span>
            </button>
            {#if ui.vpn.configured}
              <button class="ibtn" onclick={openVpnWindow} title="Открыть окно VPN-приложения">
                <Icon name="external" size={14} />
              </button>
            {/if}
          </div>

          <div class="vpn-tools">
            <button class="sbtn" onclick={pickVpnApp}>
              <Icon name="plus" size={13} />
              <span>Выбрать .exe</span>
            </button>
            <button class="sbtn" class:active={showFound} onclick={scanVpns}>
              <Icon name="search" size={13} />
              <span>Найти на ПК</span>
            </button>
          </div>

          {#if showFound && discovered.length > 0}
            <div class="clist">
              {#each discovered as p}
                <button class="crow" onclick={() => addDiscovered(p)}>
                  <div class="crow-l">
                    <span class="crow-name">{p.name}</span>
                    <span class="crow-tags mono">{p.kind}</span>
                  </div>
                  <span class="cbtn">Добавить</span>
                </button>
              {/each}
            </div>
          {:else if ui.vpn.profiles.length > 0}
            <div class="clist">
              {#each ui.vpn.profiles as p (p.id)}
                <div class="crow" class:live={ui.vpn.selected?.id === p.id}>
                  <button class="ppick" onclick={() => selectProfile(p.id)}>
                    <span class="pdot" class:on={ui.vpn.selected?.id === p.id}></span>
                    <span class="crow-name">{p.name}</span>
                    <span class="crow-tags mono">{p.kind}</span>
                  </button>
                  <button class="pdel" onclick={() => removeProfile(p.id)} title="Удалить профиль">
                    <Icon name="close" size={12} />
                  </button>
                </div>
              {/each}
            </div>
          {:else}
            <div class="cempty">Выбери любой VPN-клиент (.exe / ярлык) или нажми «Найти на ПК».</div>
          {/if}
        </div>
      {/if}
    </section>

    <!-- Питание и экран контейнер -->
    <section class="ccard" class:open={openSection === "power"}>
      <div class="chead" role="button" tabindex="0" onclick={() => toggleSection("power")} onkeydown={(e) => e.key === "Enter" && toggleSection("power")}>
        <div class="cicon" class:on={conn?.power_plan === "performance"}>
          <Icon name="bolt" size={16} />
        </div>
        <div class="cmeta">
          <span class="cname">Питание и экран</span>
          <span class="csub">{powerLabel} · {ui.captureVisible ? "видна" : "скрыта"}</span>
        </div>
        <span class="cpill mono" class:on={!ui.captureVisible}>
          {ui.captureVisible ? "REC" : "СКР"}
        </span>
        <span class="chev" class:rot={openSection === "power"}><Icon name="chevronDown" size={14} /></span>
      </div>

      {#if openSection === "power"}
        <div class="cbody">
          <div class="cbody-top">
            <span class="cbody-title">Схема электропитания</span>
            <button class="cmini" onclick={() => api.openWinSettings("ms-settings:powersleep").catch(() => {})}>
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
            <button class="row" class:on={ui.captureVisible} onclick={toggleCapture}>
              <div class="rleft">
                <span class="rl">Показывать на записи экрана</span>
                <span class="rs">Иначе скрыта в OBS и трансляциях</span>
              </div>
              <span class="pill-sw mono">{ui.captureVisible ? "ДА" : "СКРЫТА"}</span>
            </button>
            <button class="row" class:on={ui.through} onclick={toggleThrough}>
              <div class="rleft">
                <span class="rl">Сквозной режим</span>
                <span class="rs">Клики проходят сквозь каплю</span>
              </div>
              <span class="pill-sw mono">{ui.through ? "ВКЛ" : "ВЫКЛ"}</span>
            </button>
          </div>
        </div>
      {/if}
    </section>
  </div>

  <!-- Быстрые системные плитки -->
  <div class="mini-grid">
    <button class="tile" class:danger={ui.sys.mic_muted} onclick={toggleMic}>
      <Icon name={ui.sys.mic_muted ? "micOff" : "mic"} size={15} />
      <div class="tmeta">
        <span class="tl">Микрофон</span>
        <span class="ts mono">{ui.sys.mic_muted ? "ВЫКЛ" : "ВКЛ"}</span>
      </div>
    </button>

    <button class="tile" onclick={toggleTheme}>
      <Icon name={ui.sys.dark_theme ? "moon" : "sun"} size={15} />
      <div class="tmeta">
        <span class="tl">Тема Windows</span>
        <span class="ts mono">{ui.sys.dark_theme ? "Тёмная" : "Светлая"}</span>
      </div>
    </button>

    <button class="tile warn" onclick={doLock}>
      <Icon name="lock" size={15} />
      <div class="tmeta">
        <span class="tl">Блокировка</span>
        <span class="ts">Win+L</span>
      </div>
    </button>
  </div>

  <!-- Нижняя строка: акцент и хоткеи -->
  <div class="bottom-bar">
    <div class="swatches">
      <span class="ch">Акцент:</span>
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
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-right: 2px;
  }

  .containers {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 9px;
  }

  .ccard {
    position: relative;
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: var(--surface-sheen);
    border-radius: var(--r-m);
    overflow: hidden;
    transition: border-color 180ms var(--ease), background 180ms var(--ease), transform 200ms var(--ease);
  }
  .ccard.active {
    border-color: rgba(var(--accent-rgb), 0.45);
    background: rgba(var(--accent-rgb), 0.08);
  }
  .ccard.open {
    grid-column: 1 / -1;
    border-color: rgba(var(--accent-rgb), 0.62);
    box-shadow: var(--surface-sheen), 0 0 24px rgba(var(--accent-rgb), 0.14);
  }

  .chead {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 10px 12px;
    cursor: pointer;
    user-select: none;
  }
  .chead:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .chead:hover .cicon {
    transform: scale(1.08) rotate(-4deg);
  }
  .cicon {
    width: 34px;
    height: 34px;
    border-radius: 11px;
    background: rgba(255, 255, 255, 0.09);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.2);
    display: grid;
    place-items: center;
    color: var(--muted);
    flex: none;
    transition: transform 220ms var(--ease-spring), background 180ms var(--ease), color 180ms var(--ease);
  }
  .cicon.on {
    background: rgba(var(--accent-rgb), 0.22);
    border-color: rgba(var(--accent-rgb), 0.48);
    color: rgb(var(--accent-rgb));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.26), 0 0 14px rgba(var(--accent-rgb), 0.25);
  }
  .cmeta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .cname {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text);
  }
  .csub {
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .cpill {
    height: 24px;
    padding: 0 9px;
    border-radius: 99px;
    background: var(--ink-3);
    border: 1px solid var(--line);
    font-size: 10.5px;
    color: var(--muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }
  .cpill.on {
    background: rgba(var(--accent-rgb), 0.18);
    border-color: rgba(var(--accent-rgb), 0.45);
    color: rgb(var(--accent-rgb));
  }
  .cpill:disabled {
    opacity: 0.45;
  }

  .chev {
    color: var(--faint);
    display: grid;
    place-items: center;
    transition: transform 160ms var(--ease);
  }
  .chev.rot {
    transform: rotate(180deg);
    color: var(--text);
  }

  .cbody {
    padding: 10px 12px 12px;
    border-top: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 8px;
    animation: kaplaFadeUp 180ms var(--ease);
  }
  .cbody-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .cbody-title {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .cbody-acts {
    display: flex;
    gap: 6px;
  }
  .cmini {
    height: 24px;
    padding: 0 8px;
    border-radius: 7px;
    background: var(--ink-3);
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--muted);
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .cmini:hover {
    color: var(--text);
    border-color: rgba(var(--accent-rgb), 0.4);
  }

  .clist {
    display: flex;
    flex-direction: column;
    gap: 5px;
    max-height: 150px;
    overflow-y: auto;
  }
  .crow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 9px;
    border-radius: 8px;
    background: var(--ink-3);
    border: 1px solid transparent;
    text-align: left;
  }
  .crow.live {
    border-color: rgba(var(--accent-rgb), 0.42);
    background: rgba(var(--accent-rgb), 0.08);
  }
  .crow-l {
    display: flex;
    align-items: baseline;
    gap: 7px;
    min-width: 0;
    flex: 1;
  }
  .crow-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .crow-tags {
    font-size: 10px;
    color: var(--faint);
    white-space: nowrap;
  }
  .cbtn {
    height: 24px;
    padding: 0 9px;
    border-radius: 7px;
    background: var(--ink-4);
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--text);
    flex: none;
  }
  .cbtn.live {
    background: rgba(74, 214, 152, 0.14);
    border-color: rgba(74, 214, 152, 0.35);
    color: var(--ok);
  }
  .cbtn:hover:not(:disabled) {
    border-color: rgba(var(--accent-rgb), 0.5);
  }
  .cempty {
    font-size: 11.5px;
    color: var(--muted);
    padding: 6px 2px;
  }

  .vpn-actions {
    display: flex;
    gap: 7px;
  }
  .vpn-main {
    flex: 1;
    height: 34px;
    border-radius: var(--r-s);
    background: linear-gradient(155deg, rgba(255, 255, 255, 0.28) 0%, transparent 45%), rgb(var(--accent-rgb));
    color: #0b0e14;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.45), 0 6px 16px rgba(var(--accent-rgb), 0.3);
    font-weight: 600;
    font-size: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
  }
  .vpn-main.on {
    background: rgba(74, 214, 152, 0.16);
    color: var(--ok);
    border: 1px solid rgba(74, 214, 152, 0.4);
  }
  .ibtn {
    width: 34px;
    height: 34px;
    border-radius: var(--r-s);
    background: var(--ink-3);
    border: 1px solid var(--line);
    display: grid;
    place-items: center;
    color: var(--muted);
  }
  .vpn-tools {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .sbtn {
    height: 28px;
    border-radius: 8px;
    background: var(--ink-3);
    border: 1px solid var(--line);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--muted);
  }
  .sbtn:hover,
  .sbtn.active {
    color: var(--text);
    border-color: rgba(var(--accent-rgb), 0.45);
  }
  .ppick {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    text-align: left;
  }
  .pdot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ink-4);
    flex: none;
  }
  .pdot.on {
    background: rgb(var(--accent-rgb));
  }
  .pdel {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--faint);
  }
  .pdel:hover {
    background: var(--ink-4);
    color: var(--danger);
  }

  .pwr-row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .pwr-btn {
    height: 32px;
    border-radius: 8px;
    background: var(--ink-3);
    border: 1px solid var(--line);
    font-size: 12px;
    color: var(--muted);
  }
  .pwr-btn.active {
    background: rgba(var(--accent-rgb), 0.14);
    border-color: rgba(var(--accent-rgb), 0.5);
    color: var(--text);
  }
  .pwr-toggles {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border-radius: var(--r-s);
    background: var(--ink-3);
    border: 1px solid var(--line);
    text-align: left;
  }
  .row.on {
    border-color: rgba(var(--accent-rgb), 0.35);
  }
  .rleft {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .rl {
    font-size: 12px;
    font-weight: 500;
  }
  .rs {
    font-size: 10.5px;
    color: var(--muted);
  }
  .pill-sw {
    font-size: 10.5px;
    padding: 3px 8px;
    border-radius: 99px;
    background: var(--ink-4);
    color: var(--muted);
    flex: none;
  }
  .row.on .pill-sw {
    background: rgba(var(--accent-rgb), 0.2);
    color: rgb(var(--accent-rgb));
  }

  .mini-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .tile {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 9px 11px;
    border-radius: var(--r-s);
    background: var(--glass-bg);
    backdrop-filter: var(--glass-blur);
    border: 1px solid var(--line);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.14);
    text-align: left;
    transition: border-color 140ms var(--ease), background 140ms var(--ease), transform 140ms var(--ease);
  }
  .tile:hover {
    border-color: var(--line-2);
    background: var(--glass-bg-strong);
    transform: translateY(-2px);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.24), 0 8px 20px rgba(0, 0, 0, 0.25);
  }
  .tile:hover :global(svg.i) {
    transform: scale(1.12) rotate(-5deg);
    color: rgb(var(--accent-rgb));
  }
  .tile.danger {
    border-color: rgba(255, 95, 109, 0.4);
    color: var(--danger);
  }
  .tile.warn:hover {
    border-color: rgba(255, 95, 109, 0.45);
  }
  .tmeta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .tl {
    font-size: 12px;
    font-weight: 500;
    color: var(--text);
  }
  .ts {
    font-size: 10.5px;
    color: var(--muted);
  }

  .bottom-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    margin-top: auto;
    padding-top: 2px;
  }
  .swatches {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
  }
  .ch {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    margin-right: 3px;
  }
  .sw {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 9px;
    border-radius: 99px;
    background: var(--ink-2);
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--muted);
  }
  .sw.active {
    border-color: rgba(var(--accent-rgb), 0.65);
    color: var(--text);
  }
  .sdot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }
  .keys {
    display: flex;
    gap: 10px;
    font-size: 10.5px;
    color: var(--faint);
  }
  .keys b {
    color: var(--muted);
    font-weight: 500;
  }
</style>
