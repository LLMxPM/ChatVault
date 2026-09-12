<!--
  ChatVault 设置视图
  职责：身份、采集目录、定时任务、WebDAV 配置、缓存、外观主题与关于信息。
-->
<template>
  <div class="flex h-full flex-col gap-4 p-6">
    <div class="flex flex-wrap items-end justify-between gap-3">
      <div>
        <h2 class="text-cv-page text-cv-text">设置</h2>
        <p class="mt-0.5 text-cv-caption text-cv-text-2">本机身份、采集、归档连接与外观</p>
      </div>
      <UiButton variant="primary" :loading="saving" @click="save">保存设置</UiButton>
    </div>

    <div class="min-h-0 flex-1 space-y-4 overflow-y-auto pb-4">
      <UiCard title="身份与资料库">
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

      <UiCard title="采集与定时" description="采集目录与「采集」页共用；定时任务由系统计划程序拉起">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <span class="text-cv-caption text-cv-text-2">采集目录</span>
          <UiButton size="sm" variant="secondary" :loading="picking" @click="addCollectDir">选择文件夹</UiButton>
        </div>
        <p v-if="!form.collectDirs.length" class="mt-2 text-cv-caption text-cv-text-3">尚未添加采集目录</p>
        <ul v-else class="mt-2 space-y-1">
          <li
            v-for="(path, idx) in form.collectDirs"
            :key="path"
            class="flex items-center justify-between gap-2 rounded-cv bg-cv-surface-2 px-2.5 py-1.5"
          >
            <span class="truncate font-mono text-cv-caption text-cv-text-2" :title="path">{{ path }}</span>
            <button class="shrink-0 text-cv-caption text-cv-danger hover:underline" @click="removeCollectDir(idx)">移除</button>
          </li>
        </ul>
        <div class="mt-4 flex flex-wrap items-center gap-x-4 gap-y-2">
          <label class="flex items-center gap-2 whitespace-nowrap text-cv-caption text-cv-text-2">
            <input v-model="form.scheduleEnabled" type="checkbox" class="accent-[var(--cv-accent)]" />
            启用系统定时扫描
          </label>
          <label class="flex items-center gap-2 whitespace-nowrap text-cv-caption text-cv-text-2">
            周期（分钟）
            <UiInput
              :model-value="form.scanIntervalMinutes"
              type="number"
              class="w-24"
              min="5"
              max="1440"
              @update:model-value="(v) => (form.scanIntervalMinutes = Number(v) || 0)"
            />
          </label>
        </div>
        <p class="mt-2 text-cv-caption text-cv-text-3">
          <span v-if="scheduleRegistered" class="text-cv-success">当前已注册计划任务。</span>
          <span v-else>当前未注册计划任务。</span>
        </p>
      </UiCard>

      <UiCard title="WebDAV 连接" description="归档页读取此处配置；密码保存在 Windows 凭据管理器">
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
              <span class="text-cv-caption text-cv-text-2">新密码（留空则保留已存凭据）</span>
              <UiInput v-model="webdavPassword" type="password" class="mt-1" />
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
            <span class="text-cv-caption text-cv-text-3">测试用当前表单与已存凭据；清除仅删除系统凭据管理器中的密码</span>
          </div>
          <WebdavTestResult v-if="webdavTestResult" :capability="webdavTestResult" />
        </div>
      </UiCard>

      <CacheSettings v-model="form" />

      <UiCard title="外观" description="默认浅色；可固定深色或跟随系统">
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
  getScheduleStatus,
  saveWebdavCredential,
  clearWebdavCredential,
  getVaultStats,
  pickDirectory,
  testWebdav,
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
  scanIntervalMinutes: 30,
  scheduleEnabled: false,
  collectDirs: [],
});

const webdavPassword = ref("");
const saving = ref(false);
const picking = ref(false);
const testingWebdav = ref(false);
const clearingCredential = ref(false);
const webdavTestResult = ref<WebdavCapabilityDto | null>(null);
const scheduleRegistered = ref(false);
const stats = ref<VaultStatsDto | null>(null);

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

/** 弹出系统目录选择框并加入采集目录；取消或重复路径不写入。 */
async function addCollectDir() {
  picking.value = true;
  try {
    const path = await pickDirectory();
    if (!path) return;
    if (form.value.collectDirs.includes(path)) {
      pushToast({ tone: "warning", title: "目录已存在" });
      return;
    }
    form.value.collectDirs = [...form.value.collectDirs, path];
  } catch (err) {
    pushToast({ tone: "danger", title: "选择目录失败", description: String(err) });
  } finally {
    picking.value = false;
  }
}

/** 按索引移除采集目录，保存前只改本地表单。 */
function removeCollectDir(index: number) {
  form.value.collectDirs = form.value.collectDirs.filter((_, i) => i !== index);
}

/** 校验缓存与计划周期并写入设置；若填写了新密码则写入凭据管理器。 */
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
    const interval = Number(form.value.scanIntervalMinutes);
    if (!Number.isInteger(interval) || interval < 5 || interval > 1440) {
      throw new Error("扫描周期需在 5–1440 分钟之间");
    }
    form.value.scanIntervalMinutes = interval;
    await setAppSettings(form.value);
    if (webdavPassword.value && form.value.webdavUrl && form.value.webdavUsername) {
      await saveWebdavCredential(form.value.webdavUrl, form.value.webdavUsername, webdavPassword.value);
      webdavPassword.value = "";
    }
    scheduleRegistered.value = await getScheduleStatus();
    pushToast({ tone: "success", title: "设置已保存" });
  } catch (err) {
    pushToast({ tone: "danger", title: "保存失败", description: String(err) });
  } finally {
    saving.value = false;
  }
}

onMounted(async () => {
  try {
    const s = await getAppSettings();
    form.value = s;
    scheduleRegistered.value = await getScheduleStatus();
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
