<!-- ChatVault 来源管理视图：维护来源账号与来源聊天的显示名称和收藏状态。 -->
<template>
  <div class="h-full flex flex-col p-6 space-y-5 overflow-auto">
    <div class="flex items-start justify-between gap-4">
      <div>
        <h2 class="text-xl font-semibold text-white">来源管理</h2>
        <p class="mt-1 text-xs text-slate-400">来源 ID 由来源系统提供且不可编辑；名称和收藏状态会随 WebDAV 同步。</p>
      </div>
      <button
        class="inline-flex items-center gap-2 px-3 py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-sm text-slate-200"
        :disabled="loading"
        @click="refresh"
      >
        <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': loading }" />
        刷新
      </button>
    </div>

    <div v-if="error" class="rounded-lg border border-red-900/70 bg-red-950/30 px-4 py-3 text-sm text-red-300">
      {{ error }}
    </div>

    <div class="grid grid-cols-1 xl:grid-cols-[minmax(320px,0.9fr)_minmax(420px,1.4fr)] gap-4 min-h-0">
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 overflow-hidden">
        <div class="px-4 py-3 border-b border-slate-800 flex items-center justify-between">
          <div>
            <h3 class="font-medium text-slate-100">来源账号</h3>
            <p class="text-[11px] text-slate-500 mt-1">{{ accounts.length }} 个账号来源</p>
          </div>
          <ContactRound class="w-5 h-5 text-emerald-400" />
        </div>

        <div v-if="accounts.length === 0" class="p-8 text-center text-sm text-slate-500">
          扫描并入库文件后，来源账号会自动出现在这里。
        </div>
        <div v-else class="divide-y divide-slate-800/70">
          <div
            v-for="account in accounts"
            :key="accountKey(account)"
            class="w-full text-left px-4 py-3 transition-colors"
            :class="selectedAccountKey === accountKey(account) ? 'bg-emerald-950/40' : 'hover:bg-slate-800/50'"
            @click="selectAccount(account)"
          >
            <div class="flex items-start gap-3">
              <button
                class="mt-0.5 text-lg leading-none"
                :class="account.isFavorite ? 'text-amber-300' : 'text-slate-600 hover:text-amber-300'"
                :title="account.isFavorite ? '取消收藏' : '收藏账号'"
                @click.stop="toggleAccountFavorite(account)"
              >
                {{ account.isFavorite ? '★' : '☆' }}
              </button>
              <div class="min-w-0 flex-1">
                <div class="flex items-center justify-between gap-2">
                  <span class="truncate font-medium text-slate-100">{{ account.effectiveName }}</span>
                  <span class="shrink-0 text-[11px] text-slate-500">{{ account.recordCount }} 个文件</span>
                </div>
                <p class="mt-1 truncate text-[11px] text-slate-500" :title="account.sourceAccountId">
                  {{ account.sourceType }} · {{ account.sourceAccountId }}
                </p>
                <p class="mt-2 text-[11px] text-slate-600">原始名称：{{ account.sourceName || '未提供' }}</p>
                <div class="mt-2 flex gap-2" @click.stop>
                  <input
                    v-model="accountDrafts[accountKey(account)]"
                    class="min-w-0 flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-xs text-slate-200 focus:border-emerald-500 focus:outline-none"
                    placeholder="自定义显示名称（可清空）"
                    @keyup.enter="saveAccount(account)"
                  />
                  <button class="rounded bg-emerald-700 px-2.5 py-1 text-xs text-white hover:bg-emerald-600" @click="saveAccount(account)">
                    保存
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section class="rounded-xl border border-slate-800 bg-slate-900/60 overflow-hidden">
        <div class="px-4 py-3 border-b border-slate-800 flex items-center justify-between">
          <div>
            <h3 class="font-medium text-slate-100">来源聊天</h3>
            <p class="text-[11px] text-slate-500 mt-1">
              {{ selectedAccount ? `当前账号：${selectedAccount.effectiveName}` : '选择左侧账号查看聊天' }}
            </p>
          </div>
          <MessageCircle class="w-5 h-5 text-sky-400" />
        </div>

        <div v-if="!selectedAccount" class="p-12 text-center text-sm text-slate-500">
          请选择一个来源账号。
        </div>
        <div v-else-if="conversations.length === 0" class="p-12 text-center">
          <MessageCircle class="mx-auto mb-3 h-9 w-9 text-slate-700" />
          <p class="text-sm text-slate-400">当前账号没有可维护的聊天映射</p>
          <p class="mt-2 text-xs leading-5 text-slate-600">微信 4.x 只有在文件位于明确的聊天子目录时才会产生聊天 ID；标准月份平铺文件没有可靠聊天信息，因此不会被伪造归类。</p>
        </div>
        <div v-else class="divide-y divide-slate-800/70">
          <div v-for="conversation in conversations" :key="conversationKey(conversation)" class="px-4 py-3">
            <div class="flex items-start gap-3">
              <button
                class="mt-0.5 text-lg leading-none"
                :class="conversation.isFavorite ? 'text-amber-300' : 'text-slate-600 hover:text-amber-300'"
                :title="conversation.isFavorite ? '取消收藏' : '收藏聊天'"
                @click="toggleConversationFavorite(conversation)"
              >
                {{ conversation.isFavorite ? '★' : '☆' }}
              </button>
              <div class="min-w-0 flex-1">
                <div class="flex items-center justify-between gap-2">
                  <span class="truncate font-medium text-slate-100">{{ conversation.effectiveName }}</span>
                  <span class="shrink-0 text-[11px] text-slate-500">{{ conversation.recordCount }} 个文件</span>
                </div>
                <p class="mt-1 truncate font-mono text-[11px] text-slate-500">{{ conversation.sourceConversationId }}</p>
                <p class="mt-2 text-[11px] text-slate-600">原始名称：{{ conversation.sourceName || '未提供' }}</p>
                <div class="mt-2 flex gap-2">
                  <input
                    v-model="conversationDrafts[conversationKey(conversation)]"
                    class="min-w-0 flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-xs text-slate-200 focus:border-emerald-500 focus:outline-none"
                    placeholder="自定义显示名称（可清空）"
                    @keyup.enter="saveConversation(conversation)"
                  />
                  <button class="rounded bg-emerald-700 px-2.5 py-1 text-xs text-white hover:bg-emerald-600" @click="saveConversation(conversation)">
                    保存
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { ContactRound, MessageCircle, RefreshCw } from "lucide-vue-next";
import {
  listSourceAccounts,
  listSourceConversations,
  updateSourceAccount,
  updateSourceConversation,
} from "../api/tauri";
import type { SourceAccountDto, SourceConversationDto } from "../types";

const accounts = ref<SourceAccountDto[]>([]);
const conversations = ref<SourceConversationDto[]>([]);
const selectedAccount = ref<SourceAccountDto | null>(null);
const selectedAccountKey = ref("");
const accountDrafts = ref<Record<string, string>>({});
const conversationDrafts = ref<Record<string, string>>({});
const loading = ref(false);
const error = ref("");

/** 生成账号的复合键，隔离不同来源类型下相同的原始 ID。 */
function accountKey(account: SourceAccountDto): string {
  return `${account.sourceType}\u0000${account.sourceAccountId}`;
}

/** 生成聊天的复合键，隔离不同账号下相同的聊天 ID。 */
function conversationKey(conversation: SourceConversationDto): string {
  return `${conversation.sourceType}\u0000${conversation.sourceAccountId}\u0000${conversation.sourceConversationId}`;
}

/** 读取映射列表并保留当前选中的账号。 */
async function refresh() {
  loading.value = true;
  error.value = "";
  try {
    accounts.value = await listSourceAccounts();
    for (const account of accounts.value) {
      accountDrafts.value[accountKey(account)] = account.displayName || "";
    }
    if (selectedAccountKey.value) {
      const next = accounts.value.find((item) => accountKey(item) === selectedAccountKey.value);
      if (next) await selectAccount(next);
      else {
        selectedAccount.value = null;
        conversations.value = [];
      }
    }
  } catch (err) {
    error.value = `读取来源映射失败：${err}`;
  } finally {
    loading.value = false;
  }
}

/** 选择账号并加载其聊天映射。 */
async function selectAccount(account: SourceAccountDto) {
  selectedAccount.value = account;
  selectedAccountKey.value = accountKey(account);
  try {
    conversations.value = await listSourceConversations(account.sourceType, account.sourceAccountId);
    for (const conversation of conversations.value) {
      conversationDrafts.value[conversationKey(conversation)] = conversation.displayName || "";
    }
  } catch (err) {
    error.value = `读取来源聊天失败：${err}`;
  }
}

/** 提交账号自定义名称；空白输入转换为 NULL。 */
async function saveAccount(account: SourceAccountDto) {
  try {
    const value = accountDrafts.value[accountKey(account)]?.trim() || null;
    await updateSourceAccount({
      sourceType: account.sourceType,
      sourceAccountId: account.sourceAccountId,
      displayName: value,
      isFavorite: account.isFavorite,
    });
    await refresh();
  } catch (err) {
    error.value = `保存账号映射失败：${err}`;
  }
}

/** 切换账号收藏状态。 */
async function toggleAccountFavorite(account: SourceAccountDto) {
  try {
    await updateSourceAccount({
      sourceType: account.sourceType,
      sourceAccountId: account.sourceAccountId,
      displayName: account.displayName || null,
      isFavorite: !account.isFavorite,
    });
    await refresh();
  } catch (err) {
    error.value = `更新账号收藏失败：${err}`;
  }
}

/** 提交聊天自定义名称；空白输入转换为 NULL。 */
async function saveConversation(conversation: SourceConversationDto) {
  try {
    const value = conversationDrafts.value[conversationKey(conversation)]?.trim() || null;
    await updateSourceConversation({
      sourceType: conversation.sourceType,
      sourceAccountId: conversation.sourceAccountId,
      sourceConversationId: conversation.sourceConversationId,
      displayName: value,
      isFavorite: conversation.isFavorite,
    });
    await selectAccount(selectedAccount.value!);
  } catch (err) {
    error.value = `保存聊天映射失败：${err}`;
  }
}

/** 切换聊天收藏状态。 */
async function toggleConversationFavorite(conversation: SourceConversationDto) {
  try {
    await updateSourceConversation({
      sourceType: conversation.sourceType,
      sourceAccountId: conversation.sourceAccountId,
      sourceConversationId: conversation.sourceConversationId,
      displayName: conversation.displayName || null,
      isFavorite: !conversation.isFavorite,
    });
    await selectAccount(selectedAccount.value!);
  } catch (err) {
    error.value = `更新聊天收藏失败：${err}`;
  }
}

onMounted(refresh);
</script>
