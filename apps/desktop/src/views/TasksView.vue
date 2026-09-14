<!--
  ChatVault 任务中枢
  职责：采集范围、调度、立即运行流水线、上传队列与恢复。
  术语：运行=一次流水线；上传队列=跨运行单文件项；归档仅指流水线上传阶段。
-->
<template>
  <div class="flex h-full flex-col gap-3 p-6">
    <header class="flex shrink-0 flex-wrap items-center justify-between gap-3">
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
        <UiButton size="sm" variant="ghost" :disabled="running" @click="doRestore">
          恢复索引
        </UiButton>
        <UiButton
          v-if="running || activeRun"
          variant="danger"
          :loading="cancelling"
          @click="requestCancelRun"
        >
          结束运行
        </UiButton>
        <UiButton v-else variant="primary" :disabled="!canRun" @click="startPipeline">
          立即运行
        </UiButton>
      </div>
    </header>

    <!-- 状态条：调度 + 当前/上次运行，主路径一眼可见 -->
    <div
      class="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-2 rounded-cv-lg border border-cv-border bg-cv-surface px-3.5 py-2.5"
    >
      <div class="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1.5">
        <div class="flex items-center gap-2">
          <span
            class="h-1.5 w-1.5 shrink-0 rounded-full"
            :class="running || activeRun ? 'animate-pulse bg-cv-accent' : statusDotClass"
            aria-hidden="true"
          />
          <span class="text-cv-body font-medium" :class="statusHeadlineClass">{{ statusHeadline }}</span>
          <UiBadge :tone="statusBadgeTone">{{ statusBadge }}</UiBadge>
        </div>
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
        <span class="text-cv-caption text-cv-text-3">|</span>
        <span class="text-cv-caption text-cv-text-2">
          定时 {{ scheduleEnabled ? "开" : "关" }}
          <template v-if="scheduleEnabled"> · 每 {{ scanIntervalHours }} 小时</template>
        </span>
        <UiBadge v-if="scheduleEnabled" :tone="scheduleRegistered ? 'success' : 'danger'">
          {{ scheduleRegistered ? "计划已注册" : "注册失败" }}
        </UiBadge>
      </div>
      <div class="min-w-0 max-w-full flex-1 basis-full sm:basis-auto sm:max-w-md">
        <p v-if="statusDetail" class="truncate text-cv-caption text-cv-text-3" :title="statusDetail">
          {{ statusDetail }}
        </p>
        <p
          v-if="running && liveProgress.currentName"
          class="truncate font-mono text-cv-caption text-cv-text-2"
          :title="liveProgress.currentName"
        >
          当前：{{ liveProgress.currentName }}
        </p>
        <p
          v-else-if="!running && activeRun?.currentName"
          class="truncate font-mono text-cv-caption text-cv-text-2"
          :title="activeRun.currentName"
        >
          当前：{{ activeRun.currentName }}
        </p>
        <div
          v-if="running && liveProgress.total > 0"
          class="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-cv-surface-2"
        >
          <div
            class="h-full rounded-full bg-cv-accent transition-all"
            :style="{ width: progressPercent + '%' }"
          />
        </div>
        <div
          v-else-if="!running && activeRun?.total"
          class="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-cv-surface-2"
        >
          <div
            class="h-full rounded-full bg-cv-accent transition-all"
            :style="{ width: activeRunProgressPercent + '%' }"
          />
        </div>
      </div>
    </div>

    <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pb-2">
      <div class="grid shrink-0 grid-cols-1 gap-3 lg:grid-cols-2">
        <!-- 采集范围 -->
        <UiCard title="采集范围" info="扫描会检查以下目录；立即运行时可只勾选本次微信账号。">
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
              class="rounded-cv border border-cv-border"
            >
              <div class="flex items-start justify-between gap-3 px-3 py-2.5">
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-1.5">
                    <UiBadge :tone="source.sourceType === 'wechat-windows-4' ? 'accent' : 'neutral'">
                      {{ sourceTypeLabel(source.sourceType) }}
                    </UiBadge>
                    <UiBadge :tone="sourceStatusTone(source.status)">
                      {{ sourceStatusLabel(source.status) }}
                    </UiBadge>
                    <span
                      v-if="source.sourceType === 'wechat-windows-4' && source.accounts.length"
                      class="text-cv-caption text-cv-text-2"
                    >
                      账号 {{ selectedCountInSource(source) }}/{{ source.accounts.length }}
                    </span>
                  </div>
                  <p class="mt-1 truncate font-mono text-cv-caption text-cv-text" :title="displayPath(source.path)">
                    {{ displayPath(source.path) }}
                  </p>
                  <p class="mt-0.5 text-cv-caption text-cv-text-3">{{ sourceStatusDetail(source) }}</p>
                </div>
                <div class="flex shrink-0 items-center gap-1">
                  <UiButton
                    v-if="source.sourceType === 'wechat-windows-4' && source.accounts.length"
                    size="sm"
                    variant="ghost"
                    @click="toggleAccountsExpanded(sourceKey(source))"
                  >
                    {{ isAccountsExpanded(sourceKey(source)) ? "收起账号" : "勾选账号" }}
                  </UiButton>
                  <UiButton
                    size="sm"
                    variant="ghost"
                    :loading="source.inspecting"
                    @click="source.status === 'missing' ? pickAndReplaceSource(index) : refreshSource(index)"
                  >
                    {{ source.status === "missing" ? "重新选择" : source.sourceType === "wechat-windows-4" ? "重新识别" : "重新检查" }}
                  </UiButton>
                  <UiButton size="sm" variant="ghost" @click="removeSource(index)">移除</UiButton>
                </div>
              </div>

              <div
                v-if="source.sourceType === 'wechat-windows-4'"
                class="flex items-center gap-2 border-t border-cv-border px-3 py-2"
              >
                <label class="inline-flex cursor-pointer items-center gap-2 text-cv-caption text-cv-text-2">
                  <input
                    type="checkbox"
                    class="h-4 w-4 accent-[var(--cv-accent)]"
                    :checked="source.enableVideos !== false"
                    @change="toggleSourceVideos(source)"
                  />
                  <span>识别视频</span>
                </label>
                <span class="text-cv-caption text-cv-text-3">扫描 msg/video 下的 .mp4</span>
              </div>

              <div
                v-if="source.sourceType === 'wechat-windows-4' && source.accounts.length && isAccountsExpanded(sourceKey(source))"
                class="border-t border-cv-border px-3 py-2"
              >
                <div class="grid gap-1 sm:grid-cols-2">
                  <label
                    v-for="account in source.accounts"
                    :key="accountKey(account)"
                    class="flex cursor-pointer items-start gap-2 rounded-cv px-1.5 py-1 hover:bg-cv-surface-2"
                  >
                    <input
                      type="checkbox"
                      class="mt-0.5 accent-[var(--cv-accent)]"
                      :checked="isAccountSelected(account)"
                      @change="toggleAccount(account)"
                    />
                    <span class="min-w-0">
                      <span class="block truncate font-mono text-cv-caption text-cv-text">
                        {{ account.sourceAccountId }}
                      </span>
                      <span class="block text-cv-caption text-cv-text-3">
                        附件 {{ account.filesCountEstimated }} · 视频 {{ account.videosCountEstimated ?? 0 }}
                      </span>
                    </span>
                  </label>
                </div>
              </div>
            </div>
          </div>
          <div v-else class="py-3">
            <p class="text-cv-caption text-cv-text-2">尚未添加采集源</p>
            <p class="mt-1 text-cv-caption text-cv-text-3">从上方添加微信目录或附件目录后即可扫描。</p>
          </div>
        </UiCard>

        <!-- 调度与运行 -->
        <UiCard
          title="调度与运行"
          info="定时与「立即运行」共用同一条流水线：扫描 → 上传 → 同步元数据。未配置 WebDAV 时只做本地扫描。"
        >
          <div class="space-y-0">
            <div class="flex items-center justify-between gap-3 py-2">
              <div class="min-w-0">
                <p class="text-cv-body font-medium text-cv-text-2">执行间隔</p>
                <p class="mt-0.5 text-cv-caption text-cv-text-3">定时自动运行频率</p>
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

            <div class="flex items-center justify-between gap-3 border-t border-cv-border py-2">
              <div class="min-w-0">
                <p class="text-cv-body font-medium text-cv-text-2">自动运行</p>
                <p class="mt-0.5 text-cv-caption text-cv-text-3">按固定周期自动扫描并上传</p>
              </div>
              <UiSwitch
                :model-value="scheduleEnabled"
                label="自动运行"
                @update:model-value="onScheduleToggle"
              />
            </div>

            <label
              class="flex cursor-pointer items-start gap-3 border-t border-cv-border py-2"
              :class="running ? 'opacity-60' : ''"
            >
              <input
                v-model="fullScan"
                type="checkbox"
                class="mt-0.5 h-4 w-4 shrink-0 accent-[var(--cv-accent)]"
                :disabled="running"
              />
              <span class="min-w-0">
                <span class="block text-cv-body font-medium text-cv-text-2">本次强制全量扫描</span>
                <span class="mt-0.5 block text-cv-caption text-cv-text-3">忽略增量记录，重新检查全部附件</span>
              </span>
            </label>

            <div class="border-t border-cv-border pt-3">
              <p class="text-cv-caption text-cv-text-3">
                主操作在页头「立即运行」；运行中可在此页随时结束。
              </p>
            </div>
          </div>
        </UiCard>
      </div>

      <!-- 运行历史 / 上传队列 -->
      <UiCard body-class="!p-0" class="min-h-0">
        <template #header>
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div class="flex items-center gap-1">
              <button
                type="button"
                class="rounded-cv px-2.5 py-1 text-cv-caption transition-colors"
                :class="
                  activeTab === 'history'
                    ? 'bg-cv-accent-soft font-medium text-cv-accent'
                    : 'text-cv-text-2 hover:bg-cv-surface-2 hover:text-cv-text'
                "
                @click="activeTab = 'history'"
              >
                运行历史
              </button>
              <button
                type="button"
                class="rounded-cv px-2.5 py-1 text-cv-caption transition-colors"
                :class="
                  activeTab === 'queue'
                    ? 'bg-cv-accent-soft font-medium text-cv-accent'
                    : 'text-cv-text-2 hover:bg-cv-surface-2 hover:text-cv-text'
                "
                @click="activeTab = 'queue'"
              >
                上传队列
                <span v-if="pendingQueueCount > 0" class="ml-1 text-cv-accent">{{ pendingQueueCount }}</span>
              </button>
            </div>
            <div class="flex items-center gap-2">
              <template v-if="activeTab === 'history'">
                <UiButton size="sm" variant="ghost" :loading="historyLoading" @click="() => refreshHistory()">
                  刷新
                </UiButton>
              </template>
              <template v-else>
                <div class="w-36">
                  <UiSelect v-model="statusFilter" @change="() => refreshTasks()">
                    <option value="pending">待处理</option>
                    <option value="">全部状态</option>
                    <option value="queued">待上传</option>
                    <option value="retryable_failed">可重试失败</option>
                    <option value="missing">本地缺失</option>
                    <option value="paused">已暂停</option>
                    <option value="backed_up">已校验</option>
                  </UiSelect>
                </div>
                <UiButton size="sm" variant="ghost" :loading="tasksLoading" @click="() => refreshTasks()">
                  刷新
                </UiButton>
              </template>
            </div>
          </div>
        </template>

        <!-- 运行历史 -->
        <div v-if="activeTab === 'history'" class="p-4 pt-2">
          <p
            v-if="!historyLoading && !runs.length"
            class="py-6 text-center text-cv-caption text-cv-text-3"
          >
            暂无运行记录，执行一次「立即运行」后可在此回看。
          </p>
          <div v-else class="space-y-2">
            <div
              v-for="run in runs"
              :key="run.runId"
              class="rounded-cv border border-cv-border"
            >
              <button
                type="button"
                class="grid w-full grid-cols-[minmax(0,1fr)] items-center gap-2 px-3 py-2 text-left hover:bg-cv-surface-2 md:grid-cols-[7.5rem_5rem_3.5rem_minmax(0,1fr)_4rem_4rem_2.5rem]"
                @click="toggleRunDetail(run.runId)"
              >
                <span class="text-cv-caption text-cv-text-2">{{ formatDateTime(run.startedAt, { compact: true }) }}</span>
                <span>
                  <UiBadge :tone="runStatusTone(run.status)">{{ runStatusLabel(run.status) }}</UiBadge>
                </span>
                <span class="text-cv-caption text-cv-text-3">
                  {{ run.triggerSource === "schedule" ? "定时" : "手动" }}
                </span>
                <span
                  class="min-w-0 truncate text-cv-caption text-cv-text"
                  :title="run.errorMessage || runSummaryText(run)"
                >
                  {{ runSummaryText(run) }}
                </span>
                <span v-if="run.failedItems > 0" class="text-cv-caption text-cv-danger">
                  失败 {{ run.failedItems }}
                </span>
                <span v-else class="hidden md:block" />
                <span class="text-cv-caption text-cv-text-3">
                  {{ run.durationMs != null ? formatDurationMs(run.durationMs) : "—" }}
                </span>
                <span class="text-right text-cv-caption text-cv-text-3">
                  {{ expandedRunId === run.runId ? "收起" : "详情" }}
                </span>
              </button>

              <div v-if="expandedRunId === run.runId" class="border-t border-cv-border px-3 py-2.5">
                <p v-if="detailLoading" class="text-cv-caption text-cv-text-3">加载中…</p>
                <template v-else-if="detail">
                  <div class="mb-3 flex flex-wrap items-center gap-1.5">
                    <template v-for="st in visibleStages(detail.stages)" :key="st.stage">
                      <span
                        class="inline-flex items-center gap-1 rounded-cv bg-cv-surface-2 px-2 py-1 text-cv-caption"
                        :title="st.message || ''"
                      >
                        <span class="text-cv-text-2">{{ stageLabel(st.stage) }}</span>
                        <span :class="stageStatusClass(st.status)">{{ stageStatusLabel(st.status) }}</span>
                        <span v-if="st.durationMs != null" class="text-cv-text-3">
                          · {{ formatDurationMs(st.durationMs) }}
                        </span>
                      </span>
                    </template>
                  </div>

                  <div class="mb-2 flex flex-wrap items-center gap-2">
                    <span class="text-cv-caption text-cv-text-2">异常明细</span>
                    <div class="w-32">
                      <UiSelect v-model="itemStatusFilter">
                        <option value="">全部</option>
                        <option value="failed">上传失败</option>
                        <option value="missing">本地缺失</option>
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
                          <th class="px-2 py-1.5 font-medium">原因</th>
                          <th class="w-36 px-2 py-1.5 font-medium">操作</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr
                          v-for="item in filteredDetailItems"
                          :key="item.itemId"
                          class="border-b border-cv-border/60 last:border-0"
                        >
                          <td class="max-w-[220px] truncate px-2 py-1.5 text-cv-text" :title="item.name">
                            {{ item.name }}
                          </td>
                          <td class="px-2 py-1.5">
                            <UiBadge :tone="itemStatusTone(item.status)">
                              {{ itemStatusLabel(item.status) }}
                            </UiBadge>
                          </td>
                          <td
                            class="max-w-[240px] truncate px-2 py-1.5 text-cv-text-3"
                            :title="item.errorMessage || item.errorCode || ''"
                          >
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
                              <UiButton size="sm" variant="ghost" @click="goToLibraryWithQuery(item.name)">
                                文件库
                              </UiButton>
                            </div>
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                  <p v-else class="text-cv-caption text-cv-text-3">无异常明细</p>
                </template>
                <p v-else-if="detailError" class="text-cv-caption text-cv-danger">{{ detailError }}</p>
              </div>
            </div>
          </div>
        </div>

        <!-- 上传队列 -->
        <div v-else class="p-4 pt-2">
          <p class="mb-2 text-cv-caption text-cv-text-3">
            共 {{ tasks.length }} 条
            <template v-if="statusFilter === 'pending'"> · 仅显示待处理（待上传 / 失败 / 缺失 / 暂停）</template>
          </p>
          <div class="max-h-80 overflow-auto rounded-cv border border-cv-border">
            <table class="w-full text-left text-cv-caption">
              <thead class="sticky top-0 border-b border-cv-border bg-cv-surface-2 text-cv-text-2">
                <tr>
                  <th class="px-2.5 py-2 font-medium">文件名</th>
                  <th class="w-28 px-2.5 py-2 font-medium">状态</th>
                  <th class="w-20 px-2.5 py-2 font-medium">大小</th>
                  <th class="w-32 px-2.5 py-2 font-medium">更新时间</th>
                  <th class="w-36 px-2.5 py-2 font-medium">说明</th>
                  <th class="w-28 px-2.5 py-2 font-medium">操作</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="t in tasks"
                  :key="t.taskId"
                  class="border-b border-cv-border/60 last:border-0"
                >
                  <td class="max-w-[200px] truncate px-2.5 py-1.5 text-cv-text" :title="t.originalName">
                    {{ t.originalName }}
                  </td>
                  <td class="px-2.5 py-1.5">
                    <UiBadge :tone="statusTone(t.status)">{{ statusLabel(t.status) }}</UiBadge>
                  </td>
                  <td class="px-2.5 py-1.5 text-cv-text-2">{{ t.formattedSize }}</td>
                  <td class="px-2.5 py-1.5 text-cv-text-3">
                    {{ formatDateTime(t.updatedAt, { compact: true }) }}
                  </td>
                  <td
                    class="max-w-[160px] truncate px-2.5 py-1.5 text-cv-text-3"
                    :title="queueNoteTitle(t)"
                  >
                    {{ queueNote(t) }}
                  </td>
                  <td class="px-2.5 py-1.5">
                    <div class="flex gap-1">
                      <UiButton v-if="canRequeue(t.status)" size="sm" variant="secondary" @click="requeue(t.taskId)">
                        重新入队
                      </UiButton>
                      <UiButton v-if="canPause(t.status)" size="sm" variant="ghost" @click="pause(t.taskId)">
                        暂停
                      </UiButton>
                    </div>
                  </td>
                </tr>
                <tr v-if="!tasksLoading && tasks.length === 0">
                  <td colspan="6" class="py-8 text-center text-cv-text-3">
                    {{ statusFilter === "pending" ? "没有待处理的上传项" : "暂无上传项" }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
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
import { formatDurationMs, formatDateTime, displayPath } from "../utils/format";
import type {
  PipelineResultDto,
  UploadTaskDto,
  TaskRunDto,
  TaskRunDetailDto,
  TaskRunStageDto,
  WebdavConfigDto,
  ActiveRunDto,
  WechatAccountDto,
} from "../types";

type StageId = "idle" | "scan" | "done" | "failed";
type CollectSourceLike = ReturnType<typeof useCollectSources>["collectSources"]["value"][number];

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
  toggleSourceVideos,
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

const activeTab = ref<"history" | "queue">("history");
const expandedSourceKeys = ref<Set<string>>(new Set());

const tasks = ref<UploadTaskDto[]>([]);
const statusFilter = ref("pending");
const pendingCount = ref(0);
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

const pendingQueueCount = computed(() => pendingCount.value);

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
    const stageName =
      liveProgress.value.stageMessage ||
      (liveProgress.value.stage ? stageLabel(liveProgress.value.stage) : "扫描 → 上传 → 同步");
    return stageName;
  }
  if (activeRun.value) {
    const parts = [activeRun.value.runnerKind === "cli" ? "系统计划任务" : "桌面立即运行"];
    if (activeRun.value.currentStage) parts.push(stageLabel(activeRun.value.currentStage));
    if (activeRun.value.cancelRequested) parts.push("正在结束…");
    return parts.join(" · ");
  }
  if (stage.value === "failed") return pipelineError.value || "请检查网络与 WebDAV 配置后重试";
  if (stage.value === "done" && pipelineResult.value) {
    return pipelineResult.value.message + " · 耗时 " + formatDurationMs(pipelineResult.value.durationMs);
  }
  if (!webdavReady.value) return "未连接 WebDAV，本次只会本地扫描入库";
  return "";
});

const statusHeadlineClass = computed(() => {
  if (running.value || activeRun.value) return "text-cv-accent";
  if (stage.value === "failed") return "text-cv-danger";
  return "text-cv-text";
});

const statusBadge = computed(() => {
  if (running.value || activeRun.value) {
    if (activeRun.value?.cancelRequested || cancelling.value) return "结束中";
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

const statusDotClass = computed(() => {
  if (stage.value === "failed") return "bg-cv-danger";
  if (stage.value === "done") return "bg-cv-success";
  if (!webdavReady.value) return "bg-cv-warning";
  return "bg-cv-text-3";
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

/** 队列行说明：失败原因、重试次数或占位。 */
function queueNote(t: UploadTaskDto) {
  if (t.status === "retryable_failed" || t.status === "missing") {
    if (t.retryCount > 0) return `已试 ${t.retryCount} 次`;
    return t.errorMessage ? "见详情" : "—";
  }
  if (t.status === "paused" && t.retryCount > 0) return `已试 ${t.retryCount} 次`;
  return "—";
}

function queueNoteTitle(t: UploadTaskDto) {
  const parts: string[] = [];
  if (t.errorMessage) parts.push(t.errorMessage);
  if (t.retryCount > 0) parts.push(`重试 ${t.retryCount} 次`);
  return parts.join(" · ") || t.originalPath || "";
}

function isAccountsExpanded(key: string) {
  return expandedSourceKeys.value.has(key);
}

function toggleAccountsExpanded(key: string) {
  const next = new Set(expandedSourceKeys.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  expandedSourceKeys.value = next;
}

function selectedCountInSource(source: CollectSourceLike) {
  if (!source.accounts.length) return 0;
  return source.accounts.filter((account: WechatAccountDto) => isAccountSelected(account)).length;
}

/** 隐藏 skipped 阶段，减少未配置 WebDAV / 取消时的噪音。 */
function visibleStages(stages: TaskRunStageDto[]) {
  return stages.filter((st) => st.status !== "skipped");
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
  const interval = Math.min(
    maxScanIntervalHours * 60,
    Math.max(5, Number(scanIntervalMinutes.value) || 30),
  );
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

/** 立即运行：与定时同构的扫描→上传→同步。 */
async function startPipeline() {
  running.value = true;
  pipelineError.value = "";
  pipelineResult.value = null;
  stage.value = "scan";
  resetLive();
  try {
    // 已配置微信源且存在账号时传本次勾选项；空数组表示本次跳过微信。
    const targetAccounts = allWechatAccounts.value.length ? selectedAccounts.value.slice() : null;
    const result = await runPipeline({
      targetAccounts,
      fullScan: fullScan.value,
    });
    pipelineResult.value = result;
    stage.value = "done";
    pushToast({
      tone: result.archive && result.archive.failedCount > 0 ? "warning" : "success",
      title: "运行完成",
      description: result.message,
      action: { label: "去检索", onClick: () => navigateTo("library") },
    });
    await refreshTasks();
    await refreshHistory();
  } catch (err) {
    stage.value = "failed";
    pipelineError.value = String(err);
    pushToast({ tone: "danger", title: "运行失败", description: String(err) });
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
    const list = await listUploadTasks(statusFilter.value || undefined, 300);
    tasks.value = list;
    // 待处理/全部筛选时刷新角标；其他筛选保留上次待处理数
    if (statusFilter.value === "pending" || statusFilter.value === "") {
      pendingCount.value = list.filter((t) => t.status !== "backed_up").length;
    }
  } catch (err) {
    if (!silent) {
      pushToast({ tone: "danger", title: "加载上传队列失败", description: String(err) });
    } else {
      const now = Date.now();
      if (now - lastErrorToastAt > 60000) {
        lastErrorToastAt = now;
        pushToast({ tone: "warning", title: "上传队列刷新失败", description: String(err) });
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
    pushToast({ tone: "success", title: "已重新入队，下次运行时上传" });
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
    archive: "上传",
    publish: "同步",
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
    skipped: "已跳过",
  };
  return map[s] || s;
}

function itemStatusTone(s: string): "neutral" | "accent" | "success" | "warning" | "danger" {
  if (s === "missing") return "warning";
  if (s === "skipped") return "neutral";
  return "danger";
}

/** 列表行摘要：只保留用户可理解的计数。 */
function runSummaryText(run: TaskRunDto) {
  if (run.errorMessage) return run.errorMessage;
  if (!run.summaryJson) return "—";
  try {
    const s = JSON.parse(run.summaryJson) as Record<string, unknown>;
    const scan = s.scan as { newObjects?: number } | null;
    const archive = s.archive as { uploaded?: number; failed?: number } | null;
    const parts: string[] = [];
    if (scan && typeof scan.newObjects === "number") parts.push(`新文件 ${scan.newObjects}`);
    if (archive) {
      const uploaded = archive.uploaded ?? 0;
      const failed = archive.failed ?? 0;
      parts.push(uploaded > 0 || failed > 0 ? `上传 ${uploaded}` : "无待传");
      if (failed > 0) parts.push(`上传失败 ${failed}`);
    }
    if (typeof s.failedItems === "number" && s.failedItems > 0) parts.push(`异常 ${s.failedItems}`);
    return parts.join(" · ") || "—";
  } catch {
    return "—";
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
    description: "当前阶段将停止，已上传的内容保留，队列中的文件待下次运行继续。",
    confirmLabel: "结束运行",
    danger: true,
  });
  if (!ok) return;
  cancelling.value = true;
  try {
    await cancelTaskRun(runId);
    pushToast({ tone: "success", title: "已请求结束，等待运行停止" });
    await refreshActiveRun();
  } catch (err) {
    pushToast({ tone: "danger", title: "结束运行失败", description: String(err) });
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
