<!-- UiConfirm：破坏性操作二次确认对话框；支持强制输入短语 -->
<template>
  <div
    v-if="confirmState.open"
    class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4"
    @click.self="onCancel"
  >
    <div
      class="w-full max-w-md rounded-cv-lg border border-cv-border bg-cv-surface p-5 shadow-xl"
      role="dialog"
      aria-modal="true"
    >
      <h3 class="text-cv-section" :class="confirmState.danger ? 'text-cv-danger' : 'text-cv-text'">
        {{ confirmState.title }}
      </h3>
      <p
        v-if="confirmState.description"
        class="mt-2 whitespace-pre-line text-cv-body text-cv-text-2"
      >
        {{ confirmState.description }}
      </p>
      <label v-if="confirmState.requirePhrase" class="mt-4 block">
        <span class="text-cv-caption text-cv-text-2">
          {{ confirmState.requirePhraseLabel || `请输入「${confirmState.requirePhrase}」以确认` }}
        </span>
        <UiInput v-model="typedPhrase" class="mt-1 font-mono" :placeholder="confirmState.requirePhrase" />
      </label>
      <div class="mt-5 flex justify-end gap-2">
        <UiButton variant="secondary" @click="onCancel">
          {{ confirmState.cancelLabel || "取消" }}
        </UiButton>
        <UiButton
          :variant="confirmState.danger ? 'danger' : 'primary'"
          :disabled="!canConfirm"
          @click="onConfirm"
        >
          {{ confirmState.confirmLabel || "确认" }}
        </UiButton>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useConfirm } from "../../composables/useConfirm";
import UiButton from "./UiButton.vue";
import UiInput from "./UiInput.vue";

const { confirmState, settleConfirm } = useConfirm();
const typedPhrase = ref("");

const canConfirm = computed(() => {
  const required = confirmState.value.requirePhrase;
  if (!required) return true;
  return typedPhrase.value.trim() === required;
});

watch(
  () => confirmState.value.open,
  (open) => {
    if (open) typedPhrase.value = "";
  },
);

function onCancel() {
  settleConfirm(false);
}

function onConfirm() {
  if (!canConfirm.value) return;
  settleConfirm(true);
}
</script>
