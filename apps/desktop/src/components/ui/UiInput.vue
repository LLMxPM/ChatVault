<!-- UiInput：统一输入框，支持 v-model 与 type -->
<template>
  <input
    :id="id"
    :value="modelValue"
    :type="type"
    class="w-full max-w-full rounded-cv border bg-cv-surface px-3 py-2 text-cv-body text-cv-text placeholder:text-cv-text-3 focus:outline-none focus:ring-2 focus:ring-cv-accent/30 disabled:opacity-60"
    :class="invalid ? 'border-cv-danger' : 'border-cv-border focus:border-cv-accent'"
    :disabled="disabled"
    :min="min"
    :max="max"
    :maxlength="maxlength"
    :placeholder="placeholder"
    @input="onInput"
  />
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    modelValue?: string | number;
    type?: string;
    invalid?: boolean;
    disabled?: boolean;
    min?: number | string;
    max?: number | string;
    maxlength?: number | string;
    id?: string;
    placeholder?: string;
  }>(),
  {
    modelValue: "",
    type: "text",
    invalid: false,
    disabled: false,
    min: undefined,
    max: undefined,
    maxlength: undefined,
    id: undefined,
    placeholder: undefined,
  },
);

const emit = defineEmits<{ "update:modelValue": [string] }>();

/** 将原生 input 值回传给 v-model。 */
function onInput(event: Event): void {
  emit("update:modelValue", (event.target as HTMLInputElement).value);
}

void props;
</script>
