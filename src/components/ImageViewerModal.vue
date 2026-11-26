<script setup lang="ts">
import { computed, ref, watch } from "vue";

const props = defineProps<{
  open: boolean;
  src: string;
  alt: string;
  label: string;
  closeLabel: string;
}>();

const emit = defineEmits<{
  close: [event?: Event];
}>();

const frameEl = ref<HTMLElement | null>(null);
const viewScale = ref(1);
const viewX = ref(0);
const viewY = ref(0);
const viewScaleMin = 0.25;
const viewScaleMax = 8;
let viewDrag: { id: number; x: number; y: number; originX: number; originY: number; moved: boolean } | null = null;
let ignoreViewerClick = false;

const viewerTransform = computed(() => ({
  transform: `translate(${viewX.value}px, ${viewY.value}px) scale(${viewScale.value})`,
}));

function resetViewer() {
  viewScale.value = 1;
  viewX.value = 0;
  viewY.value = 0;
  viewDrag = null;
  ignoreViewerClick = false;
}

function clampOffset(x: number, y: number, scale: number) {
  const frame = frameEl.value;
  if (!frame) return { x, y };
  const maxX = Math.max(frame.clientWidth * 0.45, (frame.clientWidth * scale) / 2);
  const maxY = Math.max(frame.clientHeight * 0.45, (frame.clientHeight * scale) / 2);
  return {
    x: Math.min(maxX, Math.max(-maxX, x)),
    y: Math.min(maxY, Math.max(-maxY, y)),
  };
}

watch(
  () => [props.open, props.src] as const,
  ([isOpen]) => {
    if (isOpen) resetViewer();
  },
);

function onViewerBackdrop(event: MouseEvent) {
  if (ignoreViewerClick) {
    ignoreViewerClick = false;
    return;
  }
  emit("close", event);
}

function onViewerWheel(event: WheelEvent) {
  const frame = frameEl.value;
  if (!frame) return;
  const rect = frame.getBoundingClientRect();
  const ox = event.clientX - (rect.left + rect.width / 2);
  const oy = event.clientY - (rect.top + rect.height / 2);
  const prev = viewScale.value;
  let delta = event.deltaY;
  if (event.deltaMode === 1) delta *= 16;
  if (event.deltaMode === 2) delta *= rect.height;
  const next = Math.min(viewScaleMax, Math.max(viewScaleMin, prev * Math.exp(-delta * 0.0015)));
  if (next === prev) return;
  const lx = (ox - viewX.value) / prev;
  const ly = (oy - viewY.value) / prev;
  const clamped = clampOffset(ox - lx * next, oy - ly * next, next);
  viewScale.value = next;
  viewX.value = clamped.x;
  viewY.value = clamped.y;
}

function onViewerPointerDown(event: PointerEvent) {
  if (event.button !== 0) return;
  const target = event.currentTarget;
  if (!(target instanceof HTMLElement)) return;
  target.setPointerCapture(event.pointerId);
  viewDrag = {
    id: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    originX: viewX.value,
    originY: viewY.value,
    moved: false,
  };
  event.preventDefault();
}

function onViewerPointerMove(event: PointerEvent) {
  if (!viewDrag || event.pointerId !== viewDrag.id) return;
  const dx = event.clientX - viewDrag.x;
  const dy = event.clientY - viewDrag.y;
  if (dx * dx + dy * dy > 9) viewDrag.moved = true;
  const clamped = clampOffset(viewDrag.originX + dx, viewDrag.originY + dy, viewScale.value);
  viewX.value = clamped.x;
  viewY.value = clamped.y;
}

function onViewerPointerUp(event: PointerEvent) {
  if (!viewDrag || event.pointerId !== viewDrag.id) return;
  const moved = viewDrag.moved;
  viewDrag = null;
  if (!moved) return;
  ignoreViewerClick = true;
  window.setTimeout(() => {
    ignoreViewerClick = false;
  }, 0);
}
</script>

<template>
  <Transition name="viewer">
    <div
      v-if="open && src"
      ref="frameEl"
      class="viewer"
      role="dialog"
      aria-modal="true"
      :aria-label="label"
      @click="onViewerBackdrop"
      @wheel.prevent="onViewerWheel"
    >
      <img
        :src="src"
        :alt="alt"
        :style="viewerTransform"
        draggable="false"
        @pointerdown="onViewerPointerDown"
        @pointermove="onViewerPointerMove"
        @pointerup="onViewerPointerUp"
        @pointercancel="onViewerPointerUp"
        @dblclick.stop="resetViewer"
        @click.stop
      />
      <button type="button" class="viewer-x" :aria-label="closeLabel" @click.stop="emit('close', $event)">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M7 7l10 10M17 7L7 17" />
        </svg>
      </button>
    </div>
  </Transition>
</template>

<style scoped>
.viewer {
  position: absolute;
  inset: 0;
  z-index: 3;
  display: grid;
  place-items: center;
  overflow: hidden;
  padding: 16px;
  background: var(--scrim);
  -webkit-app-region: no-drag;
  app-region: no-drag;
}
.viewer img {
  position: relative;
  z-index: 0;
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  transform-origin: center center;
  cursor: grab;
  touch-action: none;
  user-select: none;
}
.viewer img:active {
  cursor: grabbing;
}
.viewer-x {
  position: absolute;
  right: 16px;
  bottom: 16px;
  z-index: 2;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--card-border);
  border-radius: var(--r-ctl);
  background: var(--panel-solid);
  color: var(--text);
  cursor: pointer;
}
.viewer-x:hover {
  background: var(--fill);
}
.viewer-x svg {
  width: 16px;
  height: 16px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.8;
  stroke-linecap: round;
}
.viewer-enter-active,
.viewer-leave-active {
  transition: opacity 160ms var(--ease);
}
.viewer-enter-from,
.viewer-leave-to {
  opacity: 0;
}
</style>
