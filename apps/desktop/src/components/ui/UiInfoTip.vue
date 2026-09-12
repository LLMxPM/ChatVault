<!-- UiInfoTip：标题旁信息图标；气泡 Teleport 到 body，避免被容器裁切 -->
<template>
  <button
    ref="trigger"
    type="button"
    class="rounded-full text-cv-text-3 transition-colors hover:text-cv-text-2"
    aria-label="说明"
    :aria-expanded="open"
    @click.stop="toggle"
  >
    <Info class="h-3.5 w-3.5" />
  </button>

  <Teleport to="body">
    <div
      v-if="open"
      ref="panel"
      class="cv-info-tip fixed z-[100] w-64 rounded-cv border border-cv-border bg-cv-surface px-3 py-2 text-cv-caption leading-relaxed text-cv-text-2 shadow-lg"
      :style="panelStyle"
      @click.stop
    >
      {{ text }}
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { Info } from "lucide-vue-next";

defineProps<{ text: string }>();

const trigger = ref<HTMLElement | null>(null);
const panel = ref<HTMLElement | null>(null);
const open = ref(false);
const panelStyle = ref<{ top: string; left: string }>({ top: "0px", left: "0px" });

function clamp(n: number, min: number, max: number) {
  return Math.min(Math.max(n, min), max);
}

/** 依据触发器位置摆放气泡；靠近边缘时夹回视口。 */
async function place() {
  await nextTick();
  const el = trigger.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  const width = 256;
  const estimatedHeight = panel.value?.offsetHeight || 72;
  const gap = 8;
  const margin = 8;

  let top = rect.bottom + gap;
  if (top + estimatedHeight > window.innerHeight - margin) {
    top = rect.top - estimatedHeight - gap;
  }
  if (top < margin) top = margin;

  let left = rect.left + rect.width / 2 - width / 2;
  left = clamp(left, margin, window.innerWidth - width - margin);

  panelStyle.value = { top: `${Math.round(top)}px`, left: `${Math.round(left)}px` };
}

function toggle() {
  open.value = !open.value;
  if (open.value) void place();
}

function close() {
  if (!open.value) return;
  open.value = false;
}

function onPointerDown(event: Event) {
  if (!open.value) return;
  const target = event.target as Node | null;
  if (!target) return;
  if (trigger.value?.contains(target)) return;
  if (panel.value?.contains(target)) return;
  close();
}

function onKey(event: KeyboardEvent) {
  if (event.key === "Escape") close();
}

function onViewportChange() {
  if (open.value) void place();
}

onMounted(() => {
  document.addEventListener("pointerdown", onPointerDown, true);
  document.addEventListener("keydown", onKey);
  window.addEventListener("resize", onViewportChange);
  window.addEventListener("scroll", onViewportChange, true);
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onPointerDown, true);
  document.removeEventListener("keydown", onKey);
  window.removeEventListener("resize", onViewportChange);
  window.removeEventListener("scroll", onViewportChange, true);
});
</script>
