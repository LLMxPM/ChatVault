<!-- UiCard：面板容器，可选标题区 -->
<template>
  <section class="rounded-cv-lg border border-cv-border bg-cv-surface">
    <header v-if="title || $slots.header" class="border-b border-cv-border px-4 py-3">
      <slot name="header">
        <div class="flex items-start justify-between gap-3">
          <div>
            <h3 class="text-cv-section text-cv-text">{{ title }}</h3>
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
withDefaults(
  defineProps<{
    title?: string;
    description?: string;
    bodyClass?: string;
  }>(),
  { title: undefined, description: undefined, bodyClass: "" },
);
</script>
