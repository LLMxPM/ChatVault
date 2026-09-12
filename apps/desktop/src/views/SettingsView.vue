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
      <UiButton variant="primary" :loading="saving" @click="save">保存设置</UiButton>
    </div>

    <div class="min-h-0 flex-1 space-y-4 overflow-y-auto pb-4">
      <UiCard title="身份与资料库" info="Vault ID 标识资料库；设备 ID 由系统生成，不可修改。">
        <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label class="block">
            <span class="text-cv-caption text-cv-text-2">Vault ID</span>
            <UiInput v-model="form.vaultId" class="mt-1 font-mono" />
          </label>
          <label class="block">
            <span class="text-cv-caption text-cv-text-2">设备 ID</span>
            <UiInput v-model="form.deviceId" class="mt-1 font-mono" disabled />
          </label>
        </div>
      </UiCard>

      <UiCard
        title="WebDAV 连接"
        info="任务流水线与换机恢复共用。密码保存在 Windows 凭据管理器；测试用当前表单与已存凭据。"
      >
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

      <CacheSettings v-model="form" />

      <UiCard
        title="下载目录"
        info="仅远程文件「下载」的落点。留空则使用系统「下载\\ChatVault」。"
      >
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
import { onMounted, ref } from "vue";
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
} from "../api/tauri";
import { pushToast } from "../composables/useToast";
import { confirmAction } from "../composables/useConfirm";
import { useTheme, type ThemePreference } from "../composables/useTheme";
import type { AppSettingsDto, VaultStatsDto, WebdavCapabilityDto, WebdavConfigDto } from "../types";

const { preference, setThemePreference } = useTheme();

const themeOptions: { value: ThemePreference; label: string }[] = [
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
  { value: "system", label: "跟随系统" },
];

const form = ref<AppSettingsDto>({
  vaultId: "default-vault",
  deviceId: "",
  webdavUrl: "",
  webdavUsername: "",
  copyThresholdMib: 100,
  cacheRetentionDays: 7,
  cacheMaxMib: 1024,
  downloadDir: "",
  scanIntervalMinutes: 30,
  scheduleEnabled: false,
  collectSources: [],
});

const webdavPassword = ref("");
const saving = ref(false);
const testingWebdav = ref(false);
const clearingCredential = ref(false);
const webdavTestResult = ref<WebdavCapabilityDto | null>(null);
const stats = ref<VaultStatsDto | null>(null);

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
      vaultId: form.value.vaultId,
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
    for (const value of [
      form.value.copyThresholdMib,
      form.value.cacheRetentionDays,
      form.value.cacheMaxMib,
    ]) {
      if (!Number.isInteger(Number(value)) || Number(value) < 0 || Number(value) > 4294967295) {
        throw new Error("缓存设置必须是 0 到 4294967295 之间的整数");
      }
    }
    await setAppSettings(form.value);
    if (webdavPassword.value && form.value.webdavUrl && form.value.webdavUsername) {
      await saveWebdavCredential(form.value.webdavUrl, form.value.webdavUsername, webdavPassword.value);
      webdavPassword.value = "";
    }
    pushToast({ tone: "success", title: "设置已保存" });
  } catch (err) {
    pushToast({ tone: "danger", title: "保存失败", description: String(err) });
  } finally {
    saving.value = false;
  }
}

onMounted(async () => {
  try {
    form.value = await getAppSettings();
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
