<!--
  ChatVault 设置视图
  职责：配置 Vault/设备身份、WebDAV 连接、定时扫描周期与采集目录，并持久化到本地索引。
-->
<template>
  <div class="h-full flex flex-col p-6 space-y-6 overflow-y-auto">
    <div class="border-b border-slate-800 pb-4">
      <h2 class="text-xl font-bold text-white">设置</h2>
      <p class="text-xs text-slate-400 mt-1">
        配置本机身份、采集目录、定时扫描与 WebDAV 归档参数。设置保存在本地 SQLite。
      </p>
    </div>

    <div class="bg-slate-900/70 border border-slate-800 rounded-xl p-5 space-y-4 max-w-2xl">
      <h3 class="text-sm font-semibold text-slate-200">身份与资料库</h3>
      <div class="grid grid-cols-2 gap-3 text-xs">
        <div>
          <label class="block text-slate-400 mb-1">Vault ID</label>
          <input
            v-model="form.vaultId"
            type="text"
            class="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono"
          />
        </div>
        <div>
          <label class="block text-slate-400 mb-1">设备 ID</label>
          <input
            v-model="form.deviceId"
            readonly
            type="text"
            class="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono"
          />
        </div>
      </div>
    </div>

    <div class="bg-slate-900/70 border border-slate-800 rounded-xl p-5 space-y-4 max-w-2xl">
      <h3 class="text-sm font-semibold text-slate-200">采集与定时任务</h3>
      <div class="space-y-3 text-xs">
        <div>
          <label class="block text-slate-400 mb-1">采集目录（每行一个绝对路径）</label>
          <textarea
            v-model="collectDirsText"
            rows="3"
            placeholder="例如: D:\WeChatFiles&#10;C:\Users\you\Documents\xwechat_files"
            class="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono"
          />
        </div>
        <div class="flex items-center space-x-3">
          <label class="flex items-center space-x-2 text-slate-300">
            <input v-model="form.scheduleEnabled" type="checkbox" class="accent-emerald-500" />
            <span>启用系统定时扫描</span>
          </label>
          <div class="flex items-center space-x-2">
            <label class="text-slate-400">周期（分钟）</label>
            <input
              v-model.number="form.scanIntervalMinutes"
              type="number"
              min="5"
              max="1440"
              class="w-24 bg-slate-950 border border-slate-800 rounded-lg px-3 py-1.5 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono"
            />
          </div>
        </div>
        <p class="text-[11px] text-slate-500">
          不依赖文件系统监听。启用后将注册 Windows 计划任务，按周期拉起扫描与归档。
          <span v-if="scheduleRegistered" class="text-emerald-400">当前已注册计划任务。</span>
          <span v-else class="text-slate-400">当前未注册计划任务。</span>
        </p>
      </div>
    </div>

    <div class="bg-slate-900/70 border border-slate-800 rounded-xl p-5 space-y-4 max-w-2xl">
      <h3 class="text-sm font-semibold text-slate-200">WebDAV 连接</h3>
      <div class="grid grid-cols-2 gap-3 text-xs">
        <div class="col-span-2">
          <label class="block text-slate-400 mb-1">服务器地址</label>
          <input
            v-model="form.webdavUrl"
            type="text"
            placeholder="https://dav.example.com/dav/"
            class="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono"
          />
        </div>
        <div>
          <label class="block text-slate-400 mb-1">用户名</label>
          <input
            v-model="form.webdavUsername"
            type="text"
            class="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono"
          />
        </div>
      </div>
    </div>

    <CacheSettings v-model="form" />

    <div class="flex items-center space-x-3 max-w-2xl">
      <button
        class="px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold transition-colors"
        :disabled="saving"
        @click="save"
      >
        {{ saving ? "保存中..." : "保存设置" }}
      </button>
      <span v-if="message" class="text-xs" :class="messageOk ? 'text-emerald-400' : 'text-red-400'">
        {{ message }}
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import CacheSettings from "../components/CacheSettings.vue";
import { getAppSettings, setAppSettings, getScheduleStatus } from "../api/tauri";
import type { AppSettingsDto } from "../types";

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

const collectDirsText = ref("");
const saving = ref(false);
const message = ref("");
const messageOk = ref(true);
const scheduleRegistered = ref(false);

const dirsFromText = computed(() =>
  collectDirsText.value
    .split("\n")
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
);

onMounted(async () => {
  try {
    const s = await getAppSettings();
    form.value = s;
    collectDirsText.value = s.collectDirs.join("\n");
    scheduleRegistered.value = await getScheduleStatus();
  } catch (err) {
    message.value = `读取设置失败: ${err}`;
    messageOk.value = false;
  }
});

/** 校验缓存配置并保存所有本机设置。 */
async function save() {
  saving.value = true;
  message.value = "";
  try {
    for (const value of [form.value.copyThresholdMib, form.value.cacheRetentionDays, form.value.cacheMaxMib]) {
      if (!Number.isInteger(value) || value < 0 || value > 4294967295) {
        throw new Error("缓存设置必须是 0 到 4294967295 之间的整数");
      }
    }
    form.value.collectDirs = dirsFromText.value;
    await setAppSettings(form.value);
    scheduleRegistered.value = await getScheduleStatus();
    message.value = "设置已保存";
    messageOk.value = true;
  } catch (err) {
    message.value = `保存失败: ${err}`;
    messageOk.value = false;
  } finally {
    saving.value = false;
  }
}
</script>
