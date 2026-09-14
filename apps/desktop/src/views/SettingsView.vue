<!--
  ChatVault 设置视图
  职责：身份、WebDAV 连接、缓存、外观主题与关于。采集范围与调度在任务页维护。
-->
<template>
  <div class="flex h-full flex-col gap-4 p-6">
    <div class="flex flex-wrap items-end justify-between gap-3">
      <div>
        <h2 class="text-cv-page text-cv-text">设置</h2>
      </div>
      <UiButton variant="primary" :loading="saving" :disabled="identityConflict" @click="save">
        {{ anyDirty ? "保存更改" : "保存设置" }}
      </UiButton>
    </div>

    <div class="min-h-0 flex-1 space-y-4 overflow-y-auto pb-4">
      <UiCard
        title="身份与资料库"
        info="已关联区显示当前网盘绑定；下方可改设备名称和 Vault 后缀。换 Vault 需先清空关联。"
      >
        <template #headerExtra>
          <div class="flex shrink-0 items-center gap-2">
            <span
              class="inline-flex items-center rounded-cv px-2 py-0.5 text-cv-caption font-medium"
              :class="
                isBound
                  ? 'bg-cv-accent-soft text-cv-accent'
                  : 'bg-cv-surface-2 text-cv-text-2'
              "
            >
              {{ isBound ? "已关联" : "未关联" }}
            </span>
            <UiButton
              variant="danger"
              size="sm"
              :disabled="!isBound"
              :loading="resettingVault"
              @click="onResetVaultBinding"
            >
              清空关联
            </UiButton>
          </div>
        </template>

        <div
          class="rounded-cv border px-3 py-2.5"
          :class="isBound ? 'border-cv-border bg-cv-surface-2' : 'border-dashed border-cv-border bg-cv-surface'"
        >
          <div class="flex flex-wrap items-baseline justify-between gap-2">
            <p class="text-cv-caption font-medium text-cv-text-2">
              {{ isBound ? "当前关联的网盘资料库" : "尚未关联网盘" }}
            </p>
            <p v-if="isBound" class="font-mono text-cv-caption text-cv-text-3">
              {{ form.boundWebdavUrl }}
            </p>
          </div>
          <p v-if="isBound" class="mt-1 font-mono text-cv-body text-cv-text" :title="form.boundVaultId || undefined">
            {{ form.boundVaultId }}
          </p>
          <p v-else class="mt-1 text-cv-caption text-cv-text-3">
            填好 Vault ID 与 WebDAV 并保存后，首次运行任务会自动建立关联。
          </p>
        </div>

        <div class="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2 sm:gap-4">
          <div class="min-w-0">
            <div class="flex items-baseline justify-between gap-2">
              <label class="text-cv-caption text-cv-text-2" for="vault-suffix">Vault ID</label>
              <span v-if="identityDirty" class="text-cv-caption text-cv-warning">未保存</span>
            </div>
            <div class="mt-1 flex items-stretch">
              <span
                class="inline-flex shrink-0 items-center rounded-l-cv border border-r-0 border-cv-border bg-cv-surface-2 px-2.5 font-mono text-cv-caption text-cv-text-2"
              >
                chatvault-
              </span>
              <input
                id="vault-suffix"
                v-model="vaultSuffix"
                type="text"
                maxlength="64"
                placeholder="home"
                class="w-full min-w-0 rounded-r-cv border bg-cv-surface px-3 py-2 font-mono text-cv-body text-cv-text placeholder:text-cv-text-3 focus:outline-none focus:ring-2 focus:ring-cv-accent/30"
                :class="
                  identityConflict
                    ? 'border-cv-danger focus:border-cv-danger'
                    : 'border-cv-border focus:border-cv-accent'
                "
              />
            </div>
            <p
              class="mt-1 truncate font-mono text-cv-caption"
              :class="identityConflict ? 'text-cv-danger' : 'text-cv-text-3'"
              :title="fullVaultId"
            >
              {{ identityConflict ? "与已关联 Vault 不一致，请先清空关联" : fullVaultId }}
            </p>
          </div>
          <div class="min-w-0">
            <div class="flex items-baseline justify-between gap-2">
              <label class="text-cv-caption text-cv-text-2" for="device-name">设备名称</label>
              <span v-if="deviceNameDirty" class="text-cv-caption text-cv-warning">未保存</span>
            </div>
            <UiInput
              id="device-name"
              v-model="form.deviceName"
              class="mt-1"
              placeholder="例如 台式机"
              maxlength="64"
            />
            <p class="mt-1 truncate text-cv-caption text-cv-text-3" :title="form.deviceName">
              {{ form.deviceName || "将写入设备注册供其他设备显示" }}
            </p>
          </div>
          <div class="min-w-0 sm:col-span-2">
            <div class="flex items-baseline justify-between gap-2">
              <span class="text-cv-caption text-cv-text-2">设备 ID</span>
              <span class="text-cv-caption text-cv-text-3">系统生成，不可修改</span>
            </div>
            <p
              class="mt-1 break-all rounded-cv border border-cv-border bg-cv-surface-2 px-3 py-1.5 font-mono text-cv-caption text-cv-text-2"
              :title="form.deviceId"
            >
              {{ form.deviceId || "—" }}
            </p>
          </div>
        </div>
      </UiCard>

      <UiCard
        title="WebDAV 连接"
        info="任务流水线与换机恢复共用。密码保存在 Windows 凭据管理器；测试用当前表单与已存凭据。"
      >
        <template #headerExtra>
          <span v-if="webdavDirty" class="shrink-0 text-cv-caption text-cv-warning">未保存</span>
        </template>
        <div class="space-y-3">
          <label class="block">
            <span class="text-cv-caption text-cv-text-2">服务器地址</span>
            <UiInput v-model="form.webdavUrl" class="mt-1 font-mono" placeholder="https://dav.example.com/dav/" />
          </label>
          <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <label class="block">
              <span class="text-cv-caption text-cv-text-2">用户名</span>
              <UiInput v-model="form.webdavUsername" class="mt-1 font-mono" />
            </label>
            <label class="block">
              <span class="text-cv-caption text-cv-text-2">新密码</span>
              <UiInput v-model="webdavPassword" type="password" class="mt-1" placeholder="留空则保留已存凭据" />
            </label>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <UiButton size="sm" variant="secondary" :loading="testingWebdav" :disabled="!form.webdavUrl" @click="testWebdavConnection">
              测试连接
            </UiButton>
            <UiButton
              size="sm"
              variant="ghost"
              :loading="clearingCredential"
              :disabled="!form.webdavUrl || !form.webdavUsername"
              @click="clearStoredCredential"
            >
              清除已存密码
            </UiButton>
          </div>
          <WebdavTestResult v-if="webdavTestResult" :capability="webdavTestResult" />
        </div>
      </UiCard>

      <CacheSettings v-model="form" :dirty="cacheDirty" />

      <UiCard
        title="下载目录"
        info="仅远程文件「下载」的落点。留空则使用系统「下载\\ChatVault」。"
      >
        <template #headerExtra>
          <span v-if="downloadDirDirty" class="shrink-0 text-cv-caption text-cv-warning">未保存</span>
        </template>
        <div class="flex flex-wrap items-end gap-2">
          <label class="min-w-[220px] flex-1">
            <span class="text-cv-caption text-cv-text-2">目录路径</span>
            <UiInput
              v-model="form.downloadDir"
              class="mt-1 font-mono"
              placeholder="默认 %USERPROFILE%\Downloads\ChatVault"
            />
          </label>
          <UiButton size="md" variant="secondary" @click="pickDownloadDir">选择目录</UiButton>
          <UiButton size="md" variant="ghost" @click="form.downloadDir = ''">恢复默认</UiButton>
        </div>
      </UiCard>

      <UiCard title="外观">
        <div class="flex flex-wrap gap-2">
          <button
            v-for="opt in themeOptions"
            :key="opt.value"
            class="rounded-cv border px-3 py-1.5 text-cv-caption transition-colors"
            :class="
              preference === opt.value
                ? 'border-cv-accent bg-cv-accent-soft text-cv-accent font-medium'
                : 'border-cv-border text-cv-text-2 hover:bg-cv-surface-2'
            "
            @click="setThemePreference(opt.value)"
          >
            {{ opt.label }}
          </button>
        </div>
      </UiCard>

      <UiCard title="存储概览">
        <div v-if="stats" class="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <div>
            <p class="text-cv-caption text-cv-text-3">附件记录</p>
            <p class="text-lg font-semibold text-cv-text">{{ stats.totalRecords }}</p>
          </div>
          <div>
            <p class="text-cv-caption text-cv-text-3">唯一对象</p>
            <p class="text-lg font-semibold text-cv-text">{{ stats.uniqueObjects }}</p>
          </div>
          <div>
            <p class="text-cv-caption text-cv-text-3">原始体积</p>
            <p class="text-lg font-semibold text-cv-text">{{ stats.formattedTotalRaw }}</p>
          </div>
          <div>
            <p class="text-cv-caption text-cv-text-3">去重节省</p>
            <p class="text-lg font-semibold text-cv-accent">{{ stats.formattedSavedBytes }}</p>
          </div>
        </div>
        <p v-else class="text-cv-caption text-cv-text-3">暂无统计数据</p>
      </UiCard>

      <AppAbout />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import CacheSettings from "../components/CacheSettings.vue";
import AppAbout from "../components/AppAbout.vue";
import WebdavTestResult from "../components/WebdavTestResult.vue";
import {
  getAppSettings,
  setAppSettings,
  saveWebdavCredential,
  clearWebdavCredential,
  getVaultStats,
  testWebdav,
  pickDirectory,
  resetVaultBinding,
} from "../api/tauri";
import { pushToast } from "../composables/useToast";
import { confirmAction } from "../composables/useConfirm";
import { useTheme, type ThemePreference } from "../composables/useTheme";
import type { AppSettingsDto, VaultStatsDto, WebdavCapabilityDto, WebdavConfigDto } from "../types";

const VAULT_ID_PREFIX = "chatvault-";

const { preference, setThemePreference } = useTheme();

const themeOptions: { value: ThemePreference; label: string }[] = [
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
  { value: "system", label: "跟随系统" },
];

const form = ref<AppSettingsDto>({
  vaultId: `${VAULT_ID_PREFIX}default`,
  deviceId: "",
  deviceName: "",
  webdavUrl: "",
  webdavUsername: "",
  boundVaultId: null,
  boundWebdavUrl: null,
  copyThresholdMib: 100,
  cacheRetentionDays: 7,
  cacheMaxMib: 1024,
  downloadDir: "",
  scanIntervalMinutes: 30,
  scheduleEnabled: false,
  collectSources: [],
});

/** 用户可配置的 Vault ID 后缀 */
const vaultSuffix = ref("default");
const fullVaultId = computed(() => `${VAULT_ID_PREFIX}${vaultSuffix.value.trim()}`);

/** 从完整 Vault ID 提取可编辑后缀 */
function extractVaultSuffix(vaultId: string): string {
  return vaultId.startsWith(VAULT_ID_PREFIX) ? vaultId.slice(VAULT_ID_PREFIX.length) : vaultId;
}

/** 已保存快照，用于判断编辑态 */
const savedSnapshot = ref({
  vaultId: "",
  deviceName: "",
  webdavUrl: "",
  webdavUsername: "",
  copyThresholdMib: 0,
  cacheRetentionDays: 0,
  cacheMaxMib: 0,
  downloadDir: "",
});

const isBound = computed(() => Boolean(form.value.boundVaultId));
const identityDirty = computed(() => fullVaultId.value !== savedSnapshot.value.vaultId);
const deviceNameDirty = computed(
  () => form.value.deviceName.trim() !== savedSnapshot.value.deviceName,
);
const webdavDirty = computed(
  () =>
    form.value.webdavUrl !== savedSnapshot.value.webdavUrl ||
    form.value.webdavUsername !== savedSnapshot.value.webdavUsername ||
    webdavPassword.value.length > 0,
);
const cacheDirty = computed(
  () =>
    form.value.copyThresholdMib !== savedSnapshot.value.copyThresholdMib ||
    form.value.cacheRetentionDays !== savedSnapshot.value.cacheRetentionDays ||
    form.value.cacheMaxMib !== savedSnapshot.value.cacheMaxMib,
);
const downloadDirDirty = computed(
  () => form.value.downloadDir !== savedSnapshot.value.downloadDir,
);
const anyDirty = computed(
  () =>
    identityDirty.value ||
    deviceNameDirty.value ||
    webdavDirty.value ||
    cacheDirty.value ||
    downloadDirDirty.value,
);
/** 已关联但改成了别的 Vault：必须先清空 */
const identityConflict = computed(
  () => isBound.value && form.value.boundVaultId !== fullVaultId.value,
);

const webdavPassword = ref("");
const saving = ref(false);
const resettingVault = ref(false);
const testingWebdav = ref(false);
const clearingCredential = ref(false);
const webdavTestResult = ref<WebdavCapabilityDto | null>(null);
const stats = ref<VaultStatsDto | null>(null);

/** 用后端设置刷新表单与关联回显。 */
async function reloadSettings() {
  const s = await getAppSettings();
  form.value = s;
  vaultSuffix.value = extractVaultSuffix(s.vaultId) || "default";
  savedSnapshot.value = {
    vaultId: s.vaultId,
    deviceName: s.deviceName.trim(),
    webdavUrl: s.webdavUrl,
    webdavUsername: s.webdavUsername,
    copyThresholdMib: s.copyThresholdMib,
    cacheRetentionDays: s.cacheRetentionDays,
    cacheMaxMib: s.cacheMaxMib,
    downloadDir: s.downloadDir,
  };
  webdavPassword.value = "";
}

/** 强制确认后清空旧 Vault 绑定与同步状态。 */
async function onResetVaultBinding() {
  const boundVault = form.value.boundVaultId || fullVaultId.value;
  const ok = await confirmAction({
    title: "确定清空当前关联吗？",
    description:
      `将断开与「${boundVault}」的关联，之后可以换用新的 Vault ID。\n\n会丢掉：\n· 和网盘的连接记录\n· 备份进度（文件会重新备份）\n· 其他设备的同步信息\n\n会保留：\n· 本机已扫描的文件列表\n· 来源名称和备注\n· 采集目录、定时任务等设置\n\n网盘上已备份的文件不会被删除，只是这台电脑不再认它。`,
    confirmLabel: "我已了解，继续清空",
    danger: true,
    requirePhrase: boundVault,
    requirePhraseLabel: "请输入当前 Vault ID 确认",
  });
  if (!ok) return;
  resettingVault.value = true;
  try {
    const report = await resetVaultBinding();
    await reloadSettings();
    pushToast({
      tone: "success",
      title: "已清空关联",
      description: report.requeuedUploads
        ? `有 ${report.requeuedUploads} 个文件会重新备份到新 Vault`
        : "现在可以改用新的 Vault ID 了",
    });
  } catch (err) {
    pushToast({ tone: "danger", title: "清空失败", description: String(err) });
  } finally {
    resettingVault.value = false;
  }
}

/** 选择下载目录。 */
async function pickDownloadDir() {
  try {
    const dir = await pickDirectory("选择下载目录");
    if (dir) form.value.downloadDir = dir;
  } catch (err) {
    pushToast({ tone: "danger", title: "选择目录失败", description: String(err) });
  }
}

/** 删除 Windows 凭据管理器中的 WebDAV 密码；需二次确认。 */
async function clearStoredCredential() {
  if (!form.value.webdavUrl || !form.value.webdavUsername) return;
  const ok = await confirmAction({
    title: "清除已存 WebDAV 密码？",
    description: `将删除「${form.value.webdavUsername} @ ${form.value.webdavUrl}」在系统凭据管理器中的密码。地址与用户名设置会保留。`,
    confirmLabel: "确认清除",
    danger: true,
  });
  if (!ok) return;
  clearingCredential.value = true;
  try {
    await clearWebdavCredential(form.value.webdavUrl, form.value.webdavUsername);
    webdavPassword.value = "";
    pushToast({ tone: "success", title: "已清除 WebDAV 密码" });
  } catch (err) {
    pushToast({ tone: "danger", title: "清除失败", description: String(err) });
  } finally {
    clearingCredential.value = false;
  }
}

/** 用当前表单中的 WebDAV 地址/用户名与已存（或刚输入）密码执行探测。 */
async function testWebdavConnection() {
  if (!form.value.webdavUrl) {
    pushToast({ tone: "warning", title: "请先填写服务器地址" });
    return;
  }
  testingWebdav.value = true;
  webdavTestResult.value = null;
  try {
    const config: WebdavConfigDto = {
      url: form.value.webdavUrl,
      username: form.value.webdavUsername,
      password: webdavPassword.value || undefined,
      vaultId: fullVaultId.value,
    };
    webdavTestResult.value = await testWebdav(config);
  } catch (err) {
    webdavTestResult.value = {
      reachable: false,
      authenticated: false,
      supportMkcol: false,
      supportMove: false,
      message: "连接失败: " + err,
      durationMs: 0,
    };
  } finally {
    testingWebdav.value = false;
  }
}

/** 校验缓存并写入设置；若填写了新密码则写入凭据管理器。调度与采集源在任务页维护。 */
async function save() {
  saving.value = true;
  try {
    const suffix = vaultSuffix.value.trim();
    if (!suffix) {
      throw new Error("Vault ID 后缀不能为空");
    }
    if (!/^[A-Za-z0-9_-]+$/.test(suffix) || suffix.length > 64) {
      throw new Error("Vault ID 后缀只能包含字母、数字、下划线和连字符，最长 64 个字符");
    }
    if (!form.value.deviceName.trim()) {
      throw new Error("设备名称不能为空");
    }
    if (identityConflict.value) {
      throw new Error("当前已关联网盘，改 Vault ID 前请先点「清空关联」");
    }
    for (const value of [
      form.value.copyThresholdMib,
      form.value.cacheRetentionDays,
      form.value.cacheMaxMib,
    ]) {
      if (!Number.isInteger(Number(value)) || Number(value) < 0 || Number(value) > 4294967295) {
        throw new Error("缓存设置必须是 0 到 4294967295 之间的整数");
      }
    }
    form.value.vaultId = fullVaultId.value;
    form.value.deviceName = form.value.deviceName.trim();
    await setAppSettings(form.value);
    if (webdavPassword.value && form.value.webdavUrl && form.value.webdavUsername) {
      await saveWebdavCredential(form.value.webdavUrl, form.value.webdavUsername, webdavPassword.value);
      webdavPassword.value = "";
    }
    await reloadSettings();
    pushToast({ tone: "success", title: "设置已保存" });
  } catch (err) {
    pushToast({ tone: "danger", title: "保存失败", description: String(err) });
  } finally {
    saving.value = false;
  }
}

onMounted(async () => {
  try {
    await reloadSettings();
  } catch (err) {
    pushToast({ tone: "danger", title: "读取设置失败", description: String(err) });
  }
  try {
    stats.value = await getVaultStats();
  } catch {
    stats.value = null;
  }
});
</script>
