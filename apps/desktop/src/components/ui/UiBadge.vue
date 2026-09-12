<!-- UiBadge：状态徽章 -->
<template>
  <span
    class="inline-flex items-center rounded-cv px-1.5 py-0.5 text-cv-caption font-medium"
    :class="toneClass"
  >
    <slot />
  </span>
</template>

<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{ tone?: "neutral" | "accent" | "success" | "warning" | "danger" }>(),
  { tone: "neutral" },
);

const toneClass = computed(() => {
  switch (props.tone) {
    case "accent":
      return "bg-cv-accent-soft text-cv-accent";
    case "success":
      return "bg-cv-accent-soft text-cv-success";
    case "warning":
      return "bg-cv-surface-2 text-cv-warning";
    case "danger":
      return "bg-cv-surface-2 text-cv-danger";
    default:
      return "bg-cv-surface-2 text-cv-text-2";
  }
});
</script>
