// 主题偏好：light / dark / system，写入 localStorage 并同步 html class
import { ref } from "vue";

export type ThemePreference = "light" | "dark" | "system";

const STORAGE_KEY = "chatvault.theme";

const preference = ref<ThemePreference>(readStoredPreference());
const mediaQuery =
  typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;

/** 读取本地存储的主题偏好。 */
function readStoredPreference(): ThemePreference {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === "light" || raw === "dark" || raw === "system") return raw;
  } catch {
    /* 忽略存储异常，回落默认 */
  }
  return "light";
}

/** 根据偏好解析实际是否为深色。 */
function resolveDark(pref: ThemePreference): boolean {
  if (pref === "dark") return true;
  if (pref === "light") return false;
  return mediaQuery?.matches ?? false;
}

/** 将解析结果写入 documentElement class。 */
function applyTheme(pref: ThemePreference): void {
  const dark = resolveDark(pref);
  const root = document.documentElement;
  root.classList.toggle("dark", dark);
  root.classList.toggle("light", !dark);
}

let mediaListener: (() => void) | null = null;

/** 初始化主题：应用存储偏好，并在 system 时监听系统切换。 */
export function initTheme(): void {
  applyTheme(preference.value);
  if (!mediaQuery || mediaListener) return;
  mediaListener = () => {
    if (preference.value === "system") applyTheme("system");
  };
  mediaQuery.addEventListener("change", mediaListener);
}

/** 移除系统主题监听。 */
export function disposeTheme(): void {
  if (mediaListener && mediaQuery) mediaQuery.removeEventListener("change", mediaListener);
  mediaListener = null;
}

/** 设置主题偏好并持久化。 */
export function setThemePreference(pref: ThemePreference): void {
  preference.value = pref;
  try {
    localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    /* 忽略存储异常 */
  }
  applyTheme(pref);
}

export function useTheme() {
  return {
    preference,
    setThemePreference,
    initTheme,
    disposeTheme,
  };
}
