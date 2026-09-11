<!--
  ChatVault WebDAV 远端同步与归档视图
  职责：配置 WebDAV 认证信息、执行 RFC4918 协议能力探测，并对本地对象执行流式推送与回读哈希校验。
-->
<template>
  <div class="h-full flex flex-col p-6 space-y-6 overflow-y-auto">
    <!-- 顶部标题 -->
    <div class="border-b border-slate-800 pb-4">
      <h2 class="text-xl font-bold text-white">WebDAV 云端归档与校验</h2>
      <p class="text-xs text-slate-400 mt-1">
        支持私有 NAS（群晖/QNAP）、坚果云、Nextcloud 等任何标准 RFC4918 WebDAV 服务。
      </p>
    </div>

    <!-- 配置表单 -->
    <div class="bg-slate-900/70 border border-slate-800 rounded-xl p-5 space-y-4 max-w-2xl">
      <h3 class="text-sm font-semibold text-slate-200">远端连接参数</h3>

      <div class="space-y-3 text-xs">
        <div>
          <label class="block text-slate-400 mb-1">WebDAV 服务器根地址 (URL)</label>
          <input
            v-model="config.url"
            type="text"
            placeholder="例如: https://dav.example.com/dav/ 或 http://127.0.0.1:8080"
            class="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono"
          />
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block text-slate-400 mb-1">认证用户名</label>
            <input
              v-model="config.username"
              type="text"
              placeholder="admin"
              class="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono"
            />
          </div>
          <div>
            <label class="block text-slate-400 mb-1">认证密码 / 应用令牌</label>
            <input
              v-model="config.password"
              type="password"
              placeholder="已安全存储于系统凭据管理器 (留空则使用已存凭据)"
              class="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono text-xs"
            />
            <p class="text-[10px] text-emerald-500/80 mt-1">密码保存于 Windows 凭据管理器；留空将使用已保存凭据</p>
          </div>
        </div>

        <div>
          <label class="block text-slate-400 mb-1">资料库标识 (Vault ID)</label>
          <input
            v-model="config.vaultId"
            type="text"
            placeholder="default-vault"
            class="w-full bg-slate-950 border border-slate-800 rounded-lg px-3 py-2 text-slate-200 focus:outline-none focus:border-emerald-500 font-mono"
          />
          <p class="text-[11px] text-slate-500 mt-1">云端存储路径：ChatVault/&lt;VaultID&gt;/objects/blake3/...</p>
        </div>
      </div>

      <button class="px-4 py-2 rounded-lg bg-slate-800 text-slate-200 text-xs" :disabled="testing || archiving || syncing" @click="saveConnection">保存连接设置</button>

      <!-- 操作按钮群 -->
      <div class="pt-2 flex items-center space-x-3">
        <button
          class="px-4 py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium transition-colors flex items-center space-x-1.5"
          :disabled="testing || archiving || syncing"
          @click="testConnection"
        >
          <RefreshCw class="w-3.5 h-3.5" :class="{ 'animate-spin': testing }" />
          <span>{{ testing ? "探测中..." : "测试连接能力" }}</span>
        </button>

        <button
          class="px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold transition-colors flex items-center space-x-1.5 shadow"
          :disabled="testing || archiving || syncing"
          @click="startArchive"
        >
          <UploadCloud class="w-3.5 h-3.5" :class="{ 'animate-pulse': archiving }" />
          <span>{{ archiving ? "正在归档并校验..." : "开始归档上传" }}</span>
        </button>
      </div>

      <!-- 元数据同步按钮 -->
      <div class="pt-2 flex items-center space-x-3">
        <button
          class="px-3 py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs"
          :disabled="testing || archiving || syncing"
          @click="doSyncPublish"
        >
          发布本机日志
        </button>
        <button
          class="px-3 py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs"
          :disabled="testing || archiving || syncing"
          @click="doSyncPull"
        >
          拉取远端日志
        </button>
        <button
          class="px-3 py-2 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs"
          :disabled="testing || archiving || syncing"
          @click="doSyncRestore"
        >
          恢复资料库
        </button>
        <span v-if="syncing" class="text-xs text-slate-400">同步中...</span>
        <span v-if="syncMessage" class="text-xs text-emerald-400">{{ syncMessage }}</span>
      </div>

      <!-- 连接测试结果 -->
      <div
        v-if="capability"
        class="p-4 rounded-lg text-xs border"
        :class="
          capability.reachable
            ? 'bg-emerald-950/30 border-emerald-800 text-emerald-300'
            : 'bg-red-950/30 border-red-800 text-red-300'
        "
      >
        <p class="font-semibold">{{ capability.message }}</p>
        <div v-if="capability.reachable" class="mt-2 space-y-1 text-[11px] text-slate-300">
          <p>服务端标识: {{ capability.serverHeader || "未知" }}</p>
          <p>RFC4918 动词合规级别: {{ capability.davCompliance.join(", ") || "基础" }}</p>
          <p>LOCK / UNLOCK：未探测</p>
        </div>
      </div>

      <!-- 归档结果 -->
      <div
        v-if="archiveResult"
        class="p-4 rounded-lg text-xs border bg-slate-950 border-emerald-800 text-slate-200 space-y-1"
      >
        <p class="font-semibold text-emerald-400">两阶段归档（Staging $\rightarrow$ 回读比对 $\rightarrow$ 原子发布）完成</p>
        <p>成功发布对象: {{ archiveResult.uploadedCount }}</p>
        <p>远端流式哈希校验通过: {{ archiveResult.verifiedCount }}</p>
        <p>失败对象: {{ archiveResult.failedCount }}</p>
        <p class="text-slate-500 text-[10px]">总耗时: {{ archiveResult.durationMs }} ms</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { RefreshCw, UploadCloud } from "lucide-vue-next";
import {
  testWebdav,
  archiveToWebdav,
  getAppSettings,
  saveWebdavConfig,
  syncPublish,
  syncPull,
  syncRestore,
} from "../api/tauri";
import type { WebdavConfigDto, WebdavCapabilityDto, ArchiveResultDto } from "../types";

const config = ref<WebdavConfigDto>({
  url: "",
  username: "",
  password: "",
  vaultId: "default-vault",
});

const testing = ref(false);
const capability = ref<WebdavCapabilityDto | null>(null);

const archiving = ref(false);
const archiveResult = ref<ArchiveResultDto | null>(null);

const syncing = ref(false);
const syncMessage = ref("");

onMounted(async () => {
  try {
    const saved = await getAppSettings();
    config.value = { url:saved.webdavUrl, username:saved.webdavUsername, vaultId:saved.vaultId, password:"" };
  } catch (err) { syncMessage.value = '读取连接配置失败：' + err; }
});

/** 保存连接供手动同步和计划任务共同使用；密码留空时使用系统凭据库。 */
async function persistConnection() {
  await saveWebdavConfig(config.value);
}

/** 保存当前连接并提示结果。 */
async function saveConnection() {
  try { await persistConnection(); syncMessage.value = '连接设置已保存'; }
  catch (err) { syncMessage.value = '保存失败：' + err; }
}

async function doSyncPublish() {
  syncing.value = true;
  syncMessage.value = "";
  try {
    await persistConnection();
    const r = await syncPublish(config.value);
    syncMessage.value = r.message;
  } catch (err) {
    syncMessage.value = `发布失败: ${err}`;
  } finally {
    syncing.value = false;
  }
}

async function doSyncPull() {
  syncing.value = true;
  syncMessage.value = "";
  try {
    await persistConnection();
    const r = await syncPull(config.value);
    syncMessage.value = r.message;
  } catch (err) {
    syncMessage.value = `拉取失败: ${err}`;
  } finally {
    syncing.value = false;
  }
}

async function doSyncRestore() {
  syncing.value = true;
  syncMessage.value = "";
  try {
    await persistConnection();
    const r = await syncRestore(config.value);
    syncMessage.value = r.message;
  } catch (err) {
    syncMessage.value = `恢复失败: ${err}`;
  } finally {
    syncing.value = false;
  }
}

/**
 * 测试 WebDAV 连通性与协议能力
 */
async function testConnection() {
  testing.value = true;
  capability.value = null;
  try {
    await persistConnection();
    const res = await testWebdav(config.value);
    capability.value = res;
  } catch (err) {
    capability.value = {
      reachable: false,
      davCompliance: [],
      supportsLock: false,
      message: "连接失败: " + err,
    };
  } finally {
    testing.value = false;
  }
}

/**
 * 触发对象归档上传
 */
async function startArchive() {
  archiving.value = true;
  archiveResult.value = null;
  try {
    await persistConnection();
    const res = await archiveToWebdav(config.value);
    archiveResult.value = res;
  } catch (err) {
    alert("归档失败: " + err);
  } finally {
    archiving.value = false;
  }
}
</script>
