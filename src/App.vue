<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { copyClip, getEffectState, hidePanel, listClips, pasteClip, type Clip } from "./api";

type FilterId = "all" | "text" | "link" | "image" | "code";
type Kind = "text" | "link";

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
let unlisten: UnlistenFn[] = [];

function kindOf(content: string): Kind {
  const text = content.trim();
  if (/^https?:\/\/\S+$/i.test(text) || /^www\.\S+$/i.test(text)) return "link";
  return "text";
}

function kindLabel(kind: Kind) {
  return kind === "link" ? "链接" : "文本";
}

const searched = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return clips.value;
  return clips.value.filter((clip) => clip.content.toLowerCase().includes(q));
});

function countOf(id: FilterId) {
  if (id === "all") return searched.value.length;
  if (id === "image" || id === "code") return 0;
  return searched.value.filter((clip) => kindOf(clip.content) === id).length;
}

const filtered = computed(() => {
  if (filter.value === "all") return searched.value;
  if (filter.value === "image" || filter.value === "code") return [];
  return searched.value.filter((clip) => kindOf(clip.content) === filter.value);
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
}

function focusSearch() {
  nextTick(() => searchEl.value?.focus());
}

async function pasteSelected() {
  const clip = current.value;
  if (!clip) return;
  notice.value = "";
  try {
    await pasteClip(clip.id);
  } catch (err) {
    notice.value = err instanceof Error ? err.message : String(err);
  }
}

async function copySelected() {
  const clip = current.value;
  if (!clip) return;
  notice.value = "";
  try {
    await copyClip(clip.id);
    notice.value = "已复制";
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
    void pasteSelected();
  }
}

function onEscape(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  if (!("__TAURI_INTERNALS__" in window)) return;
  event.preventDefault();
  event.stopPropagation();
  void hidePanel();
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
  nextTick(() => {
    document.querySelector(".item.on")?.scrollIntoView({ block: "nearest" });
  });
});

onMounted(async () => {
  window.addEventListener("keydown", onEscape, true);
  if (!("__TAURI_INTERNALS__" in window)) return;
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
  for (const stop of unlisten) stop();
});
</script>

<template>
  <main class="panel" @keydown="onKeydown">
    <header class="top">
      <div class="search" data-tauri-drag-region>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="11" cy="11" r="6.5" />
          <path d="M16 16l4.5 4.5" />
        </svg>
        <input
          ref="searchEl"
          v-model="query"
          type="text"
          placeholder="搜索剪贴板"
          spellcheck="false"
          @input="selected = 0"
        />
        <div class="privacy">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <rect x="6" y="11" width="12" height="9" rx="2.5" />
            <path d="M8.5 11V8a3.5 3.5 0 0 1 7 0v3" />
          </svg>
          仅本机
        </div>
      </div>
      <div class="tabs">
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
            @dblclick="pasteSelected"
          >
            <span class="tile" :class="kindOf(item.clip.content)">
              <svg v-if="kindOf(item.clip.content) === 'link'" viewBox="0 0 24 24">
                <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" />
                <path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
              </svg>
              <svg v-else viewBox="0 0 24 24">
                <path d="M5 7h14M5 12h14M5 17h9" />
              </svg>
            </span>
            <span class="meta">
              <span class="title" :class="{ mono: kindOf(item.clip.content) === 'link' }">{{ firstLine(item.clip.content) }}</span>
              <span class="sub">{{ kindLabel(kindOf(item.clip.content)) }} · {{ item.clip.content.length }} 字</span>
            </span>
            <span class="time">{{ relativeTime(item.clip.createdAt) }}</span>
          </button>
        </template>
      </section>
      <section v-else class="list empty">
        <p v-if="clips.length === 0">还没有记录。复制一段文字后再按 Alt+V。</p>
        <p v-else>没有匹配的记录。</p>
      </section>

      <section v-if="current" class="preview">
        <div class="ph">
          <div class="name">{{ kindLabel(kindOf(current.content)) }}</div>
          <div class="when">{{ relativeTime(current.createdAt) }}</div>
        </div>
        <div class="actions">
          <button type="button" class="btn" @click="copySelected">复制</button>
          <button type="button" class="btn pri" @click="pasteSelected">粘贴</button>
        </div>
        <div class="card" :class="{ mono: kindOf(current.content) === 'link' }">{{ current.content }}</div>
        <div class="facts">
          <span><b>{{ lineCount(current.content) }}</b> 行</span>
          <i></i>
          <span><b>{{ current.content.length }}</b> 字</span>
        </div>
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
        <span><kbd>Esc</kbd> 关闭</span>
      </div>
      <div class="count">Alt+V · {{ clips.length }} 条</div>
    </footer>
  </main>
</template>

<style scoped>
.panel {
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
  padding: 18px 20px 0;
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
  display: grid;
  grid-template-columns: 340px 1fr;
  border-top: 1px solid var(--sep);
}
.list {
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
.card {
  flex: 1;
  min-height: 0;
  overflow: auto;
  white-space: pre-wrap;
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
