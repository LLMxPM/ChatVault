<!--
  ChatVault 任务中枢
  职责：采集范围、调度、立即运行流水线、归档队列与恢复。
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
        info="定时扫描全部已配置采集源；立即运行可只勾选本次微信账号。"
      >
        <template #headerExtra>
          <div class="flex flex-wrap justify-end gap-2">
            <UiButton size="sm" variant="secondary" @click="pickAndAddWechat">
              添加微信 4.x
            </UiButton>
            <UiButton size="sm" variant="secondary" @click="pickAndAddAttachment">
              添加附件目录
            </UiButton>
            <UiButton size="sm" variant="ghost" :loading="detecting" @click="discoverAndAddWechat">
              自动发现
            </UiButton>
          </div>
        </template>

        <div v-if="collectSources.length" class="space-y-2">
          <div
            v-for="(source, index) in collectSources"
            :key="sourceKey(source)"
            class="rounded-cv border border-cv-border p-2.5"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <UiBadge :tone="source.sourceType === 'wechat-windows-4' ? 'accent' : 'neutral'">
                    {{ sourceTypeLabel(source.sourceType) }}
                  </UiBadge>
                  <UiBadge :tone="sourceStatusTone(source.status)">{{ sourceStatusLabel(source.status) }}</UiBadge>
                </div>
                <p class="mt-1 truncate font-mono text-cv-caption text-cv-text" :title="source.path">
                  {{ source.path }}
                </p>
                <p class="mt-0.5 text-cv-caption text-cv-text-3">{{ sourceStatusDetail(source) }}</p>
                <label
                  v-if="source.sourceType === 'wechat-windows-4'"
                  class="mt-2 inline-flex cursor-pointer items-center gap-2 text-cv-caption text-cv-text-2"
                >
                  <input
                    type="checkbox"
                    class="h-4 w-4 accent-[var(--cv-accent)]"
                    :checked="source.enableImages === true"
                    @change="toggleSourceImages(source)"
                  />
                  <span>启用聊天图片解密</span>
                  <span class="text-cv-text-3">仅本机解密并备份可打开图片，不保存微信密钥</span>
                </label>
              </div>
              <div class="flex shrink-0 items-center gap-1">
                <UiButton
                  size="sm"
                  variant="ghost"
                  :loading="source.inspecting"
                  @click="source.status === 'missing' ? pickAndReplaceSource(index) : refreshSource(index)"
                >
                  {{ source.status === 'missing' ? '重新选择' : source.sourceType === 'wechat-windows-4' ? '重新识别' : '重新检查' }}
                </UiButton>
                <UiButton size="sm" variant="ghost" @click="removeSource(index)">移除</UiButton>
              </div>
            </div>

            <div
              v-if="source.sourceType === 'wechat-windows-4' && source.accounts.length"
              class="mt-2 grid gap-1.5 sm:grid-cols-2"
            >
              <label
                v-for="account in source.accounts"
                :key="accountKey(account)"
                class="flex cursor-pointer items-start gap-2 rounded-cv bg-cv-surface-2 px-2 py-1.5"
              >
                <input
                  type="checkbox"
                  class="mt-0.5 accent-[var(--cv-accent)]"
                  :checked="isAccountSelected(account)"
                  @change="toggleAccount(account)"
                />
                <span class="min-w-0">
                  <span class="block truncate font-mono text-cv-caption text-cv-text">{{ account.sourceAccountId }}</span>
                  <span class="block text-cv-caption text-cv-text-3">
                    附件 {{ account.filesCountEstimated }} · 视频 {{ account.videosCountEstimated ?? 0 }} · 图片 {{ account.imagesCountEstimated ?? 0 }}
                  </span>
                </span>
              </label>
            </div>
          </div>
        </div>
        <p v-else class="py-3 text-cv-caption text-cv-text-3">尚未添加采集源，请从上方选择目录。</p>
      </UiCard>

      <UiCard
        title="归档"
        info="扫描采集范围 → 上传 WebDAV → 自动同步元数据。定时与立即归档共用同一条流水线；未配置 WebDAV 时只做本地扫描。"
      >
        <div class="space-y-3">
          <div class="rounded-cv-lg border border-cv-border bg-cv-surface-2 px-3.5 py-3">
            <div class="grid gap-3 sm:grid-cols-2">
              <div class="flex items-center justify-between gap-3">
                <div class="min-w-0">
                  <p class="text-cv-caption font-medium text-cv-text-2">执行间隔</p>
                  <p class="mt-1 text-cv-caption text-cv-text-3">定时任务的自动归档频率</p>
                </div>
                <div class="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-cv-caption text-cv-text-2">
                  <span>每</span>
                  <UiInput
                    id="scan-interval"
                    :model-value="scanIntervalHours"
                    type="number"
                    class="w-16"
                    min="0.5"
                    :max="maxScanIntervalHours"
                    step="0.5"
                    @update:model-value="onIntervalInput"
                  />
                  <span>小时</span>
                </div>
              </div>

              <div class="flex items-center justify-between gap-3 sm:border-l sm:border-cv-border sm:pl-4">
                <div class="min-w-0">
                  <p class="text-cv-caption font-medium text-cv-text-2">自动归档</p>
                  <p class="mt-1 text-cv-caption text-cv-text-3">按固定周期自动扫描并归档</p>
                  <p
                    v-if="scheduleEnabled"
                    class="mt-1 whitespace-nowrap text-cv-caption"
                    :class="scheduleRegistered ? 'text-cv-success' : 'text-cv-warning'"
                  >
                    {{ scheduleRegistered ? "计划已注册" : "注册失败" }}
                  </p>
                </div>
                <UiSwitch
                  :model-value="scheduleEnabled"
                  label="自动归档"
                  @update:model-value="onScheduleToggle"
                />
              </div>
            </div>
          </div>

          <div class="flex flex-wrap items-center justify-between gap-3 rounded-cv-lg border border-cv-border px-3.5 py-2.5">
            <label class="flex min-w-0 cursor-pointer items-center gap-3">
              <input
                v-model="fullScan"
                type="checkbox"
                class="h-4 w-4 shrink-0 accent-[var(--cv-accent)]"
                :disabled="running"
              />
              <span class="min-w-0">
                <span class="block text-cv-caption font-medium text-cv-text-2">本次强制全量扫描</span>
                <span class="mt-0.5 block text-cv-caption text-cv-text-3">忽略增量记录，重新检查全部附件</span>
              </span>
            </label>
            <UiButton
              v-if="running || activeRun"
              class="shrink-0"
              variant="danger"
              :loading="cancelling"
              @click="requestCancelRun"
            >
              结束运行
            </UiButton>
            <UiButton
              v-else
              class="shrink-0"
              variant="primary"
              :disabled="!canRun"
              @click="startPipeline"
            >
              立即运行
            </UiButton>
          </div>

          <div class="flex flex-wrap items-center gap-3 rounded-cv-lg border border-cv-border bg-cv-surface-2 px-3.5 py-3">
            <div class="flex min-w-0 flex-1 items-start gap-3">
              <span
                class="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-full"
                :class="statusIndicatorClass"
                aria-hidden="true"
              >
                <span class="h-2 w-2 rounded-full bg-current" :class="running || activeRun ? 'animate-pulse' : ''" />
              </span>
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-2">
                  <p class="text-cv-body text-cv-text" :class="statusHeadlineClass">{{ statusHeadline }}</p>
                  <UiBadge :tone="statusBadgeTone">{{ statusBadge }}</UiBadge>
                  <template v-if="running && liveProgress.total > 0">
                    <span class="text-cv-caption text-cv-text-2">
                      {{ liveProgress.done }}/{{ liveProgress.total }}
                    </span>
                  </template>
                  <template v-else-if="!running && activeRun?.total">
                    <span class="text-cv-caption text-cv-text-2">
                      {{ activeRun.done ?? 0 }}/{{ activeRun.total }}
                    </span>
                  </template>
                </div>
                <p v-if="statusDetail" class="mt-0.5 max-w-2xl break-words text-cv-caption text-cv-text-3">{{ statusDetail }}</p>
                <p
                  v-if="running && liveProgress.currentName"
                  class="mt-0.5 max-w-2xl truncate font-mono text-cv-caption text-cv-text-2"
                  :title="liveProgress.currentName"
                >
                  当前：{{ liveProgress.currentName }}
                </p>
                <p
                  v-else-if="!running && activeRun?.currentName"
                  class="mt-0.5 max-w-2xl truncate font-mono text-cv-caption text-cv-text-2"
                  :title="activeRun.currentName"
                >
                  当前：{{ activeRun.currentName }}
                </p>
                <div v-if="running && liveProgress.total > 0" class="mt-2 h-1.5 w-full max-w-md overflow-hidden rounded-full bg-cv-surface">
                  <div
                    class="h-full rounded-full bg-cv-accent transition-all"
                    :style="{ width: progressPercent + '%' }"
                  />
                </div>
                <div v-else-if="!running && activeRun?.total" class="mt-2 h-1.5 w-full max-w-md overflow-hidden rounded-full bg-cv-surface">
                  <div
                    class="h-full rounded-full bg-cv-accent transition-all"
                    :style="{ width: activeRunProgressPercent + '%' }"
                  />
                </div>
              </div>
            </div>
          </div>
        </div>
      </UiCard>

      <UiCard
        title="运行历史"
        info="最近运行（含定时 CLI）；展开可看阶段与失败明细。保留最近 50 次或 30 天。"
        class="xl:col-span-2"
      >
        <template #headerExtra>
          <UiButton size="sm" variant="secondary" :loading="historyLoading" @click="() => refreshHistory()">
            刷新
          </UiButton>
        </template>

        <p v-if="!historyLoading && !runs.length" class="py-4 text-cv-caption text-cv-text-3">
          暂无运行记录，执行一次「立即归档」后可在此回看。
        </p>
        <div v-else class="space-y-2">
          <div
            v-for="run in runs"
            :key="run.runId"
            class="rounded-cv border border-cv-border"
          >
            <button
              type="button"
              class="flex w-full flex-wrap items-center gap-2 px-3 py-2 text-left hover:bg-cv-surface-2"
              @click="toggleRunDetail(run.runId)"
            >
              <UiBadge :tone="runStatusTone(run.status)">{{ runStatusLabel(run.status) }}</UiBadge>
              <span class="text-cv-caption text-cv-text">{{ formatDateTime(run.startedAt) }}</span>
              <UiBadge tone="neutral">{{ run.triggerSource === "schedule" ? "定时" : "手动" }}</UiBadge>
              <span class="min-w-0 flex-1 truncate text-cv-caption text-cv-text-2" :title="run.summaryJson || run.errorMessage || ''">
                {{ runSummaryText(run) }}
              </span>
              <span v-if="run.failedItems > 0" class="text-cv-caption text-cv-danger">
                失败 {{ run.failedItems }}
              </span>
              <span v-if="run.durationMs != null" class="text-cv-caption text-cv-text-3">
                {{ formatDurationMs(run.durationMs) }}
              </span>
              <span class="text-cv-caption text-cv-text-3">{{ expandedRunId === run.runId ? "收起" : "展开" }}</span>
            </button>

            <div v-if="expandedRunId === run.runId" class="border-t border-cv-border px-3 py-2.5">
              <p v-if="detailLoading" class="text-cv-caption text-cv-text-3">加载中…</p>
              <template v-else-if="detail">
                <div class="mb-3 flex flex-wrap gap-1.5">
                  <span
                    v-for="st in detail.stages"
                    :key="st.stage"
                    class="rounded-cv bg-cv-surface-2 px-2 py-1 text-cv-caption text-cv-text-2"
                    :title="st.message || ''"
                  >
                    {{ stageLabel(st.stage) }}
                    <span :class="stageStatusClass(st.status)">{{ stageStatusLabel(st.status) }}</span>
                    <span v-if="st.durationMs != null" class="text-cv-text-3"> · {{ formatDurationMs(st.durationMs) }}</span>
                  </span>
                </div>

                <div class="mb-2 flex flex-wrap items-center gap-2">
                  <span class="text-cv-caption text-cv-text-2">明细</span>
                  <div class="w-32">
                    <UiSelect v-model="itemStatusFilter">
                      <option value="">全部</option>
                      <option value="failed">上传失败</option>
                      <option value="missing">本地缺失</option>
                      <option value="decrypt_failed">解密失败</option>
                    </UiSelect>
                  </div>
                  <span class="text-cv-caption text-cv-text-3">共 {{ filteredDetailItems.length }} 条</span>
                </div>

                <div v-if="filteredDetailItems.length" class="max-h-48 overflow-auto rounded-cv border border-cv-border">
                  <table class="w-full text-left text-cv-caption">
                    <thead class="sticky top-0 border-b border-cv-border bg-cv-surface-2 text-cv-text-2">
                      <tr>
                        <th class="px-2 py-1.5 font-medium">文件</th>
                        <th class="w-24 px-2 py-1.5 font-medium">状态</th>
                        <th class="px-2 py-1.5 font-medium">错误</th>
                        <th class="w-36 px-2 py-1.5 font-medium">操作</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr v-for="item in filteredDetailItems" :key="item.itemId" class="border-b border-cv-border/60 last:border-0">
                        <td class="max-w-[220px] truncate px-2 py-1.5 text-cv-text" :title="item.name">{{ item.name }}</td>
                        <td class="px-2 py-1.5">
                          <UiBadge :tone="itemStatusTone(item.status)">{{ itemStatusLabel(item.status) }}</UiBadge>
                        </td>
                        <td class="max-w-[240px] truncate px-2 py-1.5 text-cv-text-3" :title="item.errorMessage || item.errorCode || ''">
                          {{ item.errorMessage || item.errorCode || "—" }}
                        </td>
                        <td class="px-2 py-1.5">
                          <div class="flex gap-1">
                            <UiButton
                              v-if="item.taskId"
                              size="sm"
                              variant="secondary"
                              @click="requeue(item.taskId!)"
                            >
                              重新入队
                            </UiButton>
                            <UiButton
                              size="sm"
                              variant="ghost"
                              @click="goToLibraryWithQuery(item.name)"
                            >
                              文件库
                            </UiButton>
                          </div>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
                <p v-else class="text-cv-caption text-cv-text-3">无关键明细</p>
              </template>
              <p v-else-if="detailError" class="text-cv-caption text-cv-danger">{{ detailError }}</p>
            </div>
          </div>
        </div>
      </UiCard>

      <UiCard
        title="归档队列"
        info="重新入队只改状态，上传由下次运行执行。列表与进行中运行每 2 秒自动刷新。"
        class="xl:col-span-2"
      >
        <template #headerExtra>
          <div class="flex items-center gap-2">
            <div class="w-36">
              <UiSelect v-model="statusFilter" @change="() => refreshTasks()">
                <option value="">全部状态</option>
                <option value="queued">待归档</option>
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
                    <UiButton v-if="canRequeue(t.status)" size="sm" variant="secondary" @click="requeue(t.taskId)">重新入队</UiButton>
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
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from "vue";
import UiButton from "../components/ui/UiButton.vue";
import UiCard from "../components/ui/UiCard.vue";
import UiInput from "../components/ui/UiInput.vue";
import UiSelect from "../components/ui/UiSelect.vue";
import UiBadge from "../components/ui/UiBadge.vue";
import UiSwitch from "../components/ui/UiSwitch.vue";
import {
  getAppSettings,
  getCollectSelectedAccounts,
  getCollectSourceCache,
  getScheduleStatus,
  listUploadTasks,
  listTaskRuns,
  getTaskRunDetail,
  requeueUploadTask,
  pauseUploadTask,
  runPipeline,
  setScheduleConfig,
  syncRestore,
  getActiveRun,
  cancelTaskRun,
} from "../api/tauri";
import { useCollectSources } from "../composables/useCollectSources";
import { pushToast } from "../composables/useToast";
import { confirmAction } from "../composables/useConfirm";
import { navigateTo, goToLibraryWithQuery } from "../composables/useNav";
import { usePipelineProgress } from "../composables/usePipelineProgress";
import { formatDurationMs, formatDateTime } from "../utils/format";
import type {
  PipelineResultDto,
  UploadTaskDto,
  TaskRunDto,
  TaskRunDetailDto,
  WebdavConfigDto,
  ActiveRunDto,
} from "../types";

type StageId = "idle" | "scan" | "done" | "failed";

const {
  allWechatAccounts,
  accountKey,
  canRun,
  collectSources,
  detecting,
  discoverAndAddWechat,
  isAccountSelected,
  loadSources,
  pickAndAddAttachment,
  pickAndAddWechat,
  pickAndReplaceSource,
  refreshSource,
  removeSource,
  selectedAccounts,
  sourceKey,
  sourceStatusDetail,
  sourceStatusLabel,
  sourceStatusTone,
  sourceTypeLabel,
  toggleAccount,
  toggleSourceImages,
} = useCollectSources();
const scheduleEnabled = ref(false);
const scanIntervalMinutes = ref(30);
const maxScanIntervalHours = 168;
const scheduleRegistered = ref(false);
const fullScan = ref(false);
const running = ref(false);
const cancelling = ref(false);
const activeRun = ref<ActiveRunDto | null>(null);
const pipelineError = ref("");
const pipelineResult = ref<PipelineResultDto | null>(null);
const stage = ref<StageId>("idle");
const webdavReady = ref(false);
const webdavConfig = ref<WebdavConfigDto>({ url: "", username: "", password: "", vaultId: "chatvault-default" });

const tasks = ref<UploadTaskDto[]>([]);
const statusFilter = ref("");
const tasksLoading = ref(false);
let pollTimer: number | undefined;
let scheduleSaveTimer: number | undefined;

const runs = ref<TaskRunDto[]>([]);
const historyLoading = ref(false);
const expandedRunId = ref<string | null>(null);
const detail = ref<TaskRunDetailDto | null>(null);
const detailLoading = ref(false);
const detailError = ref("");
const itemStatusFilter = ref("");

const { live: liveProgress, reset: resetLive } = usePipelineProgress(() => {
  void refreshHistory();
  void refreshTasks(true);
});

const progressPercent = computed(() => {
  if (!liveProgress.value.total) return 0;
  return Math.min(100, Math.round((liveProgress.value.done / liveProgress.value.total) * 100));
});

const activeRunProgressPercent = computed(() => {
  const a = activeRun.value;
  if (!a?.total) return 0;
  return Math.min(100, Math.round(((a.done ?? 0) / a.total) * 100));
});

const scanIntervalHours = computed(() => {
  const hours = scanIntervalMinutes.value / 60;
  return Number.isFinite(hours) ? String(Number(hours.toFixed(2))) : "0.5";
});

const statusHeadline = computed(() => {
  if (running.value) {
    if (liveProgress.value.stage) return `${stageLabel(liveProgress.value.stage)}中`;
    return "运行中";
  }
  if (activeRun.value) {
    const src = activeRun.value.runnerKind === "cli" ? "定时" : "手动";
    const st = activeRun.value.currentStage ? stageLabel(activeRun.value.currentStage) : "运行";
    return `${src}${st}中`;
  }
  if (stage.value === "failed") return "运行失败";
  if (stage.value === "done" && pipelineResult.value) {
    return pipelineResult.value.webdavConfigured ? "运行完成" : "已本地扫描";
  }
  return "等待运行";
});

const statusDetail = computed(() => {
  if (running.value) {
    const stageName = liveProgress.value.stageMessage
      || (liveProgress.value.stage ? stageLabel(liveProgress.value.stage) : "扫描 → 归档 → 同步");
    return stageName;
  }
  if (activeRun.value) {
    const parts = [
      activeRun.value.runnerKind === "cli" ? "系统计划任务" : "桌面立即运行",
    ];
    if (activeRun.value.currentStage) parts.push(stageLabel(activeRun.value.currentStage));
    if (activeRun.value.total) {
      parts.push(`${activeRun.value.done ?? 0}/${activeRun.value.total}`);
    }
    if (activeRun.value.currentName) parts.push(activeRun.value.currentName);
    if (activeRun.value.cancelRequested) parts.push("正在取消…");
    return parts.join(" · ");
  }
  if (stage.value === "failed") return pipelineError.value || "请检查网络与 WebDAV 配置后重试";
  if (stage.value === "done" && pipelineResult.value) {
    return pipelineResult.value.message + " · 耗时 " + formatDurationMs(pipelineResult.value.durationMs);
  }
  if (!webdavReady.value) return "未连接 WebDAV，本次只会进行本地扫描";
  return "";
});

const statusHeadlineClass = computed(() => {
  if (running.value || activeRun.value) return "font-medium text-cv-accent";
  if (stage.value === "failed") return "font-medium text-cv-danger";
  return "font-medium";
});

const statusBadge = computed(() => {
  if (running.value || activeRun.value) {
    if (activeRun.value?.cancelRequested || cancelling.value) return "取消中";
    return "进行中";
  }
  if (stage.value === "failed") return "失败";
  if (stage.value === "done") return "已完成";
  if (!webdavReady.value) return "仅本地";
  return "就绪";
});

const statusBadgeTone = computed<"neutral" | "accent" | "success" | "warning" | "danger">(() => {
  if (running.value || activeRun.value) return "accent";
  if (stage.value === "failed") return "danger";
  if (stage.value === "done") return "success";
  if (!webdavReady.value) return "warning";
  return "neutral";
});

const statusIndicatorClass = computed(() => {
  if (running.value || activeRun.value) return "bg-cv-accent-soft text-cv-accent";
  if (stage.value === "failed") return "bg-cv-surface-2 text-cv-danger";
  if (stage.value === "done") return "bg-cv-accent-soft text-cv-success";
  if (!webdavReady.value) return "bg-cv-surface-2 text-cv-warning";
  return "bg-cv-surface-2 text-cv-text-3";
});

/** 开关即时写入，无保存步骤。 */
async function onScheduleToggle(next: boolean) {
  scheduleEnabled.value = next;
  await saveSchedule();
}

const statusMap: Record<string, string> = {
  queued: "待归档",
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

async function loadSettings() {
  try {
    const [s, cache, persistedSelections] = await Promise.all([
      getAppSettings(),
      getCollectSourceCache().catch(() => []),
      getCollectSelectedAccounts().catch(() => null),
    ]);
    await loadSources(s.collectSources || [], cache || [], persistedSelections);
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

function onIntervalInput(raw: string | number) {
  const hours = Number(raw);
  scanIntervalMinutes.value = Number.isFinite(hours) ? Math.round(hours * 60) : 0;
  if (scheduleSaveTimer) window.clearTimeout(scheduleSaveTimer);
  scheduleSaveTimer = window.setTimeout(() => void saveSchedule(), 600);
}

async function saveSchedule() {
  if (scheduleSaveTimer) {
    window.clearTimeout(scheduleSaveTimer);
    scheduleSaveTimer = undefined;
  }
  const interval = Math.min(maxScanIntervalHours * 60, Math.max(5, Number(scanIntervalMinutes.value) || 30));
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
  resetLive();
  try {
    // 已配置微信源且存在账号时传本次勾选项；空数组表示本次跳过微信。
    const targetAccounts = allWechatAccounts.value.length
      ? selectedAccounts.value.slice()
      : null;
    const result = await runPipeline({
      targetAccounts,
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
    await refreshHistory();
  } catch (err) {
    stage.value = "failed";
    pipelineError.value = String(err);
    pushToast({ tone: "danger", title: "流水线失败", description: String(err) });
    await refreshHistory();
  } finally {
    running.value = false;
    resetLive();
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
    pushToast({ tone: "success", title: "已重新入队，待下次运行上传" });
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

function stageLabel(s: string) {
  const map: Record<string, string> = {
    scan: "扫描",
    archive: "归档",
    publish: "发布",
    pull: "拉取",
  };
  return map[s] || s;
}

function stageStatusLabel(s: string) {
  const map: Record<string, string> = {
    pending: "等待",
    running: "进行中",
    success: "成功",
    failed: "失败",
    skipped: "跳过",
  };
  return map[s] || s;
}

function stageStatusClass(s: string) {
  if (s === "success") return "text-cv-success";
  if (s === "failed") return "text-cv-danger";
  if (s === "running") return "text-cv-accent";
  if (s === "skipped") return "text-cv-text-3";
  return "text-cv-text-2";
}

function runStatusLabel(s: string) {
  const map: Record<string, string> = {
    running: "进行中",
    success: "成功",
    partial: "部分成功",
    failed: "失败",
    cancelled: "已取消",
  };
  return map[s] || s;
}

function runStatusTone(s: string): "neutral" | "accent" | "success" | "warning" | "danger" {
  if (s === "success") return "success";
  if (s === "partial") return "warning";
  if (s === "failed") return "danger";
  if (s === "running") return "accent";
  if (s === "cancelled") return "neutral";
  return "neutral";
}

function itemStatusLabel(s: string) {
  const map: Record<string, string> = {
    failed: "上传失败",
    missing: "本地缺失",
    decrypt_failed: "解密失败",
    skipped: "已跳过",
  };
  return map[s] || s;
}

function itemStatusTone(s: string): "neutral" | "accent" | "success" | "warning" | "danger" {
  if (s === "decrypt_failed") return "warning";
  if (s === "missing") return "warning";
  if (s === "skipped") return "neutral";
  return "danger";
}

function runSummaryText(run: TaskRunDto) {
  if (run.errorMessage) return run.errorMessage;
  if (!run.summaryJson) return "—";
  try {
    const s = JSON.parse(run.summaryJson) as Record<string, unknown>;
    const scan = s.scan as { newObjects?: number } | null;
    const archive = s.archive as { uploaded?: number; failed?: number } | null;
    const parts: string[] = [];
    if (scan && typeof scan.newObjects === "number") parts.push(`新增 ${scan.newObjects}`);
    if (archive) parts.push(`归档 ${archive.uploaded ?? 0}/失败 ${archive.failed ?? 0}`);
    if (typeof s.publishedSeq === "number") parts.push(`发布 ${s.publishedSeq}`);
    if (typeof s.applied === "number") parts.push(`应用 ${s.applied}`);
    if (typeof s.failedItems === "number" && s.failedItems > 0) parts.push(`明细失败 ${s.failedItems}`);
    return parts.join(" · ") || "—";
  } catch {
    return run.summaryJson;
  }
}

const filteredDetailItems = computed(() => {
  if (!detail.value) return [];
  if (!itemStatusFilter.value) return detail.value.items;
  return detail.value.items.filter((i) => i.status === itemStatusFilter.value);
});

async function refreshHistory(silent = false) {
  if (!silent) historyLoading.value = true;
  try {
    runs.value = await listTaskRuns(50);
  } catch (err) {
    if (!silent) {
      pushToast({ tone: "danger", title: "加载运行历史失败", description: String(err) });
    }
  } finally {
    if (!silent) historyLoading.value = false;
  }
}

async function toggleRunDetail(runId: string) {
  if (expandedRunId.value === runId) {
    expandedRunId.value = null;
    detail.value = null;
    return;
  }
  expandedRunId.value = runId;
  detail.value = null;
  detailError.value = "";
  detailLoading.value = true;
  itemStatusFilter.value = "";
  try {
    detail.value = await getTaskRunDetail(runId);
  } catch (err) {
    detailError.value = String(err);
  } finally {
    detailLoading.value = false;
  }
}

onMounted(async () => {
  await Promise.all([loadSettings(), refreshTasks(), refreshHistory(), refreshActiveRun()]);
});

async function refreshActiveRun() {
  try {
    const next = await getActiveRun();
    const prevId = activeRun.value?.runId;
    activeRun.value = next;
    if (prevId && (!next || next.runId !== prevId)) {
      void refreshHistory(true);
      void refreshTasks(true);
    }
  } catch {
    /* 静默：轮询失败不打断页面 */
  }
}

async function requestCancelRun() {
  const runId = running.value ? liveProgress.value.runId : activeRun.value?.runId;
  if (!runId) return;
  const ok = await confirmAction({
    title: "结束运行？",
    description: "当前阶段将停止，已完成的归档保留，队列中的文件待下次运行继续上传。",
    confirmLabel: "结束运行",
    danger: true,
  });
  if (!ok) return;
  cancelling.value = true;
  try {
    await cancelTaskRun(runId);
    pushToast({ tone: "success", title: "已请求取消，等待运行停止" });
    await refreshActiveRun();
  } catch (err) {
    pushToast({ tone: "danger", title: "取消失败", description: String(err) });
  } finally {
    cancelling.value = false;
  }
}

function startPolling() {
  if (pollTimer) return;
  pollTimer = window.setInterval(() => {
    void refreshTasks(true);
    void refreshActiveRun();
  }, 2000);
}

function stopPolling() {
  if (pollTimer) {
    window.clearInterval(pollTimer);
    pollTimer = undefined;
  }
}

// keep-alive：切回任务页时恢复轮询并静默刷新
onActivated(() => {
  startPolling();
  void refreshTasks(true);
  void refreshHistory(true);
  void refreshActiveRun();
});

onDeactivated(() => {
  stopPolling();
});

onBeforeUnmount(() => {
  stopPolling();
  if (scheduleSaveTimer) window.clearTimeout(scheduleSaveTimer);
});
</script>
