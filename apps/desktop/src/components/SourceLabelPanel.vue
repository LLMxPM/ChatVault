<!--
  ChatVault 来源标注面板
  职责：账号自定义显示名与收藏；由文件库打开。会话标注暂未启用。
-->
<template>
  <div class="fixed inset-0 z-40 flex justify-end bg-black/30" @click.self="emit('close')">
    <div class="flex h-full w-full max-w-xl flex-col border-l border-cv-border bg-cv-bg shadow-xl">
      <header class="flex items-center justify-between border-b border-cv-border px-4 py-3">
        <div class="flex items-center gap-1.5">
          <h3 class="text-cv-section text-cv-text">来源标注</h3>
          <UiInfoTip text="为来源账号设置显示名与收藏；文件库筛选与列表同步生效。清空名称可恢复为来源 ID。" />
        </div>
        <UiButton size="sm" variant="ghost" @click="emit('close')">关闭</UiButton>
      </header>

      <div
        v-if="error"
        class="mx-4 mt-3 rounded-cv border border-cv-border bg-cv-surface px-3 py-2 text-cv-caption text-cv-danger"
      >
        {{ error }}
      </div>

      <div class="flex min-h-0 flex-1 flex-col overflow-hidden p-4">
        <section
          class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface"
        >
          <header class="flex items-center justify-between border-b border-cv-border px-3 py-2.5">
            <div>
              <h4 class="text-cv-body font-medium text-cv-text">来源账号</h4>
              <p class="text-cv-caption text-cv-text-3">{{ accounts.length }} 个</p>
            </div>
            <UiButton size="sm" variant="ghost" :loading="loading" @click="refresh">刷新</UiButton>
          </header>
          <div class="min-h-0 flex-1 overflow-y-auto">
            <p
              v-if="accounts.length === 0"
              class="p-8 text-center text-cv-caption text-cv-text-3"
            >
              扫描入库后，来源会出现在这里
            </p>
            <div
              v-for="account in accounts"
              :key="keyOfAccount(account)"
              class="border-b border-cv-border/60 px-3 py-3 last:border-0"
            >
              <div class="flex items-start gap-2">
                <button
                  class="mt-1 shrink-0 text-cv-caption"
                  :class="
                    account.isFavorite
                      ? 'text-cv-warning'
                      : 'text-cv-text-3 hover:text-cv-warning'
                  "
                  :title="account.isFavorite ? '取消收藏' : '收藏'"
                  @click="toggleAccountFavorite(account)"
                >
                  <Star class="h-4 w-4" :fill="account.isFavorite ? 'currentColor' : 'none'" />
                </button>
                <div class="min-w-0 flex-1">
                  <div class="flex min-w-0 items-center justify-between gap-2">
                    <span class="truncate text-cv-body font-medium text-cv-text">
                      {{ account.effectiveName }}
                    </span>
                    <span
                      class="shrink-0 rounded-cv bg-cv-surface-2 px-1.5 py-0.5 text-cv-caption text-cv-text-2"
                    >
                      {{ account.recordCount }}
                    </span>
                  </div>
                  <p
                    v-if="account.sourceAccountId !== account.effectiveName"
                    class="mt-0.5 truncate font-mono text-cv-caption text-cv-text-3"
                    :title="account.sourceAccountId"
                  >
                    {{ account.sourceAccountId }}
                  </p>
                  <div class="mt-2 flex items-stretch gap-1.5" @click.stop>
                    <UiInput
                      class="min-w-0 flex-1"
                      :model-value="accountDrafts[keyOfAccount(account)] ?? account.displayName ?? ''"
                      placeholder="自定义显示名称"
                      @update:model-value="
                        (v) => (accountDrafts[keyOfAccount(account)] = String(v))
                      "
                      @keyup.enter="saveAccount(account)"
                    />
                    <UiButton
                      size="sm"
                      variant="primary"
                      class="shrink-0 whitespace-nowrap"
                      @click="saveAccount(account)"
                    >
                      保存
                    </UiButton>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { Star } from "lucide-vue-next";
import UiButton from "./ui/UiButton.vue";
import UiInput from "./ui/UiInput.vue";
import UiInfoTip from "./ui/UiInfoTip.vue";
import { listSourceAccounts, updateSourceAccount } from "../api/tauri";
import { pushToast } from "../composables/useToast";
import type { SourceAccountDto } from "../types";

const emit = defineEmits<{ close: []; changed: [] }>();

const accounts = ref<SourceAccountDto[]>([]);
const accountDrafts = ref<Record<string, string>>({});
const loading = ref(false);
const error = ref("");

function keyOfAccount(a: SourceAccountDto) {
  return a.sourceType + "|" + a.sourceAccountId;
}

async function refresh() {
  loading.value = true;
  error.value = "";
  try {
    accounts.value = await listSourceAccounts();
    for (const account of accounts.value) {
      accountDrafts.value[keyOfAccount(account)] = account.displayName || "";
    }
  } catch (err) {
    error.value = "读取来源映射失败：" + err;
  } finally {
    loading.value = false;
  }
}

async function saveAccount(account: SourceAccountDto) {
  try {
    const value = accountDrafts.value[keyOfAccount(account)]?.trim() || null;
    await updateSourceAccount({
      sourceType: account.sourceType,
      sourceAccountId: account.sourceAccountId,
      displayName: value,
      isFavorite: account.isFavorite,
    });
    pushToast({ tone: "success", title: "账号名称已保存" });
    emit("changed");
    await refresh();
  } catch (err) {
    pushToast({ tone: "danger", title: "保存失败", description: String(err) });
  }
}

async function toggleAccountFavorite(account: SourceAccountDto) {
  try {
    await updateSourceAccount({
      sourceType: account.sourceType,
      sourceAccountId: account.sourceAccountId,
      displayName: account.displayName || null,
      isFavorite: !account.isFavorite,
    });
    emit("changed");
    await refresh();
  } catch (err) {
    pushToast({ tone: "danger", title: "更新收藏失败", description: String(err) });
  }
}

onMounted(refresh);
</script>
