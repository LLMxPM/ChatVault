<!-- UiSelect：统一下拉框样式，支持 v-model -->
<template>
  <select
    :value="modelValue"
    class="w-full rounded-cv border border-cv-border bg-cv-surface px-3 py-2 text-cv-body text-cv-text focus:border-cv-accent focus:outline-none focus:ring-2 focus:ring-cv-accent/30 disabled:opacity-60"
    :disabled="disabled"
    @change="onChange"
  >
    <slot />
  </select>
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{ modelValue?: string | number; disabled?: boolean }>(),
  { modelValue: "", disabled: false },
);

const emit = defineEmits<{ "update:modelValue": [string] }>();

/** 将原生 select 值回传给 v-model。 */
function onChange(event: Event): void {
  emit("update:modelValue", (event.target as HTMLSelectElement).value);
}

void props;
</script>
