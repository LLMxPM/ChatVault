<!-- UiSwitch：即时开关，无保存按钮 -->
<template>
  <label class="relative inline-flex cursor-pointer items-center" :title="label">
    <input
      class="peer sr-only"
      type="checkbox"
      :checked="modelValue"
      :disabled="disabled"
      @change="onChange"
    />
    <span
      class="h-5 w-9 rounded-full bg-cv-border transition-colors peer-checked:bg-cv-accent peer-disabled:opacity-50"
    />
    <span
      class="absolute left-0.5 h-4 w-4 rounded-full bg-white shadow-sm transition-transform peer-checked:translate-x-4"
    />
  </label>
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    modelValue: boolean;
    disabled?: boolean;
    label?: string;
  }>(),
  { disabled: false, label: "" },
);

const emit = defineEmits<{ "update:modelValue": [boolean] }>();

function onChange(event: Event) {
  emit("update:modelValue", (event.target as HTMLInputElement).checked);
}

void props;
</script>
