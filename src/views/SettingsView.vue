<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { clearClips, storageUsage } from "../api";
import { useSettingsStore, type ShortcutField } from "../stores/settings";
import { useI18n, type AppLocale } from "../i18n";
import WindowControls from "../components/WindowControls.vue";

const emit = defineEmits<{ back: [] }>();
const store = useSettingsStore();
const { t } = useI18n();
const version = ref("");
const ignoreDraft = ref("");
const clearArmed = ref(false);
const clearNote = ref("");
const storageTotal = ref<number | null>(null);
const storageKept = ref<number | null>(null);
let clearTimer = 0;

const storageText = computed(() => {
  if (storageTotal.value === null || storageKept.value === null) return "";
  return t("storageUse", {
    total: formatBytes(storageTotal.value),
    kept: formatBytes(storageKept.value),
  });
});

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let size = bytes / 1024;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  const digits = size >= 10 ? 0 : 1;
  return `${size.toFixed(digits)} ${units[unit]}`;
}

async function loadStorage() {
  if (!("__TAURI_INTERNALS__" in window)) return;
  try {
    const usage = await storageUsage();
    storageTotal.value = usage.total;
    storageKept.value = usage.kept;
  } catch {
    storageTotal.value = null;
    storageKept.value = null;
  }
}

const historySizes = [200, 500, 1000] as const;

const languages: { id: AppLocale; label: string }[] = [
  { id: "zh-CN", label: "简体中文" },
  { id: "en", label: "English" },
];

const themes = computed(() => [
  { id: "system" as const, label: t("themeSystem") },
  { id: "light" as const, label: t("themeLight") },
  { id: "dark" as const, label: t("themeDark") },
]);

const summonParts = computed(() => store.settings.summonShortcut.split("+"));
const captureParts = computed(() => store.settings.captureShortcut.split("+"));

function addIgnore() {
  const name = ignoreDraft.value.trim();
  if (!name) return;
  ignoreDraft.value = "";
  void store.update({ ignoredApps: [...store.settings.ignoredApps, name] });
}

function removeIgnore(name: string) {
  void store.update({
    ignoredApps: store.settings.ignoredApps.filter((item) => item !== name),
  });
}

function setHistoryLimit(limit: 200 | 500 | 1000) {
  if (store.settings.historyLimit === limit) return;
  void store.update({ historyLimit: limit });
}

async function clearKept() {
  clearNote.value = "";
  if (!clearArmed.value) {
    clearArmed.value = true;
    window.clearTimeout(clearTimer);
    clearTimer = window.setTimeout(() => {
      clearArmed.value = false;
    }, 3000);
    return;
  }
  window.clearTimeout(clearTimer);
  clearArmed.value = false;
  try {
    await clearClips();
    clearNote.value = t("clearedKept");
    await loadStorage();
  } catch (err) {
    clearNote.value = err instanceof Error ? err.message : String(err);
  }
}

function parts(field: ShortcutField) {
  return field === "summon" ? summonParts.value : captureParts.value;
}

function tokenFromCode(code: string) {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  if (code.startsWith("Numpad")) return code;
  const named: Record<string, string> = {
    Space: "Space",
    Tab: "Tab",
    Enter: "Enter",
    Backspace: "Backspace",
    Delete: "Delete",
    Insert: "Insert",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Minus: "Minus",
    Equal: "Equal",
    BracketLeft: "BracketLeft",
    BracketRight: "BracketRight",
    Backslash: "Backslash",
    Semicolon: "Semicolon",
    Quote: "Quote",
    Comma: "Comma",
    Period: "Period",
    Slash: "Slash",
    Backquote: "Backquote",
  };
  return named[code] ?? null;
}

async function onCaptureKey(event: KeyboardEvent) {
  if (!store.capturing || event.repeat) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape") {
    await store.cancelCapture();
    return;
  }
  if (event.key === "Control" || event.key === "Alt" || event.key === "Shift" || event.key === "Meta") {
    return;
  }
  const key = tokenFromCode(event.code);
  if (!key) {
    store.error = t("keyRejected");
    return;
  }
  const mods: string[] = [];
  if (event.ctrlKey) mods.push("Ctrl");
  if (event.altKey) mods.push("Alt");
  if (event.shiftKey) mods.push("Shift");
  if (event.metaKey) mods.push("Win");
  if (mods.length === 0) {
    store.error = t("needModifier");
    return;
  }
  try {
    await store.commitCapture([...mods, key].join("+"));
  } catch {
    // 错误已经写在 store.error 上，继续录制。
  }
}

function toggle(field: "launchAtLogin" | "pauseRecording" | "reduceTransparency" | "ocrAuto" | "autoHide" | "showSource") {
  void store.update({ [field]: !store.settings[field] });
}

onMounted(() => {
  window.addEventListener("keydown", onCaptureKey, true);
  if ("__TAURI_INTERNALS__" in window) {
    void getVersion().then((value) => {
      version.value = value;
    });
    void loadStorage();
  }
});

onUnmounted(() => {
  window.removeEventListener("keydown", onCaptureKey, true);
  window.clearTimeout(clearTimer);
  if (store.capturing) void store.cancelCapture();
});
</script>

<template>
  <main class="panel">
    <header class="top" data-tauri-drag-region>
      <button type="button" class="back" @click="emit('back')">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M15 6l-6 6 6 6" />
        </svg>
        {{ t("back") }}
      </button>
      <h1>{{ t("settingsTitle") }}</h1>
      <span class="edge"><WindowControls /></span>
    </header>

    <div class="body">
      <section>
        <h2>{{ t("general") }}</h2>
        <div class="row">
          <div>
            <div class="label">{{ t("language") }}</div>
            <p>{{ t("languageHint") }}</p>
          </div>
          <div class="seg" role="radiogroup" :aria-label="t('language')">
            <button
              v-for="item in languages"
              :key="item.id"
              type="button"
              role="radio"
              :aria-checked="store.settings.language === item.id"
              :class="{ on: store.settings.language === item.id }"
              @click="store.setLanguage(item.id)"
            >
              {{ item.label }}
            </button>
          </div>
        </div>
        <div class="row">
          <div>
            <div class="label">{{ t("launch") }}</div>
            <p>{{ t("launchHint") }}</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.launchAtLogin"
            :aria-label="t('launch')"
            @click="toggle('launchAtLogin')"
          >
            <span></span>
          </button>
        </div>
        <div class="row">
          <div>
            <div class="label">{{ t("autoHide") }}</div>
            <p>{{ t("autoHideHint") }}</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.autoHide"
            :aria-label="t('autoHide')"
            @click="toggle('autoHide')"
          >
            <span></span>
          </button>
        </div>
        <div class="row">
          <div>
            <div class="label">{{ t("guide") }}</div>
            <p>{{ t("guideHint") }}</p>
          </div>
          <button type="button" class="text" @click="store.replayGuide()">{{ t("guideAgain") }}</button>
        </div>
      </section>

      <section>
        <h2>{{ t("shortcuts") }}</h2>
        <div class="row">
          <div>
            <div class="label">{{ t("summon") }}</div>
            <p>{{ t("summonHint") }}</p>
          </div>
          <div class="control">
            <span class="keys">
              <kbd v-for="part in parts('summon')" :key="part">{{ part }}</kbd>
            </span>
            <button type="button" class="text" @click="store.startCapture('summon')">
              {{ store.capturing === "summon" ? t("pressKeys") : t("change") }}
            </button>
          </div>
        </div>
        <p v-if="store.capturing === 'summon'" class="capture-hint">{{ t("captureHintKeys") }}</p>

        <div class="row">
          <div>
            <div class="label">{{ t("capture") }}</div>
            <p>{{ t("captureHint") }}</p>
          </div>
          <div class="control">
            <span class="keys">
              <kbd v-for="part in parts('capture')" :key="part">{{ part }}</kbd>
            </span>
            <button type="button" class="text" @click="store.startCapture('capture')">
              {{ store.capturing === "capture" ? t("pressKeys") : t("change") }}
            </button>
          </div>
        </div>
        <p v-if="store.capturing === 'capture'" class="capture-hint">{{ t("captureHintKeys") }}</p>
      </section>

      <section>
        <h2>{{ t("history") }}</h2>
        <div class="row">
          <div>
            <div class="label">{{ t("pause") }}</div>
            <p>{{ t("pauseHint") }}</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.pauseRecording"
            :aria-label="t('pause')"
            @click="toggle('pauseRecording')"
          >
            <span></span>
          </button>
        </div>

        <div class="row">
          <div>
            <div class="label">{{ t("showSource") }}</div>
            <p>{{ t("showSourceHint") }}</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.showSource"
            :aria-label="t('showSource')"
            @click="toggle('showSource')"
          >
            <span></span>
          </button>
        </div>

        <div class="ignore">
          <div class="label">{{ t("ignoredApps") }}</div>
          <p>{{ t("ignoredAppsHint") }}</p>
          <div v-if="store.settings.ignoredApps.length" class="chips">
            <span v-for="name in store.settings.ignoredApps" :key="name" class="chip">
              {{ name }}
              <button type="button" :aria-label="t('removeApp', { name })" @click="removeIgnore(name)">×</button>
            </span>
          </div>
          <form class="add" @submit.prevent="addIgnore">
            <input v-model="ignoreDraft" :placeholder="t('ignoredPlaceholder')" :aria-label="t('ignoredApps')" />
            <button type="submit" class="text">{{ t("addApp") }}</button>
          </form>
        </div>

        <div class="row">
          <div>
            <div class="label">{{ t("historyLimit") }}</div>
            <p>{{ t("historyLimitHint") }}</p>
          </div>
          <div class="seg" role="radiogroup" :aria-label="t('historyLimit')">
            <button
              v-for="size in historySizes"
              :key="size"
              type="button"
              role="radio"
              :aria-checked="store.settings.historyLimit === size"
              :class="{ on: store.settings.historyLimit === size }"
              @click="setHistoryLimit(size)"
            >
              {{ size }}
            </button>
          </div>
        </div>

        <div class="row">
          <div>
            <div class="label">{{ t("clearKept") }}</div>
            <p v-if="storageText">{{ storageText }}</p>
            <p>{{ clearNote || t("clearKeptHint") }}</p>
          </div>
          <button type="button" class="text danger" @click="clearKept()">
            {{ clearArmed ? t("confirmClear") : t("clearAll") }}
          </button>
        </div>
      </section>

      <section>
        <h2>{{ t("appearance") }}</h2>
        <div class="row">
          <div>
            <div class="label">{{ t("theme") }}</div>
            <p>{{ t("themeHint") }}</p>
          </div>
          <div class="seg" role="radiogroup" :aria-label="t('theme')">
            <button
              v-for="item in themes"
              :key="item.id"
              type="button"
              role="radio"
              :aria-checked="store.settings.theme === item.id"
              :class="{ on: store.settings.theme === item.id }"
              @click="store.setTheme(item.id)"
            >
              {{ item.label }}
            </button>
          </div>
        </div>
        <div class="row">
          <div>
            <div class="label">{{ t("reduceTransparency") }}</div>
            <p>{{ t("reduceTransparencyHint") }}</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.reduceTransparency"
            :aria-label="t('reduceTransparency')"
            @click="toggle('reduceTransparency')"
          >
            <span></span>
          </button>
        </div>
      </section>

      <section>
        <h2>{{ t("recognition") }}</h2>
        <div class="row">
          <div>
            <div class="label">{{ t("ocrAuto") }}</div>
            <p>{{ t("ocrAutoHint") }}</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.ocrAuto"
            :aria-label="t('ocrAuto')"
            @click="toggle('ocrAuto')"
          >
            <span></span>
          </button>
        </div>
      </section>

      <p v-if="store.error" class="error">{{ store.error }}</p>
      <p v-if="version" class="version">{{ t("version", { version }) }}</p>
    </div>
  </main>
</template>

<style scoped>
.panel {
  box-sizing: border-box;
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--panel);
  color: var(--text);
  border: 1px solid var(--panel-border);
  border-radius: var(--r-panel);
  overflow: hidden;
}
.top {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 14px 16px 8px;
  cursor: grab;
}
.top h1 {
  margin: 0;
  flex: 1;
  font-size: 20px;
  line-height: 28px;
  font-weight: 600;
  letter-spacing: -0.02em;
}
.edge {
  margin-left: auto;
  display: flex;
}
.back,
.text,
.seg button,
.switch {
  -webkit-app-region: no-drag;
  app-region: no-drag;
  cursor: pointer;
  font: inherit;
}
.back {
  display: flex;
  align-items: center;
  gap: 2px;
  height: 32px;
  padding: 0 8px 0 4px;
  border: 0;
  border-radius: var(--r-ctl);
  background: transparent;
  color: var(--text-2);
}
.back:hover,
.text:hover {
  background: var(--fill);
  color: var(--text);
}
.back svg {
  width: 16px;
  height: 16px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: auto;
  padding: 8px 20px 20px;
}
section {
  flex: none;
}
section + section {
  margin-top: 18px;
}
h2 {
  margin: 0 0 6px;
  font-size: 12px;
  font-weight: 650;
  letter-spacing: 0.04em;
  color: var(--text-3);
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-height: 58px;
  padding: 8px 0;
  border-top: 1px solid var(--sep);
}
.row:first-of-type {
  border-top: 0;
}
.ignore {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 8px;
  padding: 10px 0 12px;
  border-top: 1px solid var(--sep);
}
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 4px 0 8px;
  border-radius: 8px;
  background: var(--fill);
  color: var(--text);
  font-size: 12px;
}
.chip button {
  width: 18px;
  height: 18px;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--text-2);
  line-height: 18px;
}
.chip button:hover {
  background: var(--fill-2);
  color: var(--danger);
}
.add {
  display: flex;
  gap: 8px;
}
.add input {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--sep);
  border-radius: var(--r-ctl);
  background: var(--fill);
  color: var(--text);
  font: inherit;
  font-size: 13px;
}
.label {
  font-size: 14px;
  font-weight: 600;
}
.row p,
.capture-hint,
.error {
  margin: 2px 0 0;
  color: var(--text-2);
  font-size: 12px;
}
.control {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: none;
}
.keys {
  display: flex;
  gap: 4px;
}
kbd {
  font-family: var(--font-ui);
  font-size: 11px;
  font-weight: 600;
  line-height: 20px;
  padding: 0 6px;
  border-radius: 6px;
  background: var(--kbd-bg);
  border: 1px solid var(--kbd-border);
  color: var(--text);
}
.text {
  height: 28px;
  padding: 0 10px;
  border: 0;
  border-radius: var(--r-ctl);
  background: var(--fill);
  color: var(--text);
}
.text.danger {
  color: var(--danger);
}
.seg {
  display: inline-flex;
  padding: 3px;
  border-radius: 11px;
  background: var(--fill);
  flex: none;
}
.seg button {
  height: 28px;
  padding: 0 10px;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--text-2);
}
.seg button.on {
  background: var(--seg-thumb);
  color: var(--text);
  box-shadow: var(--seg-shadow);
}
.switch {
  width: 40px;
  height: 24px;
  padding: 3px;
  border: 0;
  border-radius: 999px;
  background: var(--fill);
  flex: none;
}
.switch span {
  display: block;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--text-3);
  transform: translateX(0);
  transition: transform 160ms var(--ease);
}
.switch[aria-checked="true"] {
  background: var(--accent-soft);
}
.switch[aria-checked="true"] span {
  background: var(--accent);
  transform: translateX(16px);
}
.error {
  color: var(--text);
  margin-top: 12px;
}
.version {
  flex: none;
  margin: auto 0 0;
  padding-top: 28px;
  text-align: center;
  color: var(--text-3);
  font-size: 12px;
}
</style>
