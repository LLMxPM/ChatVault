<!-- UiButton：统一主次按钮样式，支持 loading / danger / ghost -->
<template>
  <button
    class="inline-flex items-center justify-center gap-1.5 rounded-cv font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cv-accent/40"
    :class="[sizeClass, variantClass]"
    :disabled="disabled || loading"
    :type="type"
    @click="$emit('click', $event)"
  >
    <Loader2 v-if="loading" class="animate-spin" :class="iconClass" />
    <slot v-else name="icon" />
    <slot />
  </button>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { Loader2 } from "lucide-vue-next";

const props = withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "sm" | "md";
    loading?: boolean;
    disabled?: boolean;
    type?: "button" | "submit" | "reset";
    block?: boolean;
  }>(),
  {
    variant: "secondary",
    size: "md",
    loading: false,
    disabled: false,
    type: "button",
    block: false,
  },
);

defineEmits<{ click: [MouseEvent] }>();

const sizeClass = computed(() => {
  if (props.size === "sm") return "px-2.5 py-1 text-[12px]";
  return "px-3.5 py-2 text-cv-body";
});

const iconClass = computed(() => (props.size === "sm" ? "h-3.5 w-3.5" : "h-4 w-4"));

const variantClass = computed(() => {
  const base = props.block ? "w-full" : "";
  switch (props.variant) {
    case "primary":
      return `${base} bg-cv-accent text-cv-accent-fg hover:opacity-90`;
    case "danger":
      return `${base} bg-cv-danger text-white hover:opacity-90`;
    case "ghost":
      return `${base} text-cv-text-2 hover:bg-cv-surface-2 hover:text-cv-text`;
    default:
      return `${base} border border-cv-border bg-cv-surface text-cv-text hover:bg-cv-surface-2`;
  }
});
</script>
