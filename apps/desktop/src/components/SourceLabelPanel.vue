<!--
  ChatVault 来源标注面板
  职责：账号/聊天自定义显示名与收藏；由文件库打开。
-->
<template>
  <div class="fixed inset-0 z-40 flex justify-end bg-black/30" @click.self="emit('close')">
    <div class="flex h-full w-full max-w-2xl flex-col border-l border-cv-border bg-cv-bg shadow-xl">
      <header class="flex items-center justify-between border-b border-cv-border px-4 py-3">
        <div>
          <div class="flex items-center gap-1.5">
            <h3 class="text-cv-section text-cv-text">来源标注</h3>
            <UiInfoTip text="为账号与聊天设置显示名与收藏；文件库筛选与列表同步生效。清空名称可恢复为来源 ID。" />
          </div>
        </div>
        <UiButton size="sm" variant="ghost" @click="emit('close')">关闭</UiButton>
      </header>

      <div v-if="error" class="mx-4 mt-3 rounded-cv border border-cv-border bg-cv-surface px-3 py-2 text-cv-caption text-cv-danger">
        {{ error }}
      </div>

      <div class="grid min-h-0 flex-1 grid-cols-1 gap-3 overflow-hidden p-4 md:grid-cols-2">
        <section class="flex min-h-0 flex-col overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface">
          <header class="flex items-center justify-between border-b border-cv-border px-3 py-2.5">
            <div>
              <h4 class="text-cv-body font-medium text-cv-text">来源账号</h4>
              <p class="text-cv-caption text-cv-text-3">{{ accounts.length }} 个</p>
            </div>
            <UiButton size="sm" variant="ghost" :loading="loading" @click="refresh">刷新</UiButton>
          </header>
          <div class="min-h-0 flex-1 overflow-y-auto">
            <p v-if="accounts.length === 0" class="p-6 text-center text-cv-caption text-cv-text-3">扫描入库后，来源会出现在这里</p>
            <div
              v-for="account in accounts"
              :key="keyOfAccount(account)"
              class="cursor-pointer border-b border-cv-border/60 px-3 py-2.5 last:border-0"
              :class="selectedKey === keyOfAccount(account) ? 'bg-cv-accent-soft' : 'hover:bg-cv-surface-2'"
              @click="selectAccount(account)"
            >
              <div class="flex items-start gap-2">
                <button
                  class="mt-0.5 text-cv-caption"
                  :class="account.isFavorite ? 'text-cv-warning' : 'text-cv-text-3 hover:text-cv-warning'"
                  @click.stop="toggleAccountFavorite(account)"
                >
                  <Star class="h-3.5 w-3.5" :fill="account.isFavorite ? 'currentColor' : 'none'" />
                </button>
                <div class="min-w-0 flex-1">
                  <div class="flex items-center justify-between gap-2">
                    <span class="truncate text-cv-caption font-medium text-cv-text">{{ account.effectiveName }}</span>
                    <span class="shrink-0 text-cv-caption text-cv-text-3">{{ account.recordCount }}</span>
                  </div>
                  <p class="mt-0.5 truncate text-cv-caption text-cv-text-3">{{ account.sourceAccountId }}</p>
                  <div class="mt-1.5 flex gap-1.5" @click.stop>
                    <UiInput
                      :model-value="accountDrafts[keyOfAccount(account)] ?? account.displayName ?? ''"
                      placeholder="自定义显示名称"
                      @update:model-value="(v) => (accountDrafts[keyOfAccount(account)] = String(v))"
                      @keyup.enter="saveAccount(account)"
                    />
                    <UiButton size="sm" variant="primary" @click="saveAccount(account)">保存</UiButton>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section class="flex min-h-0 flex-col overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface">
          <header class="border-b border-cv-border px-3 py-2.5">
            <h4 class="text-cv-body font-medium text-cv-text">来源聊天</h4>
            <p class="text-cv-caption text-cv-text-3">{{ selected ? selected.effectiveName : "选择左侧账号" }}</p>
          </header>
          <div class="min-h-0 flex-1 overflow-y-auto">
            <p v-if="!selected" class="p-8 text-center text-cv-caption text-cv-text-3">请选择一个来源账号</p>
            <p v-else-if="conversations.length === 0" class="p-8 text-center text-cv-caption text-cv-text-3">当前账号没有可标注的聊天</p>
            <div v-for="c in conversations" :key="keyOfConversation(c)" class="border-b border-cv-border/60 px-3 py-2.5 last:border-0">
              <div class="flex items-start gap-2">
                <button
                  class="mt-0.5 text-cv-caption"
                  :class="c.isFavorite ? 'text-cv-warning' : 'text-cv-text-3 hover:text-cv-warning'"
                  @click="toggleConversationFavorite(c)"
                >
                  <Star class="h-3.5 w-3.5" :fill="c.isFavorite ? 'currentColor' : 'none'" />
                </button>
                <div class="min-w-0 flex-1">
                  <div class="flex items-center justify-between gap-2">
                    <span class="truncate text-cv-caption font-medium text-cv-text">{{ c.effectiveName }}</span>
                    <span class="shrink-0 text-cv-caption text-cv-text-3">{{ c.recordCount }}</span>
                  </div>
                  <p class="mt-0.5 truncate font-mono text-cv-caption text-cv-text-3">{{ c.sourceConversationId }}</p>
                  <div class="mt-1.5 flex gap-1.5">
                    <UiInput
                      :model-value="conversationDrafts[keyOfConversation(c)] ?? c.displayName ?? ''"
                      placeholder="自定义显示名称"
                      @update:model-value="(v) => (conversationDrafts[keyOfConversation(c)] = String(v))"
                      @keyup.enter="saveConversation(c)"
                    />
                    <UiButton size="sm" variant="primary" @click="saveConversation(c)">保存</UiButton>
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
import {
  listSourceAccounts,
  listSourceConversations,
  updateSourceAccount,
  updateSourceConversation,
} from "../api/tauri";
import { pushToast } from "../composables/useToast";
import type { SourceAccountDto, SourceConversationDto } from "../types";

const emit = defineEmits<{ close: []; changed: [] }>();

const accounts = ref<SourceAccountDto[]>([]);
const conversations = ref<SourceConversationDto[]>([]);
const selected = ref<SourceAccountDto | null>(null);
const selectedKey = ref("");
const accountDrafts = ref<Record<string, string>>({});
const conversationDrafts = ref<Record<string, string>>({});
const loading = ref(false);
const error = ref("");

function keyOfAccount(a: SourceAccountDto) {
  return a.sourceType + "|" + a.sourceAccountId;
}

function keyOfConversation(c: SourceConversationDto) {
  return c.sourceType + "|" + c.sourceAccountId + "|" + c.sourceConversationId;
}

async function refresh() {
  loading.value = true;
  error.value = "";
  try {
    accounts.value = await listSourceAccounts();
    for (const account of accounts.value) {
      accountDrafts.value[keyOfAccount(account)] = account.displayName || "";
    }
    if (selectedKey.value) {
      const next = accounts.value.find((item) => keyOfAccount(item) === selectedKey.value);
      if (next) await selectAccount(next);
      else {
        selected.value = null;
        conversations.value = [];
      }
    }
  } catch (err) {
    error.value = "读取来源映射失败：" + err;
  } finally {
    loading.value = false;
  }
}

async function selectAccount(account: SourceAccountDto) {
  selected.value = account;
  selectedKey.value = keyOfAccount(account);
  try {
    conversations.value = await listSourceConversations(account.sourceType, account.sourceAccountId);
    for (const conversation of conversations.value) {
      conversationDrafts.value[keyOfConversation(conversation)] = conversation.displayName || "";
    }
  } catch (err) {
    error.value = "读取聊天失败：" + err;
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

async function saveConversation(conversation: SourceConversationDto) {
  try {
    const value = conversationDrafts.value[keyOfConversation(conversation)]?.trim() || null;
    await updateSourceConversation({
      sourceType: conversation.sourceType,
      sourceAccountId: conversation.sourceAccountId,
      sourceConversationId: conversation.sourceConversationId,
      displayName: value,
      isFavorite: conversation.isFavorite,
    });
    pushToast({ tone: "success", title: "聊天名称已保存" });
    emit("changed");
    if (selected.value) await selectAccount(selected.value);
  } catch (err) {
    pushToast({ tone: "danger", title: "保存失败", description: String(err) });
  }
}

async function toggleConversationFavorite(conversation: SourceConversationDto) {
  try {
    await updateSourceConversation({
      sourceType: conversation.sourceType,
      sourceAccountId: conversation.sourceAccountId,
      sourceConversationId: conversation.sourceConversationId,
      displayName: conversation.displayName || null,
      isFavorite: !conversation.isFavorite,
    });
    emit("changed");
    if (selected.value) await selectAccount(selected.value);
  } catch (err) {
    pushToast({ tone: "danger", title: "更新收藏失败", description: String(err) });
  }
}

onMounted(refresh);
</script>
