import type { Clip, MediaInfo, ShelfItem, Sys, VpnStatus } from "./ipc";

export type Tab = "home" | "clipboard" | "shelf" | "text" | "note" | "browser" | "controls";
export type FlashKind = "volume" | "layout" | "caps" | "copied" | "mic" | "color" | "shot" | "vpn";
export type AccentPreset = "auto" | "amber" | "mint" | "sky" | "rose";

export const ACCENT_PRESETS: Record<Exclude<AccentPreset, "auto">, [number, number, number]> = {
  amber: [255, 122, 61],
  mint: [74, 214, 152],
  sky: [92, 168, 255],
  rose: [242, 102, 148],
};

export interface Flash {
  kind: FlashKind;
  at: number;
  text?: string;
}

function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

export const ui = $state({
  open: false,
  /** Открыт хоткеем/кликом/вводом — не сворачиваем, пока курсор не войдёт и не выйдет (или не сброшен). */
  pinned: false,
  tab: "home" as Tab,
  tiny: loadJson<boolean>("kapla.tiny", false),
  inputMode: false,
  through: false,
  captureVisible: false,
  accentPreset: loadJson<AccentPreset>("kapla.accentPreset", "auto"),
  scale: loadJson<number>("kapla.scale", 1),
  media: {
    active: false,
    title: "",
    artist: "",
    app: "",
    playing: false,
    position: 0,
    duration: 0,
    cover: null,
  } as MediaInfo,
  audioLevel: 0,
  sys: {
    battery: null,
    charging: false,
    layout: "",
    caps: false,
    volume: 0,
    muted: false,
    mic_muted: false,
    dark_theme: true,
  } as Sys,
  clips: [] as Clip[],
  editorClip: null as Clip | null,
  editorTool: "arrow" as "arrow" | "rect" | "brush" | "marker" | "blur" | "text" | "crop",
  editorSaveDir: loadJson<string>("kapla.editorSaveDir", ""),
  recentShot: null as { id: number; preview: string; text: string; at: number } | null,
  shelf: loadJson<ShelfItem[]>("kapla.shelf", []),
  dragOver: false,
  browserUrl: loadJson<string>("kapla.browserUrl", "https://chatgpt.com/"),
  weatherCity: loadJson<string>("kapla.weatherCity", ""),
  weather: null as import("./ipc").WeatherInfo | null,
  vpn: {
    configured: false,
    selected: null,
    profiles: [],
    app_running: false,
    tunnel_up: false,
    tunnel_name: null,
    connected: false,
  } as VpnStatus,
  accent: [255, 122, 61] as [number, number, number],
  flash: null as Flash | null,
  now: Date.now(),
});

export function saveShelf() {
  try {
    localStorage.setItem("kapla.shelf", JSON.stringify(ui.shelf));
  } catch {}
}

export function savePrefs() {
  try {
    localStorage.setItem("kapla.accentPreset", JSON.stringify(ui.accentPreset));
    localStorage.setItem("kapla.scale", JSON.stringify(ui.scale));
    localStorage.setItem("kapla.tiny", JSON.stringify(ui.tiny));
  } catch {}
}

export function saveEditorDir(dir: string) {
  ui.editorSaveDir = dir;
  try {
    localStorage.setItem("kapla.editorSaveDir", JSON.stringify(dir));
  } catch {}
}

export function openImageEditor(
  clip: Clip,
  tool: "arrow" | "rect" | "brush" | "marker" | "blur" | "text" | "crop" = "arrow",
) {
  ui.editorTool = tool;
  ui.editorClip = clip;
  ui.open = true;
  ui.pinned = true;
}

let flashTimer: ReturnType<typeof setTimeout> | undefined;
export function flash(kind: FlashKind, text?: string, ms = 1700) {
  ui.flash = { kind, at: Date.now(), text };
  clearTimeout(flashTimer);
  flashTimer = setTimeout(() => (ui.flash = null), ms);
}

export function fmtTime(sec: number) {
  if (!isFinite(sec) || sec < 0) sec = 0;
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

export function fmtSize(bytes: number) {
  if (!bytes) return "папка";
  if (bytes < 1024) return `${bytes} Б`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} КБ`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} ГБ`;
}

export function timeAgo(ts: number, now: number) {
  const d = Math.max(0, now - ts) / 1000;
  if (d < 45) return "только что";
  if (d < 3600) return `${Math.round(d / 60)} мин назад`;
  if (d < 86400) return `${Math.round(d / 3600)} ч назад`;
  return new Date(ts).toLocaleDateString("ru-RU", { day: "numeric", month: "short" });
}
