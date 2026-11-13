import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const WIN_W = 860;
export const WIN_H = 540;

export interface MediaInfo {
  active: boolean;
  title: string;
  artist: string;
  app: string;
  playing: boolean;
  position: number;
  duration: number;
  cover: string | null;
}

export interface Sys {
  battery: number | null;
  charging: boolean;
  layout: string;
  caps: boolean;
  volume: number;
  muted: boolean;
  mic_muted: boolean;
  dark_theme: boolean;
}

export interface Clip {
  id: number;
  kind: "text" | "image";
  text: string;
  preview?: string | null;
  ts: number;
  pinned: boolean;
}

export interface ShelfItem {
  path: string;
  name: string;
  size: number;
  is_dir: boolean;
}

export interface ConvertTarget {
  id: string;
  label: string;
  hint: string;
  supports_quality: boolean;
  supports_resize: boolean;
  via_canvas: boolean;
}

export interface ConvertProbe {
  path: string;
  name: string;
  ext: string;
  size: number;
  kind: string;
  width: number | null;
  height: number | null;
  has_ffmpeg: boolean;
  targets: ConvertTarget[];
}

export interface ConvertResult {
  item: ShelfItem;
  src_size: number;
  out_size: number;
  ms: number;
}

export type VpnKind = "app" | "system";

export interface VpnProfile {
  id: string;
  name: string;
  kind: VpnKind;
  path: string;
  args: string;
}

export interface VpnStatus {
  configured: boolean;
  selected: VpnProfile | null;
  profiles: VpnProfile[];
  app_running: boolean;
  tunnel_up: boolean;
  tunnel_name: string | null;
  connected: boolean;
}

export interface CursorEvt {
  x: number;
  y: number;
  edge: boolean;
  near: boolean;
  dragging: boolean;
}

export interface BrowserNavEvt {
  url: string;
  loading: boolean;
}

export interface ClipCapturedEvt {
  kind: "text" | "image";
  text: string;
}

export interface EditedImageResult {
  saved_path: string | null;
  clip_id: number | null;
}

export interface WeatherInfo {
  city: string;
  temp_c: number;
  feels_like_c: number;
  desc: string;
  icon: string;
  humidity: number;
  wind_kmph: number;
  min_c: number | null;
  max_c: number | null;
}

export interface WifiNet {
  ssid: string;
  signal: number;
  secured: boolean;
  saved: boolean;
  connected: boolean;
}

export interface BtDev {
  name: string;
  addr: string;
  connected: boolean;
  is_audio: boolean;
}

export interface ConnectivityState {
  wifi_has_radio: boolean;
  wifi_on: boolean;
  wifi_ssid: string | null;
  wifi_signal: number | null;
  wifi_nets: WifiNet[];
  bt_has_radio: boolean;
  bt_on: boolean;
  bt_devices: BtDev[];
  power_plan: string;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export const isTauri = "__TAURI_INTERNALS__" in window;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) return Promise.reject(new Error("not in tauri"));
  return invoke<T>(cmd, args);
}

export const api = {
  setHitRect: (rect: Rect | null) => call<void>("set_hit_rect", { rect }),
  setThrough: (on: boolean) => call<void>("set_through", { on }),
  setCaptureVisible: (visible: boolean) => call<void>("set_capture_visible", { visible }),
  beginInput: () => call<void>("begin_input"),
  endInput: () => call<void>("end_input"),
  media: (action: "toggle" | "next" | "prev") => call<void>("media_control", { action }),
  mediaNow: () => call<MediaInfo>("media_now"),
  sysNow: () => call<Sys>("sys_now"),
  setVolume: (value: number) => call<void>("set_volume", { value }),
  toggleMute: () => call<void>("toggle_mute"),
  toggleMic: () => call<boolean>("toggle_mic"),
  toggleTheme: () => call<boolean>("toggle_windows_theme"),
  clips: () => call<Clip[]>("clip_list"),
  setClip: (text: string) => call<void>("clip_set", { text }),
  paste: (id: number) => call<void>("clip_paste", { id }),
  pin: (id: number) => call<void>("clip_pin", { id }),
  remove: (id: number) => call<void>("clip_delete", { id }),
  clearClips: () => call<void>("clip_clear"),
  clipImageData: (id: number) => call<string>("clip_image_data", { id }),
  clipSaveEdited: (
    pngBytes: number[],
    copyClipboard: boolean,
    savePath: string | null,
    saveDir: string | null,
  ) =>
    call<EditedImageResult>("clip_save_edited", {
      pngBytes,
      copyClipboard,
      savePath,
      saveDir,
    }),
  openSnip: () => call<void>("open_snip"),
  lockPc: () => call<void>("lock_pc"),
  captureScreen: () => call<string>("capture_screen"),
  pickColor: () => call<string | null>("pick_color"),
  openPath: (path: string) => call<void>("open_path", { path }),
  revealPath: (path: string) => call<void>("reveal_path", { path }),
  shelfMeta: (paths: string[]) => call<ShelfItem[]>("shelf_meta", { paths }),
  pickFiles: () => call<ShelfItem[]>("pick_files"),
  defaultScreenshotsDir: () => call<string>("default_screenshots_dir"),
  pickSaveImagePath: (defaultName: string, initialDir: string | null) =>
    call<string | null>("pick_save_image_path", { defaultName, initialDir }),
  pickFolder: () => call<string | null>("pick_folder"),
  shelfPrepareDrop: () => call<void>("shelf_prepare_drop"),
  shelfCopyFiles: (paths: string[]) => call<void>("shelf_copy_files", { paths }),
  shelfStartDrag: (path: string) => call<boolean>("shelf_start_drag", { path }),
  convertProbe: (path: string) => call<ConvertProbe>("convert_probe", { path }),
  convertFile: (
    path: string,
    target: string,
    quality: number | null,
    maxSide: number | null,
  ) => call<ConvertResult>("convert_file", { path, target, quality, maxSide }),
  convertReadB64: (path: string) => call<string>("convert_read_b64", { path }),
  convertSaveCanvas: (srcPath: string, target: string, bytes: number[]) =>
    call<ConvertResult>("convert_save_canvas", { srcPath, target, bytes }),
  convertZipBundle: (paths: string[]) => call<ConvertResult>("convert_zip_bundle", { paths }),
  browserOpen: (
    url: string | null,
    fallbackUrl: string | null,
    x: number,
    y: number,
    w: number,
    h: number,
  ) => call<string>("browser_open", { url, fallbackUrl, x, y, w, h }),
  browserFocus: () => call<void>("browser_focus"),
  browserHide: () => call<void>("browser_hide"),
  browserGo: (input: string) => call<string>("browser_go", { input }),
  browserNav: (action: "back" | "forward" | "reload") => call<void>("browser_nav", { action }),
  browserOpenExternal: (url: string) => call<void>("browser_open_external", { url }),
  weatherNow: (city: string | null, force = false) =>
    call<WeatherInfo>("weather_now", { city, force }),
  connectivityStatus: (scan = false) => call<ConnectivityState>("connectivity_status", { scan }),
  wifiToggle: (on: boolean) => call<ConnectivityState>("wifi_toggle", { on }),
  wifiConnect: (ssid: string) => call<ConnectivityState>("wifi_connect", { ssid }),
  wifiDisconnect: () => call<ConnectivityState>("wifi_disconnect"),
  btToggle: (on: boolean) => call<ConnectivityState>("bt_toggle", { on }),
  btConnect: (addr: string, connect: boolean) =>
    call<ConnectivityState>("bt_connect", { addr, connect }),
  powerSet: (mode: "saver" | "balanced" | "performance") => call<string>("power_set", { mode }),
  openWinSettings: (uri: string) => call<void>("open_win_settings", { uri }),
  vpnStatus: () => call<VpnStatus>("vpn_status"),
  vpnDiscover: () => call<VpnProfile[]>("vpn_discover"),
  vpnPickApp: () => call<VpnStatus>("vpn_pick_app"),
  vpnAddProfile: (name: string, kind: VpnKind, path: string) =>
    call<VpnStatus>("vpn_add_profile", { name, kind, path }),
  vpnSelect: (id: string) => call<VpnStatus>("vpn_select", { id }),
  vpnRemove: (id: string) => call<VpnStatus>("vpn_remove", { id }),
  vpnConnect: () => call<VpnStatus>("vpn_connect"),
  vpnDisconnect: () => call<VpnStatus>("vpn_disconnect"),
  vpnToggle: () => call<VpnStatus>("vpn_toggle"),
  vpnOpenApp: () => call<void>("vpn_open_app"),
};

export function on<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  if (!isTauri) return Promise.resolve(() => {});
  return listen<T>(event, (e) => cb(e.payload));
}
