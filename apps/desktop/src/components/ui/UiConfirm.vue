<!-- UiConfirm：破坏性操作二次确认对话框 -->
<template>
  <div
    v-if="confirmState.open"
    class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4"
    @click.self="settleConfirm(false)"
  >
    <div
      class="w-full max-w-md rounded-cv-lg border border-cv-border bg-cv-surface p-5 shadow-xl"
      role="dialog"
      aria-modal="true"
    >
      <h3 class="text-cv-section text-cv-text">{{ confirmState.title }}</h3>
      <p v-if="confirmState.description" class="mt-2 text-cv-body text-cv-text-2">
        {{ confirmState.description }}
      </p>
      <div class="mt-5 flex justify-end gap-2">
        <UiButton variant="secondary" @click="settleConfirm(false)">
          {{ confirmState.cancelLabel || "取消" }}
        </UiButton>
        <UiButton
          :variant="confirmState.danger ? 'danger' : 'primary'"
          @click="settleConfirm(true)"
        >
          {{ confirmState.confirmLabel || "确认" }}
        </UiButton>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useConfirm } from "../../composables/useConfirm";
import UiButton from "./UiButton.vue";

const { confirmState, settleConfirm } = useConfirm();
</script>
