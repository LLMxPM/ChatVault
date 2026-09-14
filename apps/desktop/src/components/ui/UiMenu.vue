<!-- UiMenu：轻量下拉菜单；触发按钮 + 点击外部关闭 -->
<template>
  <div ref="rootEl" class="relative inline-block text-left">
    <slot name="trigger" :open="open" :toggle="toggle" />
    <div
      v-if="open"
      class="absolute right-0 z-30 mt-1 min-w-[11rem] overflow-hidden rounded-cv border border-cv-border bg-cv-surface py-1 shadow-lg"
      :class="menuClass"
    >
      <slot :close="close" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

withDefaults(
  defineProps<{
    menuClass?: string;
  }>(),
  { menuClass: "" },
);

const emit = defineEmits<{ open: [boolean] }>();

const open = ref(false);
const rootEl = ref<HTMLElement | null>(null);

function toggle() {
  open.value = !open.value;
  emit("open", open.value);
}

function close() {
  if (!open.value) return;
  open.value = false;
  emit("open", false);
}

function onDocPointerDown(event: PointerEvent) {
  if (!open.value || !rootEl.value) return;
  if (!rootEl.value.contains(event.target as Node)) close();
}

onMounted(() => document.addEventListener("pointerdown", onDocPointerDown, true));
onBeforeUnmount(() => document.removeEventListener("pointerdown", onDocPointerDown, true));
</script>
