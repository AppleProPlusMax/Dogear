<script setup lang="ts">
import { computed, ref } from "vue";
import { useSettingsStore } from "../stores/settings";

const settings = useSettingsStore();
const index = ref(0);

const steps = computed(() => {
  const summon = settings.settings.summonShortcut;
  const capture = settings.settings.captureShortcut;
  return [
    {
      title: "关掉也会留着",
      copy: "缩小收到任务栏，关闭收到托盘。Dogear 还在运行。要退出，右键托盘图标，选退出。",
      keys: [] as string[],
    },
    {
      title: `按 ${summon} 再打开`,
      copy: `点到别的窗口，它会留在任务栏。按 ${summon} 隐藏，再按一次打开。下次启动时，它先停在托盘里。`,
      keys: [summon],
    },
    {
      title: "选中后按回车",
      copy: `复制过的内容会出现在列表里。选中一条，按回车，贴回刚才的窗口。要截图，按 ${capture}。`,
      keys: ["Enter", capture],
    },
  ];
});

const step = computed(() => steps.value[index.value] ?? steps.value[0]);
const last = computed(() => index.value >= steps.value.length - 1);

function parts(shortcut: string) {
  return shortcut.split("+").filter(Boolean);
}

function next() {
  if (last.value) {
    void settings.finishGuide();
    return;
  }
  index.value += 1;
}

function skip() {
  void settings.finishGuide();
}
</script>

<template>
  <div class="scrim">
    <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="guide-title">
      <p class="kicker">{{ index + 1 }} / {{ steps.length }}</p>
      <h2 id="guide-title">{{ step.title }}</h2>
      <p class="copy">{{ step.copy }}</p>
      <div v-if="step.keys.length" class="keys">
        <span v-for="shortcut in step.keys" :key="shortcut" class="chip">
          <template v-for="(part, partIndex) in parts(shortcut)" :key="`${shortcut}-${part}`">
            <span v-if="partIndex > 0" class="plus">+</span>
            <kbd>{{ part }}</kbd>
          </template>
        </span>
      </div>
      <div class="dots" role="tablist" aria-label="教程步骤">
        <button
          v-for="(item, dot) in steps"
          :key="item.title"
          type="button"
          role="tab"
          :aria-selected="dot === index"
          :aria-label="`第 ${dot + 1} 步`"
          @click="index = dot"
        >
          {{ dot + 1 }}
        </button>
      </div>
      <div class="actions">
        <button type="button" @click="skip">跳过</button>
        <button type="button" class="pri" @click="next">{{ last ? "开始使用" : "下一步" }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.scrim {
  position: absolute;
  inset: 0;
  z-index: 2;
  display: flex;
  align-items: flex-end;
  justify-content: center;
  padding: 16px;
  background: var(--scrim);
  -webkit-app-region: no-drag;
  app-region: no-drag;
}
.sheet {
  width: min(100%, 420px);
  padding: 16px 16px 12px;
  border-radius: var(--r-card);
  background: var(--panel-solid);
  border: 1px solid var(--sep);
  color: var(--text);
}
.kicker {
  margin: 0 0 4px;
  font-size: 12px;
  color: var(--accent);
}
h2 {
  margin: 0 0 8px;
  font-size: 20px;
  line-height: 28px;
  font-weight: 600;
  letter-spacing: -0.02em;
}
.copy {
  margin: 0;
  color: var(--text-2);
}
.keys {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 12px;
}
.keys .chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  min-height: 28px;
  padding: 0 8px;
  border-radius: var(--r-ctl);
  background: var(--fill);
}
.plus {
  color: var(--text-3);
}
kbd {
  font-family: inherit;
  font-size: 12px;
}
.dots {
  display: flex;
  gap: 6px;
  margin-top: 14px;
}
.dots button,
.actions button {
  border: 0;
  background: transparent;
  color: var(--text-3);
  font: inherit;
  cursor: pointer;
}
.dots button {
  width: 28px;
  height: 28px;
  border-radius: 999px;
}
.dots button[aria-selected="true"] {
  background: var(--accent-soft);
  color: var(--accent);
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 8px;
}
.actions button {
  min-height: 32px;
  padding: 0 12px;
  border-radius: var(--r-ctl);
  color: var(--text-2);
}
.actions .pri {
  background: var(--accent);
  color: var(--on-accent);
}
</style>
