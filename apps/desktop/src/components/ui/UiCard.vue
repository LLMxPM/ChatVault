<!-- UiCard：面板容器；info 时在标题旁显示可点击说明 -->
<template>
  <section class="rounded-cv-lg border border-cv-border bg-cv-surface">
    <header v-if="title || $slots.header" class="border-b border-cv-border px-4 py-3">
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
    <div class="p-4" :class="bodyClass">
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
    bodyClass?: string;
  }>(),
  { title: undefined, description: undefined, info: undefined, bodyClass: "" },
);
</script>
