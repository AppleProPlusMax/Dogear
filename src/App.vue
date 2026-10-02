<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getEffectState, hidePanel, listClips, pasteClip, type Clip, type EffectState } from "./api";

const clips = ref<Clip[]>([]);
const query = ref("");
const selected = ref(0);
const status = ref("");
const effect = ref<EffectState | null>(null);
const searchEl = ref<HTMLInputElement | null>(null);
let unlisten: UnlistenFn[] = [];

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q) return clips.value;
  return clips.value.filter((clip) => clip.content.toLowerCase().includes(q));
});

async function refresh() {
  clips.value = await listClips();
  if (selected.value >= filtered.value.length) {
    selected.value = 0;
  }
}

async function loadEffect() {
  effect.value = await getEffectState();
  document.documentElement.dataset.effect = effect.value.effect;
}

function focusSearch() {
  nextTick(() => searchEl.value?.focus());
}

async function pasteSelected() {
  const clip = filtered.value[selected.value];
  if (!clip) return;
  status.value = "";
  try {
    await pasteClip(clip.id);
  } catch (err) {
    status.value = err instanceof Error ? err.message : String(err);
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
  } else if (event.key === "Escape") {
    event.preventDefault();
    if (query.value) {
      query.value = "";
      selected.value = 0;
      return;
    }
    void hidePanel();
  }
}

function preview(content: string) {
  const line = content.split(/\r?\n/)[0] ?? "";
  return line.length > 80 ? `${line.slice(0, 80)}…` : line;
}

function relativeTime(createdAt: number) {
  const delta = Math.max(0, Date.now() - createdAt);
  const minutes = Math.floor(delta / 60000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  return `${Math.floor(hours / 24)} 天前`;
}

onMounted(async () => {
  if (!("__TAURI_INTERNALS__" in window)) return;
  await Promise.all([refresh(), loadEffect()]);
  unlisten = [
    await listen("clip-added", () => {
      void refresh();
    }),
    await listen("window-shown", () => {
      query.value = "";
      selected.value = 0;
      status.value = "";
      focusSearch();
    }),
  ];
  focusSearch();
});

onUnmounted(() => {
  for (const stop of unlisten) stop();
});
</script>

<template>
  <main class="panel" @keydown="onKeydown">
    <header class="top" data-tauri-drag-region>
      <input
        ref="searchEl"
        v-model="query"
        class="search"
        type="text"
        placeholder="搜索剪贴板"
        spellcheck="false"
        @input="selected = 0"
      />
    </header>
    <ul v-if="filtered.length" class="list">
      <li
        v-for="(clip, index) in filtered"
        :key="clip.id"
        :class="{ on: index === selected }"
        @click="selected = index"
        @dblclick="pasteSelected"
      >
        <span class="body">{{ preview(clip.content) }}</span>
        <span class="time">{{ relativeTime(clip.createdAt) }}</span>
      </li>
    </ul>
    <p v-else class="empty">还没有记录。复制一段文字，或按 Alt+V 呼出这个面板。</p>
    <p v-if="status" class="status">{{ status }}</p>
    <footer data-tauri-drag-region>
      <span>↑↓ 选择</span>
      <span>Enter 粘贴</span>
      <span>Esc 关闭</span>
      <span class="effect">{{ effect?.effect ?? "…" }} · {{ effect?.detail }}</span>
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
  border-radius: 22px;
  overflow: hidden;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.35);
}
.top {
  padding: 16px 16px 8px;
}
.search {
  width: 100%;
  height: 40px;
  border: 0;
  border-radius: 13px;
  background: var(--fill);
  color: var(--text);
  font: inherit;
  font-size: 16px;
  padding: 0 14px;
  outline: none;
}
.list {
  list-style: none;
  margin: 0;
  padding: 4px 8px;
  overflow: auto;
  flex: 1;
}
.list li {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 44px;
  padding: 8px 10px;
  border-radius: 12px;
  cursor: pointer;
}
.list li.on {
  background: var(--fill);
}
.body {
  flex: 1;
  min-width: 0;
  font-size: 13.5px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.time {
  flex: none;
  font-size: 11.5px;
  opacity: 0.62;
}
.empty,
.status {
  margin: 0;
  padding: 20px 18px;
  font-size: 13px;
  line-height: 1.5;
}
.status {
  padding-top: 0;
}
.empty {
  flex: 1;
}
footer {
  display: flex;
  gap: 12px;
  align-items: center;
  min-height: 40px;
  padding: 0 16px;
  font-size: 11.5px;
  opacity: 0.72;
}
.effect {
  margin-left: auto;
  max-width: 46%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
