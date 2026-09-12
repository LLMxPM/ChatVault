<!-- WebDAV 连接探测结果：结构化展示连通、认证、MKCOL、MOVE 与耗时。 -->
<template>
  <div class="rounded-cv border border-cv-border bg-cv-surface p-3">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="flex items-center gap-2">
        <UiBadge :tone="passed ? 'success' : 'danger'">{{ passed ? "探测通过" : "探测失败" }}</UiBadge>
        <span class="text-cv-caption text-cv-text-3">耗时 {{ capability.durationMs }} ms</span>
      </div>
    </div>
    <p class="mt-2 text-cv-body" :class="passed ? 'text-cv-success' : 'text-cv-danger'">
      {{ capability.message }}
    </p>
    <ul class="mt-3 grid grid-cols-1 gap-1.5 sm:grid-cols-2">
      <li v-for="item in checks" :key="item.key" class="flex items-center gap-2 text-cv-caption">
        <component
          :is="item.ok ? CheckCircle2 : XCircle"
          class="h-3.5 w-3.5 shrink-0"
          :class="item.ok ? 'text-cv-success' : 'text-cv-danger'"
        />
        <span :class="item.ok ? 'text-cv-text-2' : 'text-cv-danger'">{{ item.label }}</span>
      </li>
    </ul>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { CheckCircle2, XCircle } from "lucide-vue-next";
import UiBadge from "./ui/UiBadge.vue";
import type { WebdavCapabilityDto } from "../types";

const props = defineProps<{ capability: WebdavCapabilityDto }>();

/** 探测通过需连通且后续能力全部成功。 */
const passed = computed(
  () =>
    props.capability.reachable &&
    props.capability.authenticated &&
    props.capability.supportMkcol &&
    props.capability.supportMove,
);

const checks = computed(() => [
  { key: "reachable", label: "网络连通", ok: props.capability.reachable },
  { key: "auth", label: "认证与上传回读", ok: props.capability.authenticated },
  { key: "mkcol", label: "创建目录 MKCOL", ok: props.capability.supportMkcol },
  { key: "move", label: "移动重命名 MOVE", ok: props.capability.supportMove },
]);
</script>
