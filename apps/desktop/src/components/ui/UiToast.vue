<!-- UiToast：全局轻提示容器 -->
<template>
  <div class="pointer-events-none fixed bottom-4 right-4 z-50 flex w-80 flex-col gap-2">
    <div
      v-for="item in toasts"
      :key="item.id"
      class="pointer-events-auto rounded-cv-lg border border-cv-border bg-cv-surface px-3 py-2.5 shadow-lg"
    >
      <div class="flex items-start gap-2">
        <div class="min-w-0 flex-1">
          <p class="text-cv-body font-medium" :class="titleClass(item.tone)">{{ item.title }}</p>
          <p v-if="item.description" class="mt-0.5 text-cv-caption text-cv-text-2">
            {{ item.description }}
          </p>
        </div>
        <button
          class="shrink-0 rounded-cv p-0.5 text-cv-text-3 hover:bg-cv-surface-2 hover:text-cv-text"
          aria-label="关闭"
          @click="dismissToast(item.id)"
        >
          <X class="h-3.5 w-3.5" />
        </button>
      </div>
      <div v-if="item.actions && item.actions.length > 0" class="mt-2 flex flex-wrap items-center gap-3">
        <button
          v-for="(action, index) in item.actions"
          :key="index"
          class="text-cv-caption font-medium text-cv-accent hover:underline"
          @click="
            action.onClick();
            dismissToast(item.id);
          "
        >
          {{ action.label }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { X } from "lucide-vue-next";
import { useToast, type ToastTone } from "../../composables/useToast";

const { toasts, dismissToast } = useToast();

/** 按语气设置标题颜色。 */
function titleClass(tone: ToastTone): string {
  if (tone === "success") return "text-cv-success";
  if (tone === "warning") return "text-cv-warning";
  if (tone === "danger") return "text-cv-danger";
  return "text-cv-text";
}
</script>
