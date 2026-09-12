<!--
  ChatVault 采集视图
  职责：微信账号探测、自定义目录、扫描入库；第二 Tab 维护来源标注与收藏。
-->
<template>
  <div class="flex h-full flex-col gap-4 p-6">
    <div class="flex flex-wrap items-end justify-between gap-3">
      <div>
        <h2 class="text-cv-page text-cv-text">采集</h2>
        <p class="mt-0.5 text-cv-caption text-cv-text-2">探测微信 4.x 附件目录并增量入库；也可标注来源显示名</p>
      </div>
      <div class="flex rounded-cv border border-cv-border bg-cv-surface p-0.5">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          class="rounded-cv px-3 py-1.5 text-cv-caption transition-colors"
          :class="activeTab === tab.id ? 'bg-cv-accent-soft font-medium text-cv-accent' : 'text-cv-text-2 hover:text-cv-text'"
          @click="activeTab = tab.id"
        >
          {{ tab.label }}
        </button>
      </div>
    </div>

    <template v-if="activeTab === 'scan'">
      <div class="min-h-0 flex-1 space-y-4 overflow-y-auto pb-2">
        <UiCard title="本机微信账号" :description="accountCountLabel">
          <template #headerExtra>
            <UiButton size="sm" variant="secondary" :loading="detecting" @click="loadAccounts">重新检测</UiButton>
          </template>
          <div v-if="accounts.length > 0" class="grid gap-2 sm:grid-cols-2">
            <label
              v-for="acc in accounts"
              :key="acc.sourceAccountId"
              class="flex cursor-pointer items-start gap-2.5 rounded-cv border p-3 transition-colors"
              :class="selectedAccounts.includes(acc.sourceAccountId) ? 'border-cv-accent bg-cv-accent-soft' : 'border-cv-border hover:border-cv-text-3'"
            >
              <input type="checkbox" class="mt-0.5 accent-[var(--cv-accent)]" :checked="selectedAccounts.includes(acc.sourceAccountId)" @change="toggleAccount(acc.sourceAccountId)" />
              <div class="min-w-0 flex-1">
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate font-mono text-cv-body font-medium text-cv-text">{{ acc.sourceAccountId }}</p>
                  <span class="shrink-0 text-cv-caption text-cv-text-3">约 {{ acc.filesCountEstimated }} 个文件</span>
                </div>
                <p class="mt-1 truncate font-mono text-cv-caption text-cv-text-3" :title="acc.sourceDir">{{ acc.sourceDir }}</p>
              </div>
            </label>
          </div>
          <p v-else class="py-6 text-center text-cv-caption text-cv-text-3">未探测到微信 4.x 目录</p>
        </UiCard>

        <UiCard title="通用本地文件夹" description="本次扫描临时附加；长期目录请在「设置 · 采集与定时」维护">
          <div class="flex gap-2">
            <div class="min-w-0 flex-1">
              <UiInput v-model="customFolderPath" class="font-mono" placeholder="本地文件夹绝对路径" @keyup.enter="addCustomFolder" />
            </div>
            <UiButton variant="secondary" @click="addCustomFolder">添加</UiButton>
          </div>
          <ul v-if="customFolders.length" class="mt-2 space-y-1">
            <li v-for="(path, idx) in customFolders" :key="path" class="flex items-center justify-between gap-2 rounded-cv bg-cv-surface-2 px-2.5 py-1.5 font-mono text-cv-caption text-cv-text-2">
              <span class="truncate">{{ path }}</span>
              <button class="shrink-0 text-cv-danger hover:underline" @click="customFolders.splice(idx, 1)">移除</button>
            </li>
          </ul>
        </UiCard>

        <UiCard v-if="persistedDirs.length" title="设置中的采集目录" description="扫描时自动并入，与定时任务一致">
          <ul class="space-y-1">
            <li
              v-for="path in persistedDirs"
              :key="path"
              class="rounded-cv bg-cv-surface-2 px-2.5 py-1.5 font-mono text-cv-caption text-cv-text-2 truncate"
            >
              {{ path }}
            </li>
          </ul>
        </UiCard>

        <UiCard>
          <label class="flex items-center gap-2 text-cv-caption text-cv-text-2">
            <input v-model="fullScan" type="checkbox" class="accent-[var(--cv-accent)]" />
            强制全量扫描（忽略增量检查点）
          </label>
          <div class="mt-3 flex flex-wrap items-center gap-3">
            <UiButton variant="primary" :loading="scanning" :disabled="scanning || (selectedAccounts.length === 0 && customFolders.length === 0)" @click="startScanning">
              {{ scanning ? "正在扫描入库…" : fullScan ? "开始全量扫描" : "开始增量扫描" }}
            </UiButton>
            <p class="text-cv-caption text-cv-text-3">增量扫描只处理 mtime 更新的目录，未变更文件不读内容。</p>
          </div>
        </UiCard>

        <UiCard v-if="scanResult" title="扫描完成" description="内容哈希去重后入库">
          <div class="grid grid-cols-3 gap-3">
            <div class="rounded-cv bg-cv-surface-2 p-3"><p class="text-cv-caption text-cv-text-3">发现</p><p class="mt-0.5 text-lg font-semibold text-cv-text">{{ scanResult.totalDiscovered }}</p></div>
            <div class="rounded-cv bg-cv-surface-2 p-3"><p class="text-cv-caption text-cv-text-3">去重跳过</p><p class="mt-0.5 text-lg font-semibold text-cv-text-2">{{ scanResult.totalSkipped }}</p></div>
            <div class="rounded-cv bg-cv-surface-2 p-3"><p class="text-cv-caption text-cv-text-3">新增对象</p><p class="mt-0.5 text-lg font-semibold text-cv-accent">{{ scanResult.totalNewObjects }}</p></div>
          </div>
          <div class="mt-3 flex items-center justify-between">
            <span class="text-cv-caption text-cv-text-3">耗时 {{ scanResult.durationMs }} ms</span>
            <UiButton size="sm" variant="primary" @click="navigateTo('library')">去检索</UiButton>
          </div>
        </UiCard>
      </div>
    </template>

    <template v-else>
      <div v-if="error" class="rounded-cv border border-cv-border bg-cv-surface px-3 py-2 text-cv-caption text-cv-danger">{{ error }}</div>
      <div class="grid min-h-0 flex-1 grid-cols-1 gap-3 xl:grid-cols-[minmax(300px,0.9fr)_minmax(380px,1.3fr)]">
        <section class="flex min-h-0 flex-col overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface">
          <header class="flex items-center justify-between border-b border-cv-border px-4 py-3">
            <div><h3 class="text-cv-section text-cv-text">来源账号</h3><p class="mt-0.5 text-cv-caption text-cv-text-3">{{ sourcesAccounts.length }} 个</p></div>
            <UiButton size="sm" variant="ghost" :loading="sourcesLoading" @click="refreshSources">刷新</UiButton>
          </header>
          <div class="min-h-0 flex-1 overflow-y-auto">
            <p v-if="sourcesAccounts.length === 0" class="p-8 text-center text-cv-caption text-cv-text-3">扫描入库后，来源账号会出现在这里</p>
            <div v-for="account in sourcesAccounts" :key="accountKey(account)" class="cursor-pointer border-b border-cv-border/60 px-4 py-3 last:border-0" :class="selectedAccountKey === accountKey(account) ? 'bg-cv-accent-soft' : 'hover:bg-cv-surface-2'" @click="selectAccount(account)">
              <div class="flex items-start gap-2">
                <button class="mt-0.5 text-cv-caption" :class="account.isFavorite ? 'text-cv-warning' : 'text-cv-text-3 hover:text-cv-warning'" @click.stop="toggleAccountFavorite(account)">
                  <Star class="h-3.5 w-3.5" :fill="account.isFavorite ? 'currentColor' : 'none'" />
                </button>
                <div class="min-w-0 flex-1">
                  <div class="flex items-center justify-between gap-2">
                    <span class="truncate text-cv-body font-medium text-cv-text">{{ account.effectiveName }}</span>
                    <span class="shrink-0 text-cv-caption text-cv-text-3">{{ account.recordCount }} 文件</span>
                  </div>
                  <p class="mt-0.5 truncate text-cv-caption text-cv-text-3">{{ account.sourceType }} · {{ account.sourceAccountId }}</p>
                  <div class="mt-2 flex gap-1.5" @click.stop>
                    <UiInput v-model="accountDrafts[accountKey(account)]" placeholder="自定义显示名称" @keyup.enter="saveAccount(account)" />
                    <UiButton size="sm" variant="primary" @click="saveAccount(account)">保存</UiButton>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section class="flex min-h-0 flex-col overflow-hidden rounded-cv-lg border border-cv-border bg-cv-surface">
          <header class="border-b border-cv-border px-4 py-3">
            <h3 class="text-cv-section text-cv-text">来源聊天</h3>
            <p class="mt-0.5 text-cv-caption text-cv-text-3">{{ selectedAccount ? selectedAccount.effectiveName : "选择左侧账号查看" }}</p>
          </header>
          <div class="min-h-0 flex-1 overflow-y-auto">
            <p v-if="!selectedAccount" class="p-10 text-center text-cv-caption text-cv-text-3">请选择一个来源账号</p>
            <p v-else-if="conversations.length === 0" class="p-10 text-center text-cv-caption text-cv-text-3">当前账号没有可标注的聊天映射</p>
            <div v-for="conversation in conversations" :key="conversationKey(conversation)" class="border-b border-cv-border/60 px-4 py-3 last:border-0">
              <div class="flex items-start gap-2">
                <button class="mt-0.5 text-cv-caption" :class="conversation.isFavorite ? 'text-cv-warning' : 'text-cv-text-3 hover:text-cv-warning'" @click="toggleConversationFavorite(conversation)">
                  <Star class="h-3.5 w-3.5" :fill="conversation.isFavorite ? 'currentColor' : 'none'" />
                </button>
                <div class="min-w-0 flex-1">
                  <div class="flex items-center justify-between gap-2">
                    <span class="truncate text-cv-body font-medium text-cv-text">{{ conversation.effectiveName }}</span>
                    <span class="shrink-0 text-cv-caption text-cv-text-3">{{ conversation.recordCount }} 文件</span>
                  </div>
                  <p class="mt-0.5 truncate font-mono text-cv-caption text-cv-text-3">{{ conversation.sourceConversationId }}</p>
                  <div class="mt-2 flex gap-1.5">
                    <UiInput v-model="conversationDrafts[conversationKey(conversation)]" placeholder="自定义显示名称" @keyup.enter="saveConversation(conversation)" />
                    <UiButton size="sm" variant="primary" @click="saveConversation(conversation)">保存</UiButton>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>
      </div>
    </template>
  </div>
</template>


<script setup lang="ts">
import { computed, ref, onMounted } from "vue";
import { Star } from "lucide-vue-next";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import {
  detectWechatAccounts,
  runScan,
  listSourceAccounts,
  listSourceConversations,
  updateSourceAccount,
  updateSourceConversation,
  getAppSettings,
} from "../api/tauri";
import { pushToast } from "../composables/useToast";
import { navigateTo } from "../composables/useNav";
import type {
  WechatAccountDto,
  ScanResultDto,
  SourceAccountDto,
  SourceConversationDto,
} from "../types";

const tabs = [
  { id: "scan", label: "扫描入库" },
  { id: "sources", label: "来源标注" },
] as const;
const activeTab = ref<(typeof tabs)[number]["id"]>("scan");

const accounts = ref<WechatAccountDto[]>([]);
const selectedAccounts = ref<string[]>([]);
const customFolderPath = ref("");
const customFolders = ref<string[]>([]);
const detecting = ref(false);
const scanning = ref(false);
const fullScan = ref(false);
const scanResult = ref<ScanResultDto | null>(null);

const sourcesAccounts = ref<SourceAccountDto[]>([]);
const conversations = ref<SourceConversationDto[]>([]);
const selectedAccount = ref<SourceAccountDto | null>(null);
const selectedAccountKey = ref("");
const accountDrafts = ref<Record<string, string>>({});
const conversationDrafts = ref<Record<string, string>>({});
const sourcesLoading = ref(false);
const error = ref("");

const accountCountLabel = computed(() => "已探测 " + accounts.value.length + " 个账号");

/** 设置中已持久化的采集目录，扫描时与会话目录合并。 */
const persistedDirs = ref<string[]>([]);

/** 账号复合键，隔离不同来源类型下的相同 ID。 */
function accountKey(account: SourceAccountDto): string {
  return account.sourceType + "|" + account.sourceAccountId;
}

/** 聊天复合键。 */
function conversationKey(conversation: SourceConversationDto): string {
  return (
    conversation.sourceType +
    "|" +
    conversation.sourceAccountId +
    "|" +
    conversation.sourceConversationId
  );
}

/** 探测本机微信 4.x 账号。 */
async function loadAccounts() {
  detecting.value = true;
  try {
    const list = await detectWechatAccounts();
    accounts.value = list;
    selectedAccounts.value = list.map((a) => a.sourceAccountId);
  } catch (err) {
    pushToast({ tone: "danger", title: "探测微信账号失败", description: String(err) });
  } finally {
    detecting.value = false;
  }
}

/** 切换账号勾选。 */
function toggleAccount(accId: string) {
  const idx = selectedAccounts.value.indexOf(accId);
  if (idx >= 0) selectedAccounts.value.splice(idx, 1);
  else selectedAccounts.value.push(accId);
}

/** 将输入路径加入本次扫描的会话目录（不写设置，避免 setAppSettings 副作用）。 */
function addCustomFolder() {
  const p = customFolderPath.value.trim();
  if (p && !customFolders.value.includes(p) && !persistedDirs.value.includes(p)) {
    customFolders.value.push(p);
    customFolderPath.value = "";
  }
}

/** 从设置加载持久化采集目录。 */
async function loadPersistedDirs() {
  try {
    const settings = await getAppSettings();
    persistedDirs.value = settings.collectDirs || [];
  } catch {
    persistedDirs.value = [];
  }
}

/** 执行扫描：合并设置中的采集目录与会话目录。 */
async function startScanning() {
  scanning.value = true;
  scanResult.value = null;
  try {
    await loadPersistedDirs();
    const folders = Array.from(new Set([...persistedDirs.value, ...customFolders.value]));
    const res = await runScan({
      targetAccounts: selectedAccounts.value,
      customFolders: folders,
      fullScan: fullScan.value,
    });
    scanResult.value = res;
    pushToast({
      tone: "success",
      title: "扫描完成",
      description: "发现 " + res.totalDiscovered + "，新增对象 " + res.totalNewObjects,
      action: { label: "去检索", onClick: () => navigateTo("library") },
    });
  } catch (err) {
    pushToast({ tone: "danger", title: "扫描失败", description: String(err) });
  } finally {
    scanning.value = false;
  }
}

/** 刷新来源账号列表。 */
async function refreshSources() {
  sourcesLoading.value = true;
  error.value = "";
  try {
    sourcesAccounts.value = await listSourceAccounts();
    for (const account of sourcesAccounts.value) {
      accountDrafts.value[accountKey(account)] = account.displayName || "";
    }
    if (selectedAccountKey.value) {
      const next = sourcesAccounts.value.find((item) => accountKey(item) === selectedAccountKey.value);
      if (next) await selectAccount(next);
      else {
        selectedAccount.value = null;
        conversations.value = [];
      }
    }
  } catch (err) {
    error.value = "读取来源映射失败：" + err;
  } finally {
    sourcesLoading.value = false;
  }
}

/** 选中账号并加载聊天映射。 */
async function selectAccount(account: SourceAccountDto) {
  selectedAccount.value = account;
  selectedAccountKey.value = accountKey(account);
  try {
    conversations.value = await listSourceConversations(account.sourceType, account.sourceAccountId);
    for (const conversation of conversations.value) {
      conversationDrafts.value[conversationKey(conversation)] = conversation.displayName || "";
    }
  } catch (err) {
    error.value = "读取聊天失败：" + err;
  }
}

/** 保存账号自定义显示名。 */
async function saveAccount(account: SourceAccountDto) {
  try {
    const value = accountDrafts.value[accountKey(account)]?.trim() || null;
    await updateSourceAccount({
      sourceType: account.sourceType,
      sourceAccountId: account.sourceAccountId,
      displayName: value,
      isFavorite: account.isFavorite,
    });
    pushToast({ tone: "success", title: "账号名称已保存" });
    await refreshSources();
  } catch (err) {
    pushToast({ tone: "danger", title: "保存失败", description: String(err) });
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
    await refreshSources();
  } catch (err) {
    pushToast({ tone: "danger", title: "更新收藏失败", description: String(err) });
  }
}

/** 保存聊天自定义显示名。 */
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
    pushToast({ tone: "success", title: "聊天名称已保存" });
    if (selectedAccount.value) await selectAccount(selectedAccount.value);
  } catch (err) {
    pushToast({ tone: "danger", title: "保存失败", description: String(err) });
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
    if (selectedAccount.value) await selectAccount(selectedAccount.value);
  } catch (err) {
    pushToast({ tone: "danger", title: "更新收藏失败", description: String(err) });
  }
}

onMounted(() => {
  loadAccounts();
  refreshSources();
  loadPersistedDirs();
});

</script>

