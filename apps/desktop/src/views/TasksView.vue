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
        <UiButton size="sm" variant="ghost" :disabled="running || !webdavReady" @click="doRestore">
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

    <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
      <!-- 上区：采集范围 + 调度，按窗口比例约占 55%，两列等高、各自内部滚动 -->
      <div
        class="grid min-h-[16rem] grid-cols-1 gap-3 md:grid-cols-2"
        style="flex: 0 1 55%"
      >
        <!-- 采集范围 -->
        <UiCard
          info="扫描会检查以下目录；账号勾选同时作用于立即运行与定时采集。新增来源类型在适配器注册表中扩展。"
          body-class="overflow-hidden"
        >
          <template #header>
            <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <div class="flex shrink-0 items-center gap-1.5">
                <h3 class="text-cv-section text-cv-text">采集范围</h3>
                <UiInfoTip
                  text="扫描会检查以下目录；账号勾选同时作用于立即运行与定时采集。"
                />
              </div>
              <div class="flex flex-wrap items-center justify-end gap-2">
                <UiMenu>
                  <template #trigger="{ toggle }">
                    <UiButton size="sm" variant="secondary" @click="toggle">
                      添加采集源
                    </UiButton>
                  </template>
                  <template #default="{ close }">
                    <button
                      v-for="adapter in adapters"
                      :key="adapter.id"
                      type="button"
                      class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-cv-body text-cv-text hover:bg-cv-surface-2"
                      @click="pickAndAddSource(adapter.id); close()"
                    >
                      <span
                        class="h-1.5 w-1.5 shrink-0 rounded-full"
                        :class="adapter.badgeTone === 'accent' ? 'bg-cv-accent' : 'bg-cv-text-3'"
                      />
                      {{ adapter.label }}
                    </button>
                    <template v-if="discoverableAdapters.length">
                      <div class="my-1 border-t border-cv-border" />
                      <button
                        v-for="adapter in discoverableAdapters"
                        :key="`discover-${adapter.id}`"
                        type="button"
                        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-cv-body text-cv-text-2 hover:bg-cv-surface-2 hover:text-cv-text"
                        :disabled="detecting"
                        @click="discoverAndAdd(adapter.id); close()"
                      >
                        <span class="text-cv-caption text-cv-text-3">发现</span>
                        {{ adapter.label }}
                      </button>
                    </template>
                  </template>
                </UiMenu>
              </div>
            </div>
          </template>

          <div v-if="collectSources.length" class="min-h-0 flex-1 space-y-2 overflow-y-auto">
            <CollectSourceCard
              v-for="(source, index) in collectSources"
              :key="sourceKey(source)"
              :source="source"
              :is-account-source="isAccountAdapter(source.sourceType)"
              :show-videos="supportsVideos(source.sourceType)"
              :video-hint="sourceVideoHint(source.sourceType)"
              :status-detail="sourceStatusDetail(source)"
              :refresh-label="
                source.status === 'missing'
                  ? '重新选择'
                  : isAccountAdapter(source.sourceType)
                    ? '重新识别'
                    : '重新检查'
              "
              :source-badge-label="sourceBadgeLabel(source.sourceType)"
              :source-badge-tone="sourceBadgeTone(source.sourceType)"
              :source-status-label="sourceStatusLabel(source.status)"
              :source-status-tone="sourceStatusTone(source.status)"
              :selected-count="selectedCountInSource(source)"
              :account-key="accountKey"
              :is-account-selected="isAccountSelected"
              @refresh="refreshSource(index)"
              @replace="pickAndReplaceSource(index)"
              @remove="removeSource(index)"
              @toggle-videos="toggleSourceVideos(source)"
              @toggle-account="toggleAccount"
              @select-all="setSourceAccountsSelected(source, true)"
              @select-none="setSourceAccountsSelected(source, false)"
            />
          </div>
          <div v-else class="flex min-h-0 flex-1 flex-col justify-center py-3">
            <p class="text-cv-caption text-cv-text-2">尚未添加采集源</p>
            <p class="mt-1 text-cv-caption text-cv-text-3">
              从上方「添加采集源」选择微信、企业微信或附件目录后即可扫描。
            </p>
          </div>
        </UiCard>

        <!-- 调度与运行 -->
        <UiCard
          title="调度与运行"
          info="定时与「立即运行」共用同一条流水线：扫描 → 上传 → 同步元数据。未配置 WebDAV 时只做本地扫描。"
          body-class="overflow-y-auto"
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
                <span class="block text-cv-body font-medium text-cv-text-2">执行一次全量扫描</span>
                <span class="mt-0.5 block text-cv-caption text-cv-text-3">忽略增量记录，重新检查全部附件</span>
              </span>
            </label>

          </div>
        </UiCard>
      </div>

      <!-- 下区：运行历史 / 上传队列，占剩余高度（约 45%），表格内部滚动 -->
      <UiCard
        body-class="!p-0"
        class="min-h-[10rem] flex-1"
      >
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
        <div v-if="activeTab === 'history'" class="flex min-h-0 flex-1 flex-col p-4 pt-2">
          <p
            v-if="!historyLoading && !runs.length"
            class="py-6 text-center text-cv-caption text-cv-text-3"
          >
            暂无运行记录，执行一次「立即运行」后可在此回看。
          </p>
          <div v-else class="min-h-0 flex-1 overflow-auto rounded-cv border border-cv-border">
            <table class="w-full table-fixed text-left text-cv-caption">
              <thead class="sticky top-0 border-b border-cv-border bg-cv-surface-2 text-cv-text-2">
                <tr>
                  <th class="w-24 px-2.5 py-2 font-medium whitespace-nowrap">时间</th>
                  <th class="w-24 px-2.5 py-2 font-medium whitespace-nowrap">状态</th>
                  <th class="w-16 px-2.5 py-2 font-medium whitespace-nowrap">触发</th>
                  <th class="px-2.5 py-2 font-medium">结果</th>
                  <th class="w-14 px-2.5 py-2 font-medium whitespace-nowrap">失败</th>
                  <th class="w-24 px-2.5 py-2 font-medium whitespace-nowrap">耗时</th>
                  <th class="w-20 px-2.5 py-2 font-medium"></th>
                </tr>
              </thead>
              <tbody>
                <template v-for="run in runs" :key="run.runId">
                  <tr
                    class="cursor-pointer border-b border-cv-border/60 last:border-0 hover:bg-cv-surface-2"
                    @click="openRunDetail(run)"
                  >
                    <td class="truncate px-2.5 py-2 text-cv-text-2 whitespace-nowrap">
                      {{ formatDateTime(run.startedAt, { compact: true }) }}
                    </td>
                    <td class="px-2.5 py-2">
                      <UiBadge :tone="runStatusTone(run.status)">{{ runStatusLabel(run.status) }}</UiBadge>
                    </td>
                    <td class="px-2.5 py-2 text-cv-text-3 whitespace-nowrap">
                      {{ run.triggerSource === "schedule" ? "定时" : "手动" }}
                    </td>
                    <td
                      class="truncate px-2.5 py-2 text-cv-text"
                      :title="run.errorMessage || runSummaryText(run)"
                    >
                      {{ runSummaryText(run) }}
                    </td>
                    <td class="px-2.5 py-2 whitespace-nowrap" :class="run.failedItems > 0 ? 'text-cv-danger' : 'text-cv-text-3'">
                      {{ run.failedItems > 0 ? run.failedItems : "—" }}
                    </td>
                    <td class="px-2.5 py-2 text-cv-text-3 whitespace-nowrap">
                      {{ run.durationMs != null ? formatDurationMs(run.durationMs) : "—" }}
                    </td>
                    <td class="px-2.5 py-2 text-right whitespace-nowrap">
                      <UiButton size="sm" variant="ghost" @click.stop="openRunDetail(run)">详情</UiButton>
                    </td>
                  </tr>
                </template>
              </tbody>
            </table>
          </div>
        </div>

        <UploadQueueTable
          v-else
          :tasks="tasks"
          :status-filter="statusFilter"
          :tasks-loading="tasksLoading"
          :busy-task-ids="busyTaskIds"
          @requeue="requeue"
          @pause="pause"
          @delete="deleteMissingTask"
        />
      </UiCard>
    </div>

    <!-- 运行历史详情弹窗 -->
    <div
      v-if="detailRun"
      class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4"
      @click.self="closeRunDetail"
    >
      <div
        class="flex max-h-[85vh] w-full max-w-4xl flex-col rounded-cv-lg border border-cv-border bg-cv-surface shadow-xl"
        role="dialog"
        aria-modal="true"
        aria-label="运行详情"
      >
        <header class="flex items-start justify-between gap-3 border-b border-cv-border px-5 py-4">
          <div class="min-w-0">
            <h3 class="text-cv-section text-cv-text">运行详情</h3>
            <div class="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-cv-caption text-cv-text-3">
              <span>{{ formatDateTime(detailRun.startedAt) }}</span>
              <span>·</span>
              <span>{{ detailRun.triggerSource === "schedule" ? "定时" : "手动" }}</span>
              <template v-if="detailRun.durationMs != null">
                <span>·</span>
                <span>{{ formatDurationMs(detailRun.durationMs) }}</span>
              </template>
            </div>
            <p class="mt-1.5 truncate text-cv-caption text-cv-text-2" :title="detailRun.errorMessage || runSummaryText(detailRun)">
              {{ runSummaryText(detailRun) }}
            </p>
          </div>
          <div class="flex shrink-0 items-center gap-2 whitespace-nowrap">
            <UiBadge :tone="runStatusTone(detailRun.status)">{{ runStatusLabel(detailRun.status) }}</UiBadge>
            <UiButton size="sm" variant="ghost" @click="closeRunDetail">关闭</UiButton>
          </div>
        </header>

        <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <p v-if="detailLoading" class="text-cv-caption text-cv-text-3">加载中…</p>
          <p v-else-if="detailError" class="text-cv-caption text-cv-danger">{{ detailError }}</p>
          <template v-else-if="detail">
            <div
              v-if="visibleStages(detail.stages).length"
              class="flex flex-wrap items-center gap-x-3 gap-y-1 text-cv-caption text-cv-text-3"
            >
              <span
                v-for="st in visibleStages(detail.stages)"
                :key="st.stage"
                :title="st.message || ''"
              >
                <span class="text-cv-text-2">{{ stageLabel(st.stage) }}</span>
                <template v-if="st.status !== 'success'">
                  <span class="mx-0.5">·</span>
                  <span :class="stageStatusClass(st.status)">{{ stageStatusLabel(st.status) }}</span>
                </template>
                <span v-if="st.durationMs != null" class="ml-1">{{ formatDurationMs(st.durationMs) }}</span>
              </span>
            </div>

            <template v-if="detail.items.length">
              <div class="mt-3 mb-1.5 flex flex-wrap items-center gap-2">
                <span class="text-cv-text-2">异常明细</span>
                <div class="w-28">
                  <UiSelect v-model="itemStatusFilter">
                    <option value="">全部</option>
                    <option value="failed">上传失败</option>
                    <option value="missing">本地缺失</option>
                  </UiSelect>
                </div>
                <span class="text-cv-text-3">共 {{ filteredDetailItems.length }} 条</span>
              </div>
              <div v-if="filteredDetailItems.length" class="max-h-[22rem] overflow-auto rounded-cv border border-cv-border">
                <table class="w-full table-fixed text-left text-cv-caption">
                  <thead class="sticky top-0 border-b border-cv-border bg-cv-surface-2 text-cv-text-2">
                    <tr>
                      <th class="w-[30%] px-2.5 py-1.5 font-medium">文件</th>
                      <th class="w-[14%] px-2.5 py-1.5 font-medium whitespace-nowrap">状态</th>
                      <th class="px-2.5 py-1.5 font-medium">原因</th>
                      <th class="w-[22%] px-2.5 py-1.5 font-medium whitespace-nowrap">操作</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="item in filteredDetailItems"
                      :key="item.itemId"
                      class="border-b border-cv-border/60 last:border-0"
                    >
                      <td class="truncate px-2.5 py-1.5 text-cv-text" :title="item.name">
                        {{ item.name }}
                      </td>
                      <td class="px-2.5 py-1.5">
                        <UiBadge :tone="itemStatusTone(item.status)">
                          {{ itemStatusLabel(item.status) }}
                        </UiBadge>
                      </td>
                      <td
                        class="truncate px-2.5 py-1.5 text-cv-text-3"
                        :title="item.errorMessage || item.errorCode || ''"
                      >
                        {{ item.errorMessage || item.errorCode || "—" }}
                      </td>
                      <td class="px-2.5 py-1.5">
                        <div class="flex flex-nowrap items-center gap-1 whitespace-nowrap">
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
              <p v-else class="text-cv-caption text-cv-text-3">无匹配明细</p>
            </template>
            <p
              v-else-if="!visibleStages(detail.stages).length"
              class="text-cv-caption text-cv-text-3"
            >
              无阶段记录
            </p>
          </template>
        </div>
      </div>
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
import UiInfoTip from "../components/ui/UiInfoTip.vue";
import UiMenu from "../components/ui/UiMenu.vue";
import CollectSourceCard from "../components/CollectSourceCard.vue";
import UploadQueueTable from "../components/UploadQueueTable.vue";
import { useUploadQueue } from "../composables/useUploadQueue";
import {
  getAppSettings,
  getCollectSelectedAccounts,
  getCollectSourceCache,
  getScheduleStatus,
  listTaskRuns,
  getTaskRunDetail,
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
import {
  isWebdavConfigured,
  markCollectSourcesConfigured,
  markWebdavConfigured,
  webdavConnection,
} from "../composables/useSetupStatus";
import { usePipelineProgress } from "../composables/usePipelineProgress";
import { formatDurationMs, formatDateTime } from "../utils/format";
import type {
  PipelineResultDto,
  TaskRunDto,
  TaskRunDetailDto,
  TaskRunStageDto,
  WebdavConfigDto,
  ActiveRunDto,
} from "../types";

type StageId = "idle" | "scan" | "done" | "failed";
type CollectSourceLike = ReturnType<typeof useCollectSources>["collectSources"]["value"][number];

const {
  adapters,
  discoverableAdapters,
  allWechatAccounts,
  accountKey,
  canRun,
  collectSources,
  detecting,
  discoverAndAdd,
  isAccountAdapter,
  isAccountSelected,
  loadSources,
  pickAndAddSource,
  pickAndReplaceSource,
  refreshSource,
  removeSource,
  selectedAccounts,
  setSourceAccountsSelected,
  sourceBadgeLabel,
  sourceBadgeTone,
  sourceKey,
  sourceStatusDetail,
  sourceStatusLabel,
  sourceStatusTone,
  sourceVideoHint,
  supportsVideos,
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
// 与设置页共用配置状态，避免 keep-alive 下徽标过期
const webdavReady = isWebdavConfigured;
const webdavConfig = computed<WebdavConfigDto>(() => ({
  url: webdavConnection.value.url,
  username: webdavConnection.value.username,
  password: "",
  vaultId: webdavConnection.value.vaultId || "chatvault-default",
}));

const activeTab = ref<"history" | "queue">("history");

const {
  tasks, statusFilter, pendingQueueCount, tasksLoading, busyTaskIds,
  refreshTasks, requeue, pause, deleteMissingTask,
} = useUploadQueue();
let pollTimer: number | undefined;
let scheduleSaveTimer: number | undefined;

const runs = ref<TaskRunDto[]>([]);
const historyLoading = ref(false);
const detailRun = ref<TaskRunDto | null>(null);
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

function selectedCountInSource(source: CollectSourceLike) {
  if (!source.accounts.length) return 0;
  return source.accounts.filter((account) => isAccountSelected(account)).length;
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
    // 先同步连接状态，避免后续采集源加载失败导致一直显示未配置
    const webdavOk = Boolean(s.webdavUrl && s.webdavUrl.trim());
    markWebdavConfigured(webdavOk, {
      url: s.webdavUrl || "",
      username: s.webdavUsername || "",
      vaultId: s.vaultId || "",
    });
    markCollectSourcesConfigured((s.collectSources?.length ?? 0) > 0);
    scheduleEnabled.value = s.scheduleEnabled;
    scanIntervalMinutes.value = s.scanIntervalMinutes;
    try {
      await loadSources(s.collectSources || [], cache || [], persistedSelections);
    } catch (err) {
      pushToast({ tone: "warning", title: "加载采集源失败", description: String(err) });
    }
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
  if (run.status === "running") return run.cancelRequested ? "正在取消…" : "执行中…";
  const emptySummary = `${runStatusLabel(run.status)}，暂无统计摘要`;
  if (!run.summaryJson) return emptySummary;
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
    return parts.join(" · ") || emptySummary;
  } catch {
    return `${runStatusLabel(run.status)}，统计摘要无法解析`;
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

async function openRunDetail(run: TaskRunDto) {
  detailRun.value = run;
  detail.value = null;
  detailError.value = "";
  detailLoading.value = true;
  itemStatusFilter.value = "";
  try {
    detail.value = await getTaskRunDetail(run.runId);
  } catch (err) {
    detailError.value = String(err);
  } finally {
    detailLoading.value = false;
  }
}

function closeRunDetail() {
  detailRun.value = null;
  detail.value = null;
  detailError.value = "";
}

/** Esc 关闭运行详情弹窗 */
function onDetailKeydown(e: KeyboardEvent) {
  if (e.key === "Escape" && detailRun.value) closeRunDetail();
}

onMounted(async () => {
  window.addEventListener("keydown", onDetailKeydown);
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

// keep-alive：切回任务页时恢复轮询，并重读设置以免 WebDAV/采集源状态过期
onActivated(() => {
  startPolling();
  void loadSettings();
  void refreshTasks(true);
  void refreshHistory(true);
  void refreshActiveRun();
});

onDeactivated(() => {
  closeRunDetail();
  stopPolling();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onDetailKeydown);
  stopPolling();
  if (scheduleSaveTimer) window.clearTimeout(scheduleSaveTimer);
});
</script>
