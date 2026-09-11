<!-- ChatVault 副本与缓存设置：配置复制阈值、归档后保留期和容量目标。 -->
<template>
  <div class="bg-slate-900/70 border border-slate-800 rounded-xl p-5 space-y-4 max-w-2xl">
    <h3 class="text-sm font-semibold text-slate-200">副本与缓存</h3>
    <div class="grid grid-cols-3 gap-3 text-xs">
      <label class="text-slate-400">
        复制阈值（MiB）
        <input v-model.number="settings.copyThresholdMib" type="number" min="0" max="4294967295" step="1" class="mt-1 w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200" />
      </label>
      <label class="text-slate-400">
        归档后保留（天）
        <input v-model.number="settings.cacheRetentionDays" type="number" min="0" max="4294967295" step="1" class="mt-1 w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200" />
      </label>
      <label class="text-slate-400">
        缓存容量目标（MiB）
        <input v-model.number="settings.cacheMaxMib" type="number" min="0" max="4294967295" step="1" class="mt-1 w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200" />
      </label>
    </div>
    <p class="text-xs text-slate-400 leading-relaxed">
      仅小于阈值的文件在入库时复制，等于或超过阈值直接读取原文件上传；0 表示不复制。
      未复制的原文件在上传成功前需要保留，删除或修改会导致上传失败。阈值调整适用于后续入库。
    </p>
    <p class="text-xs text-slate-500 leading-relaxed">
      仅回收文件及元数据均已归档同步的副本；过期或超量时优先回收较早归档的缓存。
      保留天数或容量设为 0 表示同步后立即回收。待上传、失败和暂停任务会保留副本，可能暂时超出容量目标。
      保存设置、执行归档及发布同步元数据时自动检查，不删除原文件。1 MiB = 1024 × 1024 字节。
    </p>
  </div>
</template>

<script setup lang="ts">
import type { AppSettingsDto } from "../types";

const settings = defineModel<AppSettingsDto>({ required: true });
</script>
