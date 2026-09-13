<!-- ChatVault 副本与缓存设置：复制阈值、归档后保留期与容量目标。 -->
<template>
  <UiCard title="副本与缓存" :info="info">
    <template #headerExtra>
      <span v-if="dirty" class="shrink-0 text-cv-caption text-cv-warning">未保存</span>
    </template>
    <div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
      <label class="block">
        <span class="text-cv-caption text-cv-text-2">复制阈值（MiB）</span>
        <UiInput
          :model-value="settings.copyThresholdMib"
          type="number"
          class="mt-1"
          min="0"
          max="4294967295"
          @update:model-value="onNumber('copyThresholdMib', $event)"
        />
      </label>
      <label class="block">
        <span class="text-cv-caption text-cv-text-2">归档后保留（天）</span>
        <UiInput
          :model-value="settings.cacheRetentionDays"
          type="number"
          class="mt-1"
          min="0"
          max="4294967295"
          @update:model-value="onNumber('cacheRetentionDays', $event)"
        />
      </label>
      <label class="block">
        <span class="text-cv-caption text-cv-text-2">容量目标（MiB）</span>
        <UiInput
          :model-value="settings.cacheMaxMib"
          type="number"
          class="mt-1"
          min="0"
          max="4294967295"
          @update:model-value="onNumber('cacheMaxMib', $event)"
        />
      </label>
    </div>
  </UiCard>
</template>

<script setup lang="ts">
import UiCard from "./ui/UiCard.vue";
import UiInput from "./ui/UiInput.vue";
import type { AppSettingsDto } from "../types";

defineProps<{ dirty?: boolean }>();

const settings = defineModel<AppSettingsDto>({ required: true });

const info =
  "仅小于阈值的文件在入库时复制；等于或超过阈值直接读取原文件上传。未复制的原文件在上传成功前需保留。仅回收已归档同步的副本；保留天数或容量为 0 表示同步后立即回收。";

/** 将数字输入写回设置对象。 */
function onNumber(key: "copyThresholdMib" | "cacheRetentionDays" | "cacheMaxMib", raw: string) {
  const n = Number(raw);
  settings.value = { ...settings.value, [key]: Number.isFinite(n) ? n : 0 };
}
</script>
