<script setup lang="ts">
import { hidePanel, minimizePanel } from "../api";
import { useSettingsStore } from "../stores/settings";

const settings = useSettingsStore();

function minimize() {
  void minimizePanel().catch(() => {});
}

function close() {
  void hidePanel().catch(() => {});
}
</script>

<template>
  <span v-if="!settings.settings.autoHide" class="controls">
    <button type="button" aria-label="缩小" @click="minimize">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M5 12h14" />
      </svg>
    </button>
    <button type="button" aria-label="关闭" @click="close">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M7 7l10 10M17 7L7 17" />
      </svg>
    </button>
  </span>
</template>

<style scoped>
.controls {
  display: flex;
  align-items: center;
  gap: 2px;
}
button {
  -webkit-app-region: no-drag;
  app-region: no-drag;
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 0;
  border-radius: var(--r-ctl);
  background: transparent;
  color: var(--text-2);
  cursor: pointer;
}
button:hover {
  background: var(--fill);
  color: var(--text);
}
svg {
  width: 16px;
  height: 16px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}
</style>
