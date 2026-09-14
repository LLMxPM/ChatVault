<!-- CollectSourceCard：单条采集源卡片；展示元数据、视频开关与账号勾选 -->
<template>
  <div class="rounded-cv border border-cv-border">
    <div class="flex items-start justify-between gap-3 px-3 py-2.5">
      <div class="min-w-0 flex-1">
        <div class="flex flex-wrap items-center gap-1.5">
          <UiBadge :tone="sourceBadgeTone">
            {{ sourceBadgeLabel }}
          </UiBadge>
          <UiBadge :tone="sourceStatusTone">
            {{ sourceStatusLabel }}
          </UiBadge>
          <span
            v-if="isAccountSource && source.accounts.length"
            class="text-cv-caption text-cv-text-2"
          >
            账号 {{ selectedCount }}/{{ source.accounts.length }}
          </span>
        </div>
        <p
          class="mt-1 truncate font-mono text-cv-caption text-cv-text"
          :title="displayPath(source.path)"
        >
          {{ displayPath(source.path) }}
        </p>
        <p v-if="statusDetail" class="mt-0.5 text-cv-caption text-cv-text-3">
          {{ statusDetail }}
        </p>
      </div>
      <div class="flex shrink-0 items-center gap-1">
        <UiButton size="sm" variant="ghost" :loading="source.inspecting" @click="onPrimaryAction">
          {{ refreshLabel }}
        </UiButton>
        <UiButton size="sm" variant="ghost" @click="emit('remove')">移除</UiButton>
      </div>
    </div>

    <div
      v-if="showVideos"
      class="flex flex-wrap items-center gap-2 border-t border-cv-border px-3 py-2"
    >
      <label class="inline-flex cursor-pointer items-center gap-2 text-cv-caption text-cv-text-2">
        <input
          type="checkbox"
          class="h-4 w-4 accent-[var(--cv-accent)]"
          :checked="source.enableVideos !== false"
          @change="emit('toggle-videos')"
        />
        <span>识别视频</span>
      </label>
      <span v-if="videoHint" class="text-cv-caption text-cv-text-3">{{ videoHint }}</span>
    </div>

    <div v-if="showAccounts" class="border-t border-cv-border px-3 py-2">
      <div class="mb-1 flex items-center justify-between gap-2">
        <span class="text-cv-caption text-cv-text-2">参与扫描的账号</span>
        <div class="flex gap-1">
          <UiButton size="sm" variant="ghost" @click="emit('select-all')">全选</UiButton>
          <UiButton size="sm" variant="ghost" @click="emit('select-none')">清空</UiButton>
        </div>
      </div>
      <div class="max-h-36 overflow-y-auto sm:max-h-40">
        <div class="grid gap-1 sm:grid-cols-2">
          <label
            v-for="account in source.accounts"
            :key="accountKey(account)"
            class="flex cursor-pointer items-start gap-2 rounded-cv px-1.5 py-1 hover:bg-cv-surface-2"
          >
            <input
              type="checkbox"
              class="mt-0.5 accent-[var(--cv-accent)]"
              :checked="isAccountSelected(account)"
              @change="emit('toggle-account', account)"
            />
            <span class="min-w-0">
              <span class="block truncate font-mono text-cv-caption text-cv-text">
                {{ account.sourceAccountId }}
              </span>
              <span class="block text-cv-caption text-cv-text-3">
                附件 {{ account.filesCountEstimated }} · 视频 {{ account.videosCountEstimated ?? 0 }}
              </span>
            </span>
          </label>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import UiBadge from "./ui/UiBadge.vue";
import UiButton from "./ui/UiButton.vue";
import { displayPath } from "../utils/format";
import type { WechatAccountDto } from "../types";
import type { CollectSourceItem } from "../composables/useCollectSources";

const props = defineProps<{
  source: CollectSourceItem;
  isAccountSource: boolean;
  showVideos: boolean;
  videoHint?: string;
  statusDetail: string;
  refreshLabel: string;
  sourceBadgeLabel: string;
  sourceBadgeTone: "accent" | "neutral";
  sourceStatusLabel: string;
  sourceStatusTone: "success" | "warning" | "danger" | "neutral";
  selectedCount: number;
  accountKey: (account: WechatAccountDto) => string;
  isAccountSelected: (account: WechatAccountDto) => boolean;
}>();

const emit = defineEmits<{
  refresh: [];
  replace: [];
  remove: [];
  "toggle-videos": [];
  "toggle-account": [WechatAccountDto];
  "select-all": [];
  "select-none": [];
}>();

const showAccounts = computed(
  () => props.isAccountSource && props.source.accounts.length > 0,
);

/** 目录缺失时改选路径，否则重新识别/检查。 */
function onPrimaryAction() {
  if (props.source.status === "missing") emit("replace");
  else emit("refresh");
}
</script>
