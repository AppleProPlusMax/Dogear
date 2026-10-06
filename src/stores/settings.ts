import { listen } from "@tauri-apps/api/event";
import { defineStore } from "pinia";
import { ref } from "vue";
import {
  beginShortcutCapture,
  endShortcutCapture,
  getSettings,
  updateSettings,
  type Settings,
  type SettingsPatch,
  type ThemePreference,
} from "../api";

const defaults: Settings = {
  summonShortcut: "Alt+V",
  captureShortcut: "Alt+C",
  launchAtLogin: false,
  pauseRecording: false,
  theme: "system",
  reduceTransparency: false,
  ocrAuto: true,
};

export type ShortcutField = "summon" | "capture";

function message(err: unknown) {
  return err instanceof Error ? err.message : String(err);
}

export function applyTheme(theme: string) {
  const root = document.documentElement;
  if (theme === "light" || theme === "dark") root.dataset.theme = theme;
  else delete root.dataset.theme;
}

export const useSettingsStore = defineStore("settings", () => {
  const settings = ref<Settings>({ ...defaults });
  const capturing = ref<ShortcutField | null>(null);
  const error = ref("");
  let loaded = false;
  let listening = false;

  async function load() {
    if (!loaded) {
      try {
        settings.value = await getSettings();
        applyTheme(settings.value.theme);
        loaded = true;
      } catch (err) {
        error.value = message(err);
      }
    }
    if (!listening && "__TAURI_INTERNALS__" in window) {
      listening = true;
      await listen<Settings>("settings-changed", (event) => {
        settings.value = event.payload;
        applyTheme(event.payload.theme);
      });
      await listen("shortcut-capture-cancel", () => {
        capturing.value = null;
      });
    }
  }

  async function update(patch: SettingsPatch) {
    error.value = "";
    try {
      settings.value = await updateSettings(patch);
      applyTheme(settings.value.theme);
    } catch (err) {
      error.value = message(err);
      throw err;
    }
  }

  async function setTheme(theme: ThemePreference) {
    if (settings.value.theme === theme) return;
    await update({ theme });
  }

  async function startCapture(field: ShortcutField) {
    error.value = "";
    capturing.value = field;
    try {
      await beginShortcutCapture();
    } catch (err) {
      capturing.value = null;
      error.value = message(err);
    }
  }

  async function commitCapture(shortcut: string) {
    const field = capturing.value;
    if (!field) return;
    const patch: SettingsPatch = field === "summon"
      ? { summonShortcut: shortcut }
      : { captureShortcut: shortcut };
    await update(patch);
    capturing.value = null;
    await endShortcutCapture();
  }

  async function cancelCapture() {
    capturing.value = null;
    error.value = "";
    if (!("__TAURI_INTERNALS__" in window)) return;
    try {
      await endShortcutCapture();
    } catch (err) {
      error.value = message(err);
    }
  }

  return {
    settings,
    capturing,
    error,
    load,
    update,
    setTheme,
    startCapture,
    commitCapture,
    cancelCapture,
  };
});
