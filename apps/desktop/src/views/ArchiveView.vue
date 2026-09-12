<!--
  ChatVault 归档视图
  职责：WebDAV 连接状态、能力探测、对象归档、元数据发布/拉取与恢复。配置在设置页维护。
-->
<template>
  <div class="flex h-full flex-col gap-4 p-6">
    <div class="flex flex-wrap items-end justify-between gap-3">
      <div>
        <h2 class="text-cv-page text-cv-text">归档</h2>
        <p class="mt-0.5 text-cv-caption text-cv-text-2">
          将本地对象推送到 WebDAV，同步元数据并支持换机恢复
        </p>
      </div>
      <UiButton size="sm" variant="ghost" @click="navigateTo('settings')">去设置修改连接</UiButton>
    </div>

    <UiCard title="连接摘要">
      <dl class="grid grid-cols-1 gap-2 text-cv-body sm:grid-cols-2">
        <div>
          <dt class="text-cv-caption text-cv-text-3">服务器</dt>
          <dd class="mt-0.5 break-all font-mono text-cv-text-2">{{ connection.url || "未配置" }}</dd>
        </div>
        <div>
          <dt class="text-cv-caption text-cv-text-3">用户名 / Vault</dt>
          <dd class="mt-0.5 font-mono text-cv-text-2">
            {{ connection.username || "—" }} · {{ connection.vaultId || "—" }}
          </dd>
        </div>
      </dl>
      <p class="mt-3 text-cv-caption text-cv-text-3">密码保存在 Windows 凭据管理器，此处不回显。</p>
    </UiCard>

    <UiCard title="操作">
      <div class="flex flex-wrap gap-2">
        <UiButton variant="secondary" :loading="testing" :disabled="busy" @click="testConnection">
          测试连接
        </UiButton>
        <UiButton variant="primary" :loading="archiving" :disabled="busy" @click="startArchive">
          开始归档上传
        </UiButton>
        <UiButton variant="secondary" :disabled="busy" @click="doSyncPublish">发布本机日志</UiButton>
        <UiButton variant="secondary" :disabled="busy" @click="doSyncPull">拉取远端日志</UiButton>
        <UiButton variant="danger" :disabled="busy" @click="doSyncRestore">恢复资料库</UiButton>
      </div>
    </UiCard>

    <UiCard v-if="capability" title="连接探测" description="对远端执行创建、上传、列举、移动与回读校验">
      <WebdavTestResult :capability="capability" />
    </UiCard>

    <UiCard v-if="archiveResult" title="归档结果">
      <div class="grid grid-cols-3 gap-3 text-cv-body">
        <div>
          <p class="text-cv-caption text-cv-text-3">发布对象</p>
          <p class="text-lg font-semibold text-cv-text">{{ archiveResult.uploadedCount }}</p>
        </div>
        <div>
          <p class="text-cv-caption text-cv-text-3">校验通过</p>
          <p class="text-lg font-semibold text-cv-success">{{ archiveResult.verifiedCount }}</p>
        </div>
        <div>
          <p class="text-cv-caption text-cv-text-3">失败</p>
          <p class="text-lg font-semibold text-cv-danger">{{ archiveResult.failedCount }}</p>
        </div>
      </div>
      <p class="mt-2 text-cv-caption text-cv-text-3">耗时 {{ archiveResult.durationMs }} ms</p>
    </UiCard>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import WebdavTestResult from "../components/WebdavTestResult.vue";
import {
  testWebdav,
  archiveToWebdav,
  getAppSettings,
  syncPublish,
  syncPull,
  syncRestore,
} from "../api/tauri";
import { pushToast } from "../composables/useToast";
import { confirmAction } from "../composables/useConfirm";
import { navigateTo } from "../composables/useNav";
import type { WebdavConfigDto, WebdavCapabilityDto, ArchiveResultDto } from "../types";

const connection = ref<WebdavConfigDto>({ url: "", username: "", password: "", vaultId: "default-vault" });
const testing = ref(false);
const capability = ref<WebdavCapabilityDto | null>(null);
const archiving = ref(false);
const archiveResult = ref<ArchiveResultDto | null>(null);
const syncing = ref(false);

const busy = computed(() => testing.value || archiving.value || syncing.value);

/** 从设置加载连接（不含密码）。 */
async function loadConnection() {
  try {
    const saved = await getAppSettings();
    connection.value = {
      url: saved.webdavUrl,
      username: saved.webdavUsername,
      vaultId: saved.vaultId,
      password: "",
    };
  } catch (err) {
    pushToast({ tone: "danger", title: "读取连接配置失败", description: String(err) });
  }
}

/** 探测 WebDAV 能力；连接配置只读自设置，不在归档页回写。 */
async function testConnection() {
  testing.value = true;
  capability.value = null;
  try {
    capability.value = await testWebdav(connection.value);
  } catch (err) {
    capability.value = {
      reachable: false,
      authenticated: false,
      supportMkcol: false,
      supportMove: false,
      message: "连接失败: " + err,
      durationMs: 0,
    };
  } finally {
    testing.value = false;
  }
}

/** 归档上传。 */
async function startArchive() {
  archiving.value = true;
  archiveResult.value = null;
  try {
    archiveResult.value = await archiveToWebdav(connection.value);
    pushToast({
      tone: archiveResult.value.failedCount > 0 ? "warning" : "success",
      title: "归档完成",
      description: `成功 ${archiveResult.value.uploadedCount}，失败 ${archiveResult.value.failedCount}`,
    });
  } catch (err) {
    pushToast({ tone: "danger", title: "归档失败", description: String(err) });
  } finally {
    archiving.value = false;
  }
}

/** 发布本机元数据日志。 */
async function doSyncPublish() {
  syncing.value = true;
  try {
    const r = await syncPublish(connection.value);
    pushToast({ tone: "success", title: "发布完成", description: r.message });
  } catch (err) {
    pushToast({ tone: "danger", title: "发布失败", description: String(err) });
  } finally {
    syncing.value = false;
  }
}

/** 拉取远端日志并合并。 */
async function doSyncPull() {
  syncing.value = true;
  try {
    const r = await syncPull(connection.value);
    pushToast({ tone: "success", title: "拉取完成", description: r.message });
  } catch (err) {
    pushToast({ tone: "danger", title: "拉取失败", description: String(err) });
  } finally {
    syncing.value = false;
  }
}

/** 从远端恢复资料库；需二次确认。 */
async function doSyncRestore() {
  const ok = await confirmAction({
    title: "恢复资料库？",
    description: "将从 WebDAV 拉取远端元数据并重建本地索引。本地未同步的元数据可能被覆盖。",
    confirmLabel: "确认恢复",
    danger: true,
  });
  if (!ok) return;
  syncing.value = true;
  try {
    const r = await syncRestore(connection.value);
    pushToast({ tone: "success", title: "恢复完成", description: r.message });
  } catch (err) {
    pushToast({ tone: "danger", title: "恢复失败", description: String(err) });
  } finally {
    syncing.value = false;
  }
}

onMounted(loadConnection);
</script>
