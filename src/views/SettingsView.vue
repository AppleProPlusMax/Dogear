<script setup lang="ts">
import { computed, onMounted, onUnmounted } from "vue";
import type { ThemePreference } from "../api";
import { useSettingsStore, type ShortcutField } from "../stores/settings";
import WindowControls from "../components/WindowControls.vue";

const emit = defineEmits<{ back: [] }>();
const store = useSettingsStore();

const themes: { id: ThemePreference; label: string }[] = [
  { id: "system", label: "跟随系统" },
  { id: "light", label: "浅色" },
  { id: "dark", label: "深色" },
];

const summonParts = computed(() => store.settings.summonShortcut.split("+"));
const captureParts = computed(() => store.settings.captureShortcut.split("+"));

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
    store.error = "这个按键不能用作快捷键";
    return;
  }
  const mods: string[] = [];
  if (event.ctrlKey) mods.push("Ctrl");
  if (event.altKey) mods.push("Alt");
  if (event.shiftKey) mods.push("Shift");
  if (event.metaKey) mods.push("Win");
  if (mods.length === 0) {
    store.error = "请至少带上 Ctrl、Alt、Shift 或 Win";
    return;
  }
  try {
    await store.commitCapture([...mods, key].join("+"));
  } catch {
    // 错误已经写在 store.error 上，继续录制。
  }
}

function toggle(field: "launchAtLogin" | "pauseRecording" | "reduceTransparency" | "ocrAuto" | "autoHide") {
  void store.update({ [field]: !store.settings[field] });
}

onMounted(() => {
  window.addEventListener("keydown", onCaptureKey, true);
});

onUnmounted(() => {
  window.removeEventListener("keydown", onCaptureKey, true);
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
        返回
      </button>
      <h1>设置</h1>
      <span class="edge"><WindowControls /></span>
    </header>

    <div class="body">
      <section>
        <h2>通用</h2>
        <div class="row">
          <div>
            <div class="label">呼出快捷键</div>
            <p>显示或隐藏历史窗口</p>
          </div>
          <div class="control">
            <span class="keys">
              <kbd v-for="part in parts('summon')" :key="part">{{ part }}</kbd>
            </span>
            <button type="button" class="text" @click="store.startCapture('summon')">
              {{ store.capturing === "summon" ? "请按键" : "更改" }}
            </button>
          </div>
        </div>
        <p v-if="store.capturing === 'summon'" class="capture-hint">按下组合键，Esc 取消</p>

        <div class="row">
          <div>
            <div class="label">截图快捷键</div>
            <p>开始框选截图</p>
          </div>
          <div class="control">
            <span class="keys">
              <kbd v-for="part in parts('capture')" :key="part">{{ part }}</kbd>
            </span>
            <button type="button" class="text" @click="store.startCapture('capture')">
              {{ store.capturing === "capture" ? "请按键" : "更改" }}
            </button>
          </div>
        </div>
        <p v-if="store.capturing === 'capture'" class="capture-hint">按下组合键，Esc 取消</p>

        <div class="row">
          <div>
            <div class="label">开机自启</div>
            <p>登录 Windows 后自动打开 Dogear</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.launchAtLogin"
            aria-label="开机自启"
            @click="toggle('launchAtLogin')"
          >
            <span></span>
          </button>
        </div>

        <div class="row">
          <div>
            <div class="label">暂停记录</div>
            <p>暂停后，新的复制不会记入历史。截图仍然保留</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.pauseRecording"
            aria-label="暂停记录"
            @click="toggle('pauseRecording')"
          >
            <span></span>
          </button>
        </div>

        <div class="row">
          <div>
            <div class="label">失焦后自动隐藏</div>
            <p>关掉后，点到别的窗口不会收起，用标题栏的缩小和关闭</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.autoHide"
            aria-label="失焦后自动隐藏"
            @click="toggle('autoHide')"
          >
            <span></span>
          </button>
        </div>

        <div class="row">
          <div>
            <div class="label">使用教程</div>
            <p>再看一遍关闭、呼出和粘贴</p>
          </div>
          <button type="button" class="text" @click="store.replayGuide()">再看一遍</button>
        </div>
      </section>

      <section>
        <h2>外观</h2>
        <div class="row">
          <div>
            <div class="label">主题</div>
            <p>默认跟随系统深浅色</p>
          </div>
          <div class="seg" role="radiogroup" aria-label="主题">
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
            <div class="label">减少透明效果</div>
            <p>改用纯色面板，适合显卡或远程桌面</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.reduceTransparency"
            aria-label="减少透明效果"
            @click="toggle('reduceTransparency')"
          >
            <span></span>
          </button>
        </div>
      </section>

      <section>
        <h2>识别</h2>
        <div class="row">
          <div>
            <div class="label">复制图片后自动识别</div>
            <p>关闭后仍可在预览里手动重新识别</p>
          </div>
          <button
            type="button"
            class="switch"
            role="switch"
            :aria-checked="store.settings.ocrAuto"
            aria-label="复制图片后自动识别"
            @click="toggle('ocrAuto')"
          >
            <span></span>
          </button>
        </div>
      </section>

      <p v-if="store.error" class="error">{{ store.error }}</p>
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
  overflow: auto;
  padding: 8px 20px 20px;
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
</style>
