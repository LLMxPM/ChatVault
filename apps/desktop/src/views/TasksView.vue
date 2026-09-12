<!--
  ChatVault 任务中枢
  职责：采集范围、调度、立即运行流水线、上传队列与恢复。
-->
<template>
  <div class="flex h-full flex-col gap-4 p-6">
    <div class="flex flex-wrap items-end justify-between gap-3">
      <h2 class="text-cv-page text-cv-text">任务</h2>
      <div class="flex flex-wrap items-center gap-2">
        <span
          class="rounded-cv px-2 py-1 text-cv-caption"
          :class="webdavReady ? 'bg-cv-accent-soft text-cv-accent' : 'bg-cv-surface-2 text-cv-text-3'"
        >
          {{ webdavReady ? "WebDAV 已连接" : "WebDAV 未配置" }}
        </span>
        <UiButton v-if="!webdavReady" size="sm" variant="ghost" @click="navigateTo('settings')">
          去设置连接
        </UiButton>
      </div>
    </div>

    <div class="grid min-h-0 flex-1 grid-cols-1 gap-4 overflow-y-auto pb-2 xl:grid-cols-2">
      <UiCard
        title="采集范围"
        info="定时扫描全部微信账号与附加目录；立即运行可只勾选本次账号。"
      >
        <template #headerExtra>
          <UiButton size="sm" variant="secondary" :loading="detecting" @click="loadAccounts">
            重新检测
          </UiButton>
        </template>

        <div v-if="accounts.length" class="grid gap-2 sm:grid-cols-2">
          <label
            v-for="acc in accounts"
            :key="acc.sourceAccountId"
            class="flex cursor-pointer items-start gap-2 rounded-cv border p-2.5 transition-colors"
            :class="
              selectedAccounts.includes(acc.sourceAccountId)
                ? 'border-cv-accent bg-cv-accent-soft'
                : 'border-cv-border hover:border-cv-text-3'
            "
          >
            <input
              type="checkbox"
              class="mt-0.5 accent-[var(--cv-accent)]"
              :checked="selectedAccounts.includes(acc.sourceAccountId)"
              @change="toggleAccount(acc.sourceAccountId)"
            />
            <div class="min-w-0 flex-1">
              <p class="truncate font-mono text-cv-caption font-medium text-cv-text">{{ acc.sourceAccountId }}</p>
              <p class="mt-0.5 truncate font-mono text-cv-text-3" :title="acc.sourceDir">{{ acc.sourceDir }}</p>
            </div>
          </label>
        </div>
        <p v-else class="py-3 text-cv-caption text-cv-text-3">未探测到微信 4.x 账号</p>

        <div class="mt-4">
          <p class="text-cv-caption text-cv-text-2">附加目录</p>
          <div class="mt-1.5 flex gap-2">
            <div class="min-w-0 flex-1">
              <UiInput v-model="newDir" class="font-mono" placeholder="本地文件夹绝对路径" @keyup.enter="addDir" />
            </div>
            <UiButton variant="secondary" @click="pickAndAddDir">选择</UiButton>
            <UiButton variant="secondary" @click="addDir">添加</UiButton>
          </div>
          <p v-if="!collectDirs.length" class="mt-2 text-cv-caption text-cv-text-3">尚未添加采集目录</p>
          <ul v-else class="mt-2 space-y-1">
            <li
              v-for="(path, idx) in collectDirs"
              :key="path"
              class="flex items-center justify-between gap-2 rounded-cv bg-cv-surface-2 px-2.5 py-1.5"
            >
              <span class="truncate font-mono text-cv-caption text-cv-text-2" :title="path">{{ path }}</span>
              <button class="shrink-0 text-cv-caption text-cv-danger hover:underline" @click="removeDir(idx)">移除</button>
            </li>
          </ul>
        </div>
      </UiCard>

      <UiCard
        title="归档"
        info="扫描采集范围 → 上传 WebDAV → 自动同步元数据。定时与立即归档共用同一条流水线；未配置 WebDAV 时只做本地扫描。"
      >
        <!-- 状态 + 主操作：通用备份卡结构 -->
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div class="min-w-0 flex-1">
            <p class="text-cv-body text-cv-text" :class="statusHeadlineClass">
              {{ statusHeadline }}
            </p>
            <p v-if="statusDetail" class="mt-0.5 text-cv-caption text-cv-text-3">{{ statusDetail }}</p>
          </div>
          <UiButton
            variant="primary"
            :loading="running"
            :disabled="running || !canRun"
            @click="startPipeline"
          >
            {{ running ? "归档中…" : "立即归档" }}
          </UiButton>
        </div>

        <label class="mt-4 flex items-center gap-2 text-cv-caption text-cv-text-3">
          <input v-model="fullScan" type="checkbox" class="accent-[var(--cv-accent)]" />
          强制全量扫描
        </label>

        <div class="mt-3 border-t border-cv-border pt-1">
          <div class="flex items-center justify-between py-2">
            <span class="text-cv-caption text-cv-text-2">自动归档</span>
            <UiSwitch
              :model-value="scheduleEnabled"
              label="自动归档"
              @update:model-value="onScheduleToggle"
            />
          </div>
          <div v-if="scheduleEnabled" class="flex items-center justify-between gap-3 pb-2">
            <span class="text-cv-caption text-cv-text-3">间隔</span>
            <div class="flex items-center gap-1.5 text-cv-caption text-cv-text-2">
              <span>每</span>
              <UiInput
                :model-value="String(scanIntervalMinutes)"
                type="number"
                class="w-20"
                min="5"
                max="1440"
                @update:model-value="onIntervalInput"
              />
              <span>分钟</span>
              <span v-if="scheduleEnabled" class="ml-2" :class="scheduleRegistered ? 'text-cv-success' : 'text-cv-warning'">
                {{ scheduleRegistered ? "已注册" : "注册失败" }}
              </span>
            </div>
          </div>
        </div>
      </UiCard>

      <UiCard
        title="上传队列"
        info="重试只改状态，上传由下次归档执行。列表每 5 秒自动刷新。"
        class="xl:col-span-2"
      >
        <template #headerExtra>
          <div class="flex items-center gap-2">
            <div class="w-36">
              <UiSelect v-model="statusFilter" @change="() => refreshTasks()">
                <option value="">全部状态</option>
                <option value="queued">待上传</option>
                <option value="retryable_failed">可重试失败</option>
                <option value="missing">本地缺失</option>
                <option value="paused">已暂停</option>
                <option value="backed_up">已校验归档</option>
              </UiSelect>
            </div>
            <UiButton size="sm" variant="secondary" :loading="tasksLoading" @click="() => refreshTasks()">刷新</UiButton>
          </div>
        </template>

        <p class="mb-2 text-cv-caption text-cv-text-3">共 {{ tasks.length }} 条</p>
        <div class="max-h-72 overflow-auto rounded-cv border border-cv-border">
          <table class="w-full text-left text-cv-caption">
            <thead class="sticky top-0 border-b border-cv-border bg-cv-surface-2 text-cv-text-2">
              <tr>
                <th class="px-2.5 py-2 font-medium">文件名</th>
                <th class="w-24 px-2.5 py-2 font-medium">状态</th>
                <th class="w-16 px-2.5 py-2 font-medium">大小</th>
                <th class="w-28 px-2.5 py-2 font-medium">操作</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="t in tasks" :key="t.taskId" class="border-b border-cv-border/60 last:border-0">
                <td class="max-w-[180px] truncate px-2.5 py-1.5 text-cv-text" :title="t.originalName">{{ t.originalName }}</td>
                <td class="px-2.5 py-1.5">
                  <UiBadge :tone="statusTone(t.status)">{{ statusLabel(t.status) }}</UiBadge>
                </td>
                <td class="px-2.5 py-1.5 text-cv-text-2">{{ t.formattedSize }}</td>
                <td class="px-2.5 py-1.5">
                  <div class="flex gap-1">
                    <UiButton v-if="canRequeue(t.status)" size="sm" variant="secondary" @click="requeue(t.taskId)">重试</UiButton>
                    <UiButton v-if="canPause(t.status)" size="sm" variant="ghost" @click="pause(t.taskId)">暂停</UiButton>
                  </div>
                </td>
              </tr>
              <tr v-if="!tasksLoading && tasks.length === 0">
                <td colspan="4" class="py-8 text-center text-cv-text-3">暂无任务</td>
              </tr>
            </tbody>
          </table>
        </div>

        <div class="mt-3 flex flex-wrap items-center justify-between gap-2">
          <span class="text-cv-caption text-cv-text-3">换机或重建本地索引时，从 WebDAV 恢复元数据</span>
          <UiButton size="sm" variant="ghost" :disabled="running" @click="doRestore">
            恢复索引
          </UiButton>
        </div>
      </UiCard>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import UiBadge from "../components/ui/UiBadge.vue";
import UiSwitch from "../components/ui/UiSwitch.vue";
import {
  detectWechatAccounts,
  getAppSettings,
  getScheduleStatus,
  listUploadTasks,
  pickDirectory,
  requeueUploadTask,
  pauseUploadTask,
  runPipeline,
  setCollectDirs,
  setScheduleConfig,
  syncRestore,
} from "../api/tauri";
import { pushToast } from "../composables/useToast";
import { confirmAction } from "../composables/useConfirm";
import { navigateTo } from "../composables/useNav";
import type { PipelineResultDto, UploadTaskDto, WechatAccountDto, WebdavConfigDto } from "../types";

type StageId = "idle" | "scan" | "done" | "failed";

const accounts = ref<WechatAccountDto[]>([]);
const selectedAccounts = ref<string[]>([]);
const detecting = ref(false);
const collectDirs = ref<string[]>([]);
const newDir = ref("");
const scheduleEnabled = ref(false);
const scanIntervalMinutes = ref(30);
const scheduleRegistered = ref(false);
const fullScan = ref(false);
const running = ref(false);
const pipelineError = ref("");
const pipelineResult = ref<PipelineResultDto | null>(null);
const stage = ref<StageId>("idle");
const webdavReady = ref(false);
const webdavConfig = ref<WebdavConfigDto>({ url: "", username: "", password: "", vaultId: "default-vault" });

const tasks = ref<UploadTaskDto[]>([]);
const statusFilter = ref("");
const tasksLoading = ref(false);
let pollTimer: number | undefined;
let scheduleSaveTimer: number | undefined;

const canRun = computed(() => selectedAccounts.value.length > 0 || collectDirs.value.length > 0);

const statusHeadline = computed(() => {
  if (running.value) return "归档中";
  if (stage.value === "failed") return "归档失败";
  if (stage.value === "done" && pipelineResult.value) {
    return pipelineResult.value.webdavConfigured ? "归档完成" : "已本地扫描";
  }
  if (!webdavReady.value) return "就绪 · 未连接 WebDAV";
  return "就绪";
});

const statusDetail = computed(() => {
  if (running.value) return "扫描 → 归档 → 同步";
  if (stage.value === "failed") return pipelineError.value || "请检查网络与 WebDAV 配置后重试";
  if (stage.value === "done" && pipelineResult.value) {
    return pipelineResult.value.message + " · 耗时 " + pipelineResult.value.durationMs + " ms";
  }
  if (!webdavReady.value) return "仅本地扫描，不会上传";
  return "";
});

const statusHeadlineClass = computed(() => {
  if (running.value) return "font-medium text-cv-accent";
  if (stage.value === "failed") return "font-medium text-cv-danger";
  return "font-medium";
});

/** 开关即时写入，无保存步骤。 */
async function onScheduleToggle(next: boolean) {
  scheduleEnabled.value = next;
  await saveSchedule();
}

const statusMap: Record<string, string> = {
  queued: "待上传",
  retryable_failed: "可重试失败",
  missing: "本地缺失",
  paused: "已暂停",
  backed_up: "已校验",
};

const toneMap: Record<string, "neutral" | "accent" | "success" | "warning" | "danger"> = {
  queued: "neutral",
  retryable_failed: "danger",
  missing: "warning",
  paused: "warning",
  backed_up: "success",
};

function statusLabel(s: string) {
  return statusMap[s] || s;
}

function statusTone(s: string) {
  return toneMap[s] || "neutral";
}

function canRequeue(s: string) {
  return ["retryable_failed", "missing", "paused"].includes(s);
}

function canPause(s: string) {
  return s === "queued";
}

/** 探测微信账号；默认全选。 */
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

function toggleAccount(accId: string) {
  const idx = selectedAccounts.value.indexOf(accId);
  if (idx >= 0) selectedAccounts.value.splice(idx, 1);
  else selectedAccounts.value.push(accId);
}

async function loadSettings() {
  try {
    const s = await getAppSettings();
    collectDirs.value = s.collectDirs || [];
    scheduleEnabled.value = s.scheduleEnabled;
    scanIntervalMinutes.value = s.scanIntervalMinutes;
    webdavReady.value = Boolean(s.webdavUrl && s.webdavUrl.trim());
    webdavConfig.value = {
      url: s.webdavUrl,
      username: s.webdavUsername,
      password: "",
      vaultId: s.vaultId,
    };
  } catch (err) {
    pushToast({ tone: "danger", title: "读取设置失败", description: String(err) });
  }
  try {
    scheduleRegistered.value = await getScheduleStatus();
  } catch {
    scheduleRegistered.value = false;
  }
}

async function addDir() {
  const p = newDir.value.trim();
  if (!p) return;
  if (collectDirs.value.includes(p)) {
    pushToast({ tone: "warning", title: "目录已存在" });
    return;
  }
  collectDirs.value = [...collectDirs.value, p];
  newDir.value = "";
  await persistDirs();
}

async function pickAndAddDir() {
  try {
    const path = await pickDirectory();
    if (!path || collectDirs.value.includes(path)) return;
    collectDirs.value = [...collectDirs.value, path];
    await persistDirs();
  } catch (err) {
    pushToast({ tone: "danger", title: "选择目录失败", description: String(err) });
  }
}

async function removeDir(index: number) {
  collectDirs.value = collectDirs.value.filter((_, i) => i !== index);
  await persistDirs();
}

async function persistDirs() {
  try {
    await setCollectDirs(collectDirs.value);
  } catch (err) {
    pushToast({ tone: "danger", title: "保存采集目录失败", description: String(err) });
  }
}

function onIntervalInput(raw: string | number) {
  const n = Number(raw);
  scanIntervalMinutes.value = Number.isFinite(n) ? n : 0;
  if (scheduleSaveTimer) window.clearTimeout(scheduleSaveTimer);
  scheduleSaveTimer = window.setTimeout(() => void saveSchedule(), 600);
}

async function saveSchedule() {
  if (scheduleSaveTimer) {
    window.clearTimeout(scheduleSaveTimer);
    scheduleSaveTimer = undefined;
  }
  const interval = Math.min(1440, Math.max(5, Number(scanIntervalMinutes.value) || 30));
  scanIntervalMinutes.value = interval;
  try {
    await setScheduleConfig(scheduleEnabled.value, interval);
    scheduleRegistered.value = await getScheduleStatus();
    pushToast({
      tone: "success",
      title: scheduleEnabled.value ? "定时已启用" : "定时已关闭",
    });
  } catch (err) {
    pushToast({ tone: "danger", title: "保存调度失败", description: String(err) });
    scheduleRegistered.value = await getScheduleStatus().catch(() => false);
  }
}

/** 立即运行：与定时同构的扫描→归档→同步。 */
async function startPipeline() {
  running.value = true;
  pipelineError.value = "";
  pipelineResult.value = null;
  stage.value = "scan";
  try {
    // 有勾选账号时传列表；探测到账号但用户取消全选则空数组=不扫微信
    const targetAccounts = accounts.value.length
      ? selectedAccounts.value.slice()
      : null;
    const result = await runPipeline({
      targetAccounts,
      extraFolders: [],
      fullScan: fullScan.value,
    });
    pipelineResult.value = result;
    stage.value = "done";
    pushToast({
      tone: result.archive && result.archive.failedCount > 0 ? "warning" : "success",
      title: "流水线完成",
      description: result.message,
      action: { label: "去检索", onClick: () => navigateTo("library") },
    });
    await refreshTasks();
  } catch (err) {
    stage.value = "failed";
    pipelineError.value = String(err);
    pushToast({ tone: "danger", title: "流水线失败", description: String(err) });
  } finally {
    running.value = false;
  }
}

async function doRestore() {
  if (!webdavReady.value) {
    pushToast({ tone: "warning", title: "请先配置 WebDAV" });
    navigateTo("settings");
    return;
  }
  const ok = await confirmAction({
    title: "恢复资料库？",
    description: "将从 WebDAV 拉取远端元数据并重建本地索引。本地未同步的元数据可能被覆盖。",
    confirmLabel: "确认恢复",
    danger: true,
  });
  if (!ok) return;
  running.value = true;
  try {
    const r = await syncRestore(webdavConfig.value);
    pushToast({ tone: "success", title: "恢复完成", description: r.message });
    await refreshTasks();
  } catch (err) {
    pushToast({ tone: "danger", title: "恢复失败", description: String(err) });
  } finally {
    running.value = false;
  }
}

let inflight = false;
let lastErrorToastAt = 0;

async function refreshTasks(silent = false) {
  if (inflight) return;
  inflight = true;
  if (!silent) tasksLoading.value = true;
  try {
    tasks.value = await listUploadTasks(statusFilter.value || undefined, 300);
  } catch (err) {
    if (!silent) {
      pushToast({ tone: "danger", title: "加载任务失败", description: String(err) });
    } else {
      const now = Date.now();
      if (now - lastErrorToastAt > 60000) {
        lastErrorToastAt = now;
        pushToast({ tone: "warning", title: "任务列表刷新失败", description: String(err) });
      }
    }
  } finally {
    inflight = false;
    if (!silent) tasksLoading.value = false;
  }
}

async function requeue(taskId: string) {
  try {
    await requeueUploadTask(taskId);
    pushToast({ tone: "success", title: "已重新入队，待下次流水线上传" });
    await refreshTasks();
  } catch (err) {
    pushToast({ tone: "danger", title: "重试失败", description: String(err) });
  }
}

async function pause(taskId: string) {
  try {
    await pauseUploadTask(taskId);
    pushToast({ tone: "success", title: "已暂停" });
    await refreshTasks();
  } catch (err) {
    pushToast({ tone: "danger", title: "暂停失败", description: String(err) });
  }
}

onMounted(async () => {
  await Promise.all([loadAccounts(), loadSettings(), refreshTasks()]);
  pollTimer = window.setInterval(() => {
    void refreshTasks(true);
  }, 5000);
});

onBeforeUnmount(() => {
  if (pollTimer) window.clearInterval(pollTimer);
  if (scheduleSaveTimer) window.clearTimeout(scheduleSaveTimer);
});
</script>
