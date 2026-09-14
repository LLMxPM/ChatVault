<!-- UiCard：面板容器；header 固定，body 可 flex-1 内部滚动；info 在标题旁显示说明 -->
<template>
  <section
    class="flex min-h-0 flex-col overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface"
  >
    <header v-if="title || $slots.header" class="shrink-0 border-b border-cv-border px-4 py-3">
      <slot name="header">
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="flex items-center gap-1.5">
              <h3 class="text-cv-section text-cv-text">{{ title }}</h3>
              <UiInfoTip v-if="info" :text="info" />
            </div>
            <p v-if="description" class="mt-1 text-cv-caption text-cv-text-2">{{ description }}</p>
          </div>
          <slot name="headerExtra" />
        </div>
      </slot>
    </header>
    <div class="flex min-h-0 flex-1 flex-col p-4" :class="bodyClass">
      <slot />
    </div>
  </section>
</template>

<script setup lang="ts">
import UiInfoTip from "./UiInfoTip.vue";

withDefaults(
  defineProps<{
    title?: string;
    description?: string;
    info?: string;
    /** 覆盖默认 body 类；需要内部滚动时传 overflow-y-auto 等 */
    bodyClass?: string;
  }>(),
  { title: undefined, description: undefined, info: undefined, bodyClass: "" },
);
</script>
