import type { Tab } from "./state.svelte";
import type { IconName } from "./icons";

export interface TabMeta {
  id: Tab;
  label: string;
  hint: string;
  icon: IconName;
}

export const TAB_META: TabMeta[] = [
  { id: "home", label: "Главная", hint: "Музыка, погода и быстрые действия", icon: "home" },
  { id: "clipboard", label: "Буфер", hint: "История копирования", icon: "clipboard" },
  { id: "shelf", label: "Полка", hint: "Файлы и конвертер", icon: "shelf" },
  { id: "text", label: "Текст", hint: "Преобразование текста", icon: "text" },
  { id: "note", label: "Заметка", hint: "Быстрые записи и таймер", icon: "note" },
  { id: "browser", label: "Браузер", hint: "Встроенный мини-браузер", icon: "globe" },
  { id: "controls", label: "Пульт", hint: "Сеть, VPN и система", icon: "controls" },
];

export const themeOf = (t: Tab) => TAB_META.find((x) => x.id === t) ?? TAB_META[0];
export const tabIndex = (t: Tab) => Math.max(0, TAB_META.findIndex((x) => x.id === t));
