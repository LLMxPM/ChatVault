<!-- UiTextarea：统一多行输入，支持 v-model -->
<template>
  <textarea
    :value="modelValue"
    class="w-full rounded-cv border border-cv-border bg-cv-surface px-3 py-2 text-cv-body text-cv-text placeholder:text-cv-text-3 focus:border-cv-accent focus:outline-none focus:ring-2 focus:ring-cv-accent/30 disabled:opacity-60"
    :disabled="disabled"
    :rows="rows"
    @input="onInput"
  />
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{ modelValue?: string; disabled?: boolean; rows?: number }>(),
  { modelValue: "", disabled: false, rows: 3 },
);

const emit = defineEmits<{ "update:modelValue": [string] }>();

/** 将原生 textarea 值回传给 v-model。 */
function onInput(event: Event): void {
  emit("update:modelValue", (event.target as HTMLTextAreaElement).value);
}

void props;
</script>
