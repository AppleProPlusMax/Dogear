<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import markUrl from "../src-tauri/icons/128x128.png";
import {
  armWindowDrag,
  clipImage,
  clipThumb,
  copyClip,
  getEffectState,
  hidePanel,
  listClips,
  startCapture,
  pasteClip,
  retryOcr,
  type Clip,
} from "./api";

type FilterId = "all" | "text" | "link" | "image" | "code";

const filters: { id: FilterId; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "text", label: "文本" },
  { id: "link", label: "链接" },
  { id: "image", label: "图片" },
  { id: "code", label: "代码" },
];

const clips = ref<Clip[]>([]);
const query = ref("");
const filter = ref<FilterId>("all");
const selected = ref(0);
const notice = ref("");
const searchEl = ref<HTMLInputElement | null>(null);
/// 图片以 data URL 取回，按条目缓存，避免每次切换选中都重新读盘。
const thumbs = ref<Record<number, string>>({});
const fullImages = ref<Record<number, string>>({});
let unlisten: UnlistenFn[] = [];

const languageNames: Record<string, string> = {
  js: "JS",
  ts: "TS",
  python: "Python",
  shell: "Shell",
  html: "HTML",
  css: "CSS",
  rust: "Rust",
  go: "Go",
  java: "Java",
  json: "JSON",
  sql: "SQL",
  other: "代码",
};

function kindLabel(clip: Clip) {
  if (clip.kind === "link") return "链接";
  if (clip.kind === "image") return "图片";
  if (clip.kind === "code") return languageNames[clip.language ?? "other"] ?? "代码";
  return "文本";
}

function imageSize(clip: Clip) {
  if (!clip.width || !clip.height) return "图片";
  return `${clip.width} × ${clip.height}`;
}

/// 图片的副标题承担状态提示，所以识别中、失败这些都显示在这里。
function ocrLabel(clip: Clip) {
  if (clip.ocrStatus === "pending") return "正在识别文字";
  if (clip.ocrStatus === "failed") return "识别失败，可重试";
  if (clip.ocrStatus === "empty") return "未识别到文字";
  if (clip.ocrStatus === "done") return `已提取 ${(clip.ocrText ?? "").length} 字`;
  return "";
}

function itemTitle(clip: Clip) {
  if (clip.kind !== "image") return firstLine(clip.content);
  return imageSize(clip);
}

function itemSub(clip: Clip) {
  if (clip.kind !== "image") return `${kindLabel(clip)} · ${clip.content.length} 字`;
  const status = ocrLabel(clip);
  return status ? `图片 · ${status}` : "图片";
}

const searched = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return clips.value;
  return clips.value.filter(
    (clip) =>
      clip.content.toLowerCase().includes(q)
      || (clip.ocrText ?? "").toLowerCase().includes(q),
  );
});

function countOf(id: FilterId) {
  if (id === "all") return searched.value.length;
  return searched.value.filter((clip) => clip.kind === id).length;
}

const filtered = computed(() => {
  if (filter.value === "all") return searched.value;
  return searched.value.filter((clip) => clip.kind === filter.value);
});

const current = computed(() => filtered.value[selected.value] ?? null);

const groups = computed(() => {
  const buckets = new Map<string, { clip: Clip; index: number }[]>();
  filtered.value.forEach((clip, index) => {
    const label = dayLabel(clip.createdAt);
    const items = buckets.get(label) ?? [];
    items.push({ clip, index });
    buckets.set(label, items);
  });
  return [...buckets.entries()];
});

async function refresh() {
  clips.value = await listClips();
  if (selected.value >= filtered.value.length) selected.value = 0;
  void loadThumbs();
  void loadFullImage();
}

async function loadThumbs() {
  for (const clip of clips.value) {
    if (clip.kind !== "image" || thumbs.value[clip.id]) continue;
    try {
      thumbs.value[clip.id] = await clipThumb(clip.id);
    } catch {
      // 缩略图缺失时退回到类型图标，不打扰用户。
    }
  }
}

async function loadFullImage() {
  const clip = current.value;
  if (!clip || clip.kind !== "image" || fullImages.value[clip.id]) return;
  try {
    fullImages.value[clip.id] = await clipImage(clip.id);
  } catch (err) {
    notice.value = err instanceof Error ? err.message : String(err);
  }
}

const currentImage = computed(() => {
  const clip = current.value;
  if (!clip || clip.kind !== "image") return "";
  return fullImages.value[clip.id] ?? thumbs.value[clip.id] ?? "";
});

const currentText = computed(() => {
  const clip = current.value;
  if (!clip) return "";
  return clip.kind === "image" ? clip.ocrText ?? "" : clip.content;
});

function focusSearch() {
  nextTick(() => searchEl.value?.focus());
}

async function pasteSelected(asText = false) {
  const clip = current.value;
  if (!clip) return;
  notice.value = "";
  try {
    await pasteClip(clip.id, asText);
  } catch (err) {
    notice.value = err instanceof Error ? err.message : String(err);
  }
}

async function copySelected(asText = false) {
  const clip = current.value;
  if (!clip) return;
  notice.value = "";
  try {
    await copyClip(clip.id, asText);
    notice.value = asText ? "已复制文字" : "已复制";
  } catch (err) {
    notice.value = err instanceof Error ? err.message : String(err);
  }
}

async function recognizeSelected() {
  const clip = current.value;
  if (!clip || clip.kind !== "image") return;
  notice.value = "";
  try {
    await retryOcr(clip.id);
    await refresh();
  } catch (err) {
    notice.value = err instanceof Error ? err.message : String(err);
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "ArrowDown") {
    event.preventDefault();
    if (filtered.value.length === 0) return;
    selected.value = (selected.value + 1) % filtered.value.length;
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    if (filtered.value.length === 0) return;
    selected.value = (selected.value - 1 + filtered.value.length) % filtered.value.length;
  } else if (event.key === "Enter") {
    event.preventDefault();
    void pasteSelected(event.shiftKey);
  } else if (event.key === "o" && event.ctrlKey) {
    event.preventDefault();
    void recognizeSelected();
  }
}

function captureScreen() {
  void startCapture().catch((err) => {
    notice.value = err instanceof Error ? err.message : String(err);
  });
}

function onEscape(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  if (!("__TAURI_INTERNALS__" in window)) return;
  event.preventDefault();
  event.stopPropagation();
  void hidePanel();
}

function dragWouldStart(event: MouseEvent) {
  if (event.button !== 0 || event.detail > 2) return false;
  const path = event.composedPath();
  for (const node of path) {
    if (!(node instanceof HTMLElement)) continue;
    const attr = node.getAttribute("data-tauri-drag-region");
    const blocksDrag =
      node.matches("input, button, a, select, textarea, label, summary")
      || node.getAttribute("role") === "tab";
    if (blocksDrag && attr === null) return false;
    if (attr === null) continue;
    if (attr === "false") return false;
    if (attr === "deep") return true;
    return node === path[0];
  }
  return false;
}

function onDragPointerDown(event: MouseEvent) {
  if (!dragWouldStart(event)) return;
  void armWindowDrag();
}

function firstLine(content: string) {
  const line = content.split(/\r?\n/).find((item) => item.trim()) ?? "";
  return line.trim();
}

function dayLabel(createdAt: number) {
  const date = new Date(createdAt);
  const start = (value: Date) => new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
  const diff = start(new Date()) - start(date);
  if (diff <= 0) return "今天";
  if (diff === 86_400_000) return "昨天";
  return `${date.getMonth() + 1}月${date.getDate()}日`;
}

function relativeTime(createdAt: number) {
  const delta = Math.max(0, Date.now() - createdAt);
  const minutes = Math.floor(delta / 60000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  return dayLabel(createdAt);
}

function lineCount(content: string) {
  return content.split(/\r?\n/).length;
}

watch(selected, () => {
  void loadFullImage();
  nextTick(() => {
    document.querySelector(".item.on")?.scrollIntoView({ block: "nearest" });
  });
});

onMounted(async () => {
  window.addEventListener("keydown", onEscape, true);
  if (!("__TAURI_INTERNALS__" in window)) return;
  window.addEventListener("mousedown", onDragPointerDown, true);
  await Promise.all([
    refresh(),
    getEffectState().then((effect) => {
      document.documentElement.dataset.effect = effect.effect;
    }),
  ]);
  unlisten = [
    await listen("clip-added", () => {
      void refresh();
    }),
    await listen("ocr-updated", () => {
      void refresh();
    }),
    await listen("window-shown", () => {
      query.value = "";
      filter.value = "all";
      selected.value = 0;
      notice.value = "";
      focusSearch();
    }),
  ];
  focusSearch();
});

onUnmounted(() => {
  window.removeEventListener("keydown", onEscape, true);
  window.removeEventListener("mousedown", onDragPointerDown, true);
  for (const stop of unlisten) stop();
});
</script>

<template>
  <main class="panel" @keydown="onKeydown">
    <header class="top" data-tauri-drag-region>
      <div class="brand" data-tauri-drag-region>
        <img :src="markUrl" alt="" />
        <span>Dogear</span>
      </div>
      <div class="search" data-tauri-drag-region>
        <span class="search-icon" data-tauri-drag-region>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="11" cy="11" r="6.5" />
            <path d="M16 16l4.5 4.5" />
          </svg>
        </span>
        <input
          ref="searchEl"
          v-model="query"
          type="text"
          placeholder="搜索剪贴板"
          spellcheck="false"
          @input="selected = 0"
        />
        <div class="privacy" data-tauri-drag-region>
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <rect x="6" y="11" width="12" height="9" rx="2.5" />
            <path d="M8.5 11V8a3.5 3.5 0 0 1 7 0v3" />
          </svg>
          仅本机
        </div>
      </div>
      <div class="tabs" data-tauri-drag-region>
        <div class="seg" role="tablist">
          <button
            v-for="item in filters"
            :key="item.id"
            type="button"
            role="tab"
            :class="{ on: filter === item.id }"
            :aria-selected="filter === item.id"
            @click="filter = item.id; selected = 0"
          >
            {{ item.label }} <em>{{ countOf(item.id) }}</em>
          </button>
        </div>
        <button type="button" class="capture" @click="captureScreen">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M7 3H5a2 2 0 0 0-2 2v2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2" />
            <circle cx="12" cy="12" r="3.2" />
          </svg>
          截图
          <kbd>Alt</kbd><kbd>C</kbd>
        </button>
      </div>
    </header>

    <div class="body">
      <section v-if="filtered.length" class="list">
        <template v-for="[label, items] in groups" :key="label">
          <div class="grp">{{ label }}</div>
          <button
            v-for="item in items"
            :key="item.clip.id"
            type="button"
            class="item"
            :class="{ on: item.index === selected }"
            @click="selected = item.index"
            @dblclick="pasteSelected($event.shiftKey)"
          >
            <span v-if="thumbs[item.clip.id]" class="tile shot">
              <img :src="thumbs[item.clip.id]" alt="" />
            </span>
            <span v-else class="tile" :class="item.clip.kind">
              <svg v-if="item.clip.kind === 'link'" viewBox="0 0 24 24">
                <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" />
                <path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
              </svg>
              <svg v-else-if="item.clip.kind === 'code'" viewBox="0 0 24 24">
                <path d="M9 8L5 12l4 4" />
                <path d="M15 8l4 4-4 4" />
              </svg>
              <svg v-else-if="item.clip.kind === 'image'" viewBox="0 0 24 24">
                <rect x="4" y="5" width="16" height="14" rx="2.5" />
                <circle cx="9" cy="10" r="1.6" />
                <path d="M5 17l4.5-4.5 3.5 3.5 2.5-2.5L19 17" />
              </svg>
              <svg v-else viewBox="0 0 24 24">
                <path d="M5 7h14M5 12h14M5 17h9" />
              </svg>
            </span>
            <span class="meta">
              <span class="title" :class="{ mono: item.clip.kind === 'link' || item.clip.kind === 'code' }">{{ itemTitle(item.clip) }}</span>
              <span class="sub">
                {{ itemSub(item.clip) }}
                <i v-if="item.clip.ocrStatus === 'pending'" class="spin" aria-hidden="true"></i>
              </span>
            </span>
            <span class="time">{{ relativeTime(item.clip.createdAt) }}</span>
          </button>
        </template>
      </section>
      <section v-else class="list empty">
        <p v-if="clips.length === 0">还没有记录。复制一段文字或截一张图后再按 Alt+V。</p>
        <p v-else>没有匹配的记录。</p>
      </section>

      <section v-if="current" class="preview">
        <div class="ph">
          <div class="name">{{ kindLabel(current) }}</div>
          <div class="when">{{ relativeTime(current.createdAt) }}</div>
        </div>

        <template v-if="current.kind === 'image'">
          <div class="actions">
            <button type="button" class="btn" @click="copySelected()">复制图片</button>
            <button
              type="button"
              class="btn"
              :disabled="current.ocrStatus !== 'done'"
              @click="copySelected(true)"
            >
              复制文字
            </button>
            <button type="button" class="btn pri" @click="pasteSelected()">粘贴</button>
          </div>
          <div class="shotbox">
            <img v-if="currentImage" :src="currentImage" :alt="imageSize(current)" />
            <p v-else>图片读取中…</p>
          </div>
          <div class="ocr" :class="{ filled: currentText }">
            <div class="ocr-head">
              <span :class="{ ok: current.ocrStatus === 'done' }">
                {{ ocrLabel(current) }}
                <template v-if="current.ocrStatus === 'done' && current.ocrLang">
                  · {{ current.ocrLang }}
                </template>
              </span>
              <button type="button" class="link" @click="recognizeSelected">重新识别</button>
            </div>
            <div v-if="currentText" class="card">{{ currentText }}</div>
          </div>
          <div class="facts">
            <span><b>{{ current.width ?? 0 }}</b> × <b>{{ current.height ?? 0 }}</b> 像素</span>
            <i></i>
            <span><b>{{ currentText.length }}</b> 字</span>
          </div>
        </template>

        <template v-else>
          <div class="actions">
            <button type="button" class="btn" @click="copySelected()">复制</button>
            <button type="button" class="btn pri" @click="pasteSelected()">粘贴</button>
          </div>
          <div class="card" :class="{ mono: current.kind !== 'text' }">{{ current.content }}</div>
          <div class="facts">
            <span><b>{{ lineCount(current.content) }}</b> 行</span>
            <i></i>
            <span><b>{{ current.content.length }}</b> 字</span>
          </div>
        </template>

        <p v-if="notice" class="notice">{{ notice }}</p>
      </section>
      <section v-else class="preview quiet">
        <p>选中一条记录后，在这里看完整内容。</p>
      </section>
    </div>

    <footer>
      <div class="keys">
        <span><kbd>↑</kbd><kbd>↓</kbd> 选择</span>
        <span><kbd>Enter</kbd> 粘贴</span>
        <span><kbd>Shift</kbd><kbd>Enter</kbd> 粘贴文字</span>
        <span><kbd>Esc</kbd> 关闭</span>
      </div>
      <div class="count">Alt+V · {{ clips.length }} 条</div>
    </footer>
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
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35);
}
.top {
  padding: 14px 20px 0;
  cursor: grab;
}
.brand {
  height: 28px;
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
  cursor: grab;
}
.brand img,
.brand span {
  pointer-events: none;
}
.brand img {
  width: 22px;
  height: 22px;
  border-radius: 6px;
  display: block;
}
.brand span {
  font-size: 15px;
  font-weight: 650;
  letter-spacing: -0.02em;
}
.search-icon {
  display: flex;
  flex: none;
  cursor: grab;
}
.search {
  height: 46px;
  border-radius: 13px;
  background: var(--fill);
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 8px 0 14px;
}
.search svg {
  width: 18px;
  height: 18px;
  stroke: var(--text-2);
  fill: none;
  stroke-width: 1.9;
  stroke-linecap: round;
  stroke-linejoin: round;
  flex: none;
}
.search input {
  -webkit-app-region: no-drag;
  app-region: no-drag;
  cursor: text;
  flex: 1;
  min-width: 0;
  height: 100%;
  border: 0;
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 16px;
  outline: none;
}
.search input::placeholder {
  color: var(--text-3);
}
.privacy {
  cursor: grab;
  display: flex;
  align-items: center;
  gap: 5px;
  flex: none;
  font-size: 11.5px;
  color: var(--text-2);
  padding: 4px 9px;
  border-radius: 999px;
  background: var(--fill-2);
  border: 1px solid var(--card-border);
}
.privacy svg {
  width: 12px;
  height: 12px;
  stroke: var(--good);
  stroke-width: 2;
}
.tabs {
  margin: 14px 0 12px;
  cursor: grab;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.capture {
  -webkit-app-region: no-drag;
  app-region: no-drag;
  cursor: pointer;
  height: 34px;
  display: flex;
  align-items: center;
  gap: 6px;
  flex: none;
  padding: 0 8px 0 11px;
  border: 0;
  border-radius: 10px;
  background: var(--seg-thumb);
  color: var(--text);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  box-shadow: 0 0 0 1.5px var(--accent), 0 6px 16px var(--accent-ring);
}
.capture svg {
  width: 15px;
  height: 15px;
  stroke: var(--accent);
  fill: none;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.capture kbd {
  padding: 0 5px;
  line-height: 18px;
}
.seg {
  display: inline-flex;
  padding: 3px;
  border-radius: 11px;
  background: var(--fill);
  gap: 1px;
}
.seg button {
  border: 0;
  background: transparent;
  color: var(--text-2);
  font: inherit;
  font-size: 13px;
  font-weight: 500;
  padding: 6px 14px;
  border-radius: 8px;
  cursor: pointer;
}
.seg button em {
  font-style: normal;
  font-size: 11px;
  color: var(--text-3);
  margin-left: 4px;
}
.seg button.on {
  background: var(--seg-thumb);
  color: var(--text);
  box-shadow: var(--seg-shadow);
}
.body {
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
  display: grid;
  grid-template-columns: 340px minmax(0, 1fr);
  border-top: 1px solid var(--sep);
}
.list {
  min-height: 0;
  min-width: 0;
  overflow: auto;
  padding: 6px 8px 12px;
  border-right: 1px solid var(--sep);
}
.list.empty,
.preview.quiet {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-2);
  padding: 24px;
  text-align: center;
}
.grp {
  font-size: 11.5px;
  font-weight: 600;
  color: var(--text-3);
  padding: 10px 10px 4px;
}
.item {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 8px 10px;
  border: 0;
  border-radius: var(--r-item);
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.item.on {
  background: var(--accent-soft);
  box-shadow: inset 0 0 0 1px var(--accent-ring);
}
.tile {
  width: 38px;
  height: 38px;
  border-radius: 10px;
  flex: none;
  display: grid;
  place-items: center;
}
.tile.text {
  color: var(--tile-text);
  background: color-mix(in srgb, var(--tile-text) 14%, transparent);
}
.tile.link {
  color: var(--tile-link);
  background: color-mix(in srgb, var(--tile-link) 14%, transparent);
}
.tile.code {
  color: var(--tile-code);
  background: color-mix(in srgb, var(--tile-code) 14%, transparent);
}
.tile.image {
  color: var(--tile-image);
  background: color-mix(in srgb, var(--tile-image) 14%, transparent);
}
.tile.shot {
  overflow: hidden;
  background: var(--fill);
  box-shadow: inset 0 0 0 1px var(--card-border);
}
.tile.shot img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.tile svg {
  width: 18px;
  height: 18px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.meta {
  flex: 1;
  min-width: 0;
}
.title {
  display: block;
  font-size: 13.5px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.title.mono,
.card.mono {
  font-family: var(--font-mono);
  font-size: 12.5px;
}
.sub,
.time,
.when,
.facts,
.count {
  color: var(--text-3);
  font-size: 11.5px;
}
.sub {
  display: block;
  margin-top: 2px;
  color: var(--text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.time {
  flex: none;
  align-self: flex-start;
  margin-top: 3px;
}
.preview {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  padding: 18px 20px 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.ph {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
}
.name {
  font-size: 15px;
  font-weight: 600;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.btn {
  height: 32px;
  padding: 0 13px;
  border: 0;
  border-radius: 9px;
  background: var(--fill);
  color: var(--text);
  font: inherit;
  font-size: 12.5px;
  font-weight: 550;
  cursor: pointer;
}
.btn.pri {
  background: var(--accent);
  color: var(--on-accent);
}
.btn:disabled {
  color: var(--text-3);
  cursor: default;
}
.link {
  border: 0;
  background: transparent;
  color: var(--accent);
  font: inherit;
  font-size: 11.5px;
  font-weight: 550;
  padding: 0;
  cursor: pointer;
}
/* 图片按原比例完整放进剩余空间。百分比高度在网格里经常算不成，
   图片会按宽度撑开后被上下裁掉，所以改成绝对定位再 object-fit。 */
.shotbox {
  flex: 1 1 0;
  min-height: 0;
  position: relative;
  overflow: hidden;
  background: var(--card);
  border: 1px solid var(--card-border);
  border-radius: var(--r-card);
  color: var(--text-3);
}
.shotbox img {
  position: absolute;
  inset: 10px;
  width: calc(100% - 20px);
  height: calc(100% - 20px);
  object-fit: contain;
  border-radius: 6px;
}
.shotbox p {
  margin: 0;
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  font-size: 12px;
}
.ocr {
  flex: 0 0 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.ocr.filled {
  flex: 1 1 0;
}
.ocr-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  color: var(--text-2);
  font-size: 11.5px;
}
.ocr-head .ok {
  color: var(--good);
}
.ocr .card {
  flex: 1 1 0;
  min-height: 0;
  padding: 10px 12px;
  line-height: 1.6;
}
.spin {
  display: inline-block;
  width: 9px;
  height: 9px;
  margin-left: 5px;
  vertical-align: -1px;
  border: 1.5px solid var(--accent-ring);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .spin {
    animation: none;
  }
}
.card {
  flex: 1;
  min-height: 0;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  word-break: break-word;
  background: var(--card);
  border: 1px solid var(--card-border);
  border-radius: var(--r-card);
  padding: 14px 16px;
  line-height: 1.7;
}
.facts {
  display: flex;
  align-items: center;
  gap: 10px;
}
.facts b {
  color: var(--text);
  font-weight: 600;
}
.facts i {
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: var(--text-3);
}
.notice {
  margin: 0;
  color: var(--text-2);
  font-size: 12px;
}
footer {
  height: 48px;
  border-top: 1px solid var(--sep);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 18px;
  gap: 12px;
}
.keys {
  display: flex;
  gap: 14px;
  color: var(--text-2);
  font-size: 12px;
}
.keys span {
  display: flex;
  align-items: center;
  gap: 6px;
}
kbd {
  font-family: var(--font-ui);
  font-size: 11px;
  font-weight: 600;
  color: var(--text-2);
  background: var(--kbd-bg);
  border: 1px solid var(--kbd-border);
  border-bottom-width: 1.5px;
  border-radius: 6px;
  padding: 1px 6px;
  line-height: 1.5;
}
</style>
