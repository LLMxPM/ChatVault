// ChatVault 采集源状态与交互逻辑
// 负责微信 4.x/通用附件目录的选择、校验、持久化、账号选择和状态刷新。

import { computed, ref } from "vue";
import {
  checkDirectory,
  detectWechatAccounts,
  inspectWechatDirectory,
  pickDirectory,
  setCollectSources,
} from "../api/tauri";
import { pushToast } from "./useToast";
import type {
  CollectSourceDto,
  CollectSourceType,
  WechatAccountDto,
  WechatAccountTargetDto,
} from "../types";

type SourceStatus = "checking" | "ready" | "empty" | "missing" | "error";

type CollectSourceItem = CollectSourceDto & {
  accounts: WechatAccountDto[];
  status: SourceStatus;
  errorMessage: string;
  inspecting: boolean;
};

/** 提供采集源列表、目录操作和微信账号临时选择状态。 */
export function useCollectSources() {
  const selectedAccounts = ref<WechatAccountTargetDto[]>([]);
  const detecting = ref(false);
  const collectSources = ref<CollectSourceItem[]>([]);
  let selectionsInitialized = false;

  const allWechatAccounts = computed(() =>
    collectSources.value
      .filter((source) => source.sourceType === "wechat-windows-4")
      .flatMap((source) => source.accounts),
  );
  const canRun = computed(
    () =>
      selectedAccounts.value.length > 0 ||
      collectSources.value.some(
        (source) => source.sourceType === "generic-folder" && source.status === "ready",
      ),
  );

  function sourceTypeLabel(sourceType: CollectSourceType) {
    return sourceType === "wechat-windows-4" ? "微信 4.x" : "附件目录";
  }

  function sourceStatusLabel(status: SourceStatus) {
    const labels: Record<SourceStatus, string> = {
      checking: "检查中",
      ready: "已识别",
      empty: "未发现账号",
      missing: "目录不存在",
      error: "识别失败",
    };
    return labels[status];
  }

  function sourceStatusTone(status: SourceStatus) {
    if (status === "ready") return "success" as const;
    if (status === "empty" || status === "checking") return "warning" as const;
    if (status === "missing" || status === "error") return "danger" as const;
    return "neutral" as const;
  }

  function sourceStatusDetail(source: CollectSourceItem) {
    if (source.status === "checking") return "正在检查目录可用性…";
    if (source.status === "missing") return "目录可能已被移动或删除，可重新选择目录。";
    if (source.status === "error") return source.errorMessage || "请确认目录类型正确后重试。";
    if (source.sourceType === "wechat-windows-4") {
      return source.accounts.length
        ? `${source.accounts.length} 个微信账号；立即归档可按账号选择`
        : "目录有效，但暂未发现微信账号。";
    }
    return "递归扫描该目录中的附件文件。";
  }

  function normalizePath(path: string) {
    return path.replace(/\//g, "\\").replace(/[\\]+$/, "").toLowerCase();
  }

  function isUnderRoot(path: string, root: string) {
    const pathKey = normalizePath(path);
    const rootKey = normalizePath(root);
    return pathKey === rootKey || pathKey.startsWith(`${rootKey}\\`);
  }

  function sourceKey(source: CollectSourceDto) {
    return normalizePath(source.path);
  }

  function accountKey(account: WechatAccountDto) {
    return `${normalizePath(account.sourceRoot)}\u0000${account.sourceAccountId}`;
  }

  function targetKey(target: WechatAccountTargetDto) {
    return `${normalizePath(target.sourceRoot)}\u0000${target.sourceAccountId}`;
  }

  function createSourceItem(source: CollectSourceDto): CollectSourceItem {
    return {
      ...source,
      // 微信图片解密默认关闭，由用户显式开启；通用目录不使用该字段。
      enableImages: source.sourceType === "wechat-windows-4" ? source.enableImages === true : true,
      accounts: [],
      status: "checking",
      errorMessage: "",
      inspecting: false,
    };
  }

  function cloneSources(sources: CollectSourceItem[]) {
    return sources.map((source) => ({ ...source, accounts: [...source.accounts] }));
  }

  function sourcePayload(sources: CollectSourceItem[]): CollectSourceDto[] {
    return sources.map(({ sourceType, path, enableImages }) => ({
      sourceType,
      path,
      enableImages: sourceType === "wechat-windows-4" ? enableImages === true : true,
    }));
  }

  function hasPathConflict(path: string, ignoredIndex = -1) {
    return collectSources.value.some(
      (source, index) =>
        index !== ignoredIndex &&
        (isUnderRoot(path, source.path) || isUnderRoot(source.path, path)),
    );
  }

  function toAccountTarget(account: WechatAccountDto): WechatAccountTargetDto {
    return {
      sourceRoot: account.sourceRoot,
      sourceAccountId: account.sourceAccountId,
    };
  }

  function isAccountSelected(account: WechatAccountDto) {
    const key = accountKey(account);
    return selectedAccounts.value.some((target) => targetKey(target) === key);
  }

  function toggleAccount(account: WechatAccountDto) {
    const key = accountKey(account);
    const index = selectedAccounts.value.findIndex((target) => targetKey(target) === key);
    if (index >= 0) selectedAccounts.value.splice(index, 1);
    else selectedAccounts.value.push(toAccountTarget(account));
  }

  /** 切换微信采集源的聊天图片解密开关并持久化。 */
  async function toggleSourceImages(source: CollectSourceItem) {
    if (source.sourceType !== "wechat-windows-4") return;
    const previousSources = cloneSources(collectSources.value);
    const previousSelections = selectedAccounts.value.map((target) => ({ ...target }));
    source.enableImages = source.enableImages !== true;
    if (!(await persistSources(previousSources, previousSelections))) {
      return;
    }
  }

  /** 根据当前已识别账号清理失效的临时勾选项。 */
  function syncSelectedAccounts() {
    if (!selectionsInitialized) return;
    const available = new Set(allWechatAccounts.value.map(accountKey));
    selectedAccounts.value = selectedAccounts.value.filter((target) => available.has(targetKey(target)));
  }

  /** 选择并添加微信 4.x 根目录；目录校验成功后才写入配置。 */
  async function pickAndAddWechat() {
    try {
      const path = await pickDirectory("选择采集目录");
      if (!path) return;
      if (hasPathConflict(path)) {
        pushToast({ tone: "warning", title: "目录已存在或与现有目录重叠" });
        return;
      }
      const accounts = await inspectWechatDirectory(path);
      await addWechatSource(path, accounts);
    } catch (err) {
      pushToast({ tone: "danger", title: "添加微信目录失败", description: String(err) });
    }
  }

  /** 选择并添加通用附件目录；路径只能来自系统目录选择器。 */
  async function pickAndAddAttachment() {
    try {
      const path = await pickDirectory("选择采集目录");
      if (!path) return;
      if (hasPathConflict(path)) {
        pushToast({ tone: "warning", title: "目录已存在或与现有目录重叠" });
        return;
      }
      if (!(await checkDirectory(path))) {
        pushToast({ tone: "danger", title: "附件目录不可用" });
        return;
      }
      await addSource({
        sourceType: "generic-folder",
        path,
        enableImages: true,
        accounts: [],
        status: "ready",
        errorMessage: "",
        inspecting: false,
      });
    } catch (err) {
      pushToast({ tone: "danger", title: "添加附件目录失败", description: String(err) });
    }
  }

  /** 自动发现默认微信根目录并快捷加入；不会隐式改变未展示的扫描范围。 */
  async function discoverAndAddWechat() {
    detecting.value = true;
    try {
      const accounts = await detectWechatAccounts();
      if (!accounts.length) {
        pushToast({ tone: "warning", title: "未发现微信 4.x 目录" });
        return;
      }
      const path = accounts[0].sourceRoot;
      if (hasPathConflict(path)) {
        const existingIndex = collectSources.value.findIndex(
          (source) => normalizePath(source.path) === normalizePath(path),
        );
        if (existingIndex >= 0) {
          await refreshSource(existingIndex);
          pushToast({ tone: "success", title: "微信目录已重新识别" });
        } else {
          pushToast({ tone: "warning", title: "自动发现的目录与现有目录重叠" });
        }
        return;
      }
      await addWechatSource(path, accounts);
    } catch (err) {
      pushToast({ tone: "danger", title: "自动发现微信目录失败", description: String(err) });
    } finally {
      detecting.value = false;
    }
  }

  /** 将微信目录及其账号加入统一采集源列表。 */
  async function addWechatSource(path: string, accounts: WechatAccountDto[]) {
    await addSource({
      sourceType: "wechat-windows-4",
      path,
      // 默认关闭聊天图片解密；用户在任务页显式开启后才会读取本机统计参数。
      enableImages: false,
      accounts,
      status: accounts.length ? "ready" : "empty",
      errorMessage: "",
      inspecting: false,
    });
  }

  /** 保存采集源；保存失败时恢复列表和临时账号选择。 */
  async function addSource(source: CollectSourceItem) {
    const previousSources = cloneSources(collectSources.value);
    const previousSelections = selectedAccounts.value.map((target) => ({ ...target }));
    collectSources.value = [...collectSources.value, source];
    if (!(await persistSources(previousSources, previousSelections))) return;

    if (source.accounts.length) {
      const selected = new Set(selectedAccounts.value.map(targetKey));
      selectedAccounts.value = [
        ...selectedAccounts.value,
        ...source.accounts
          .filter((account) => !selected.has(accountKey(account)))
          .map(toAccountTarget),
      ];
    }
    selectionsInitialized = true;
  }

  /** 检查并刷新一个采集源的可用状态与微信账号。 */
  async function refreshSource(index: number) {
    const source = collectSources.value[index];
    if (!source) return;
    source.inspecting = true;
    source.status = "checking";
    source.errorMessage = "";
    try {
      if (!(await checkDirectory(source.path))) {
        source.accounts = [];
        source.status = "missing";
        return;
      }
      if (source.sourceType === "wechat-windows-4") {
        source.accounts = await inspectWechatDirectory(source.path);
        source.status = source.accounts.length ? "ready" : "empty";
      } else {
        source.accounts = [];
        source.status = "ready";
      }
    } catch (err) {
      source.accounts = [];
      source.status = "error";
      source.errorMessage = String(err);
    } finally {
      source.inspecting = false;
      syncSelectedAccounts();
    }
  }

  /** 为目录不存在的采集源重新选择路径，并在保存失败时恢复原记录。 */
  async function pickAndReplaceSource(index: number) {
    const source = collectSources.value[index];
    if (!source) return;
    source.inspecting = true;
    try {
      const path = await pickDirectory("选择采集目录");
      if (!path) return;
      if (hasPathConflict(path, index)) {
        pushToast({ tone: "warning", title: "目录已存在或与现有目录重叠" });
        return;
      }

      const replacement = createSourceItem({
        sourceType: source.sourceType,
        path,
        enableImages: source.enableImages !== false,
      });
      if (source.sourceType === "wechat-windows-4") {
        replacement.accounts = await inspectWechatDirectory(path);
        replacement.status = replacement.accounts.length ? "ready" : "empty";
      } else if (!(await checkDirectory(path))) {
        pushToast({ tone: "danger", title: "附件目录不可用" });
        return;
      } else {
        replacement.status = "ready";
      }
      await replaceSource(index, replacement);
    } catch (err) {
      pushToast({ tone: "danger", title: "重新选择目录失败", description: String(err) });
    } finally {
      source.inspecting = false;
    }
  }

  /** 替换采集源并持久化；失败时回滚原路径、状态与账号选择。 */
  async function replaceSource(index: number, replacement: CollectSourceItem) {
    const previousSources = cloneSources(collectSources.value);
    const previousSelections = selectedAccounts.value.map((target) => ({ ...target }));
    collectSources.value = collectSources.value.map((source, sourceIndex) =>
      sourceIndex === index ? replacement : source,
    );
    syncSelectedAccounts();
    if (!(await persistSources(previousSources, previousSelections))) return;

    const selected = new Set(selectedAccounts.value.map(targetKey));
    selectedAccounts.value = [
      ...selectedAccounts.value,
      ...replacement.accounts
        .filter((account) => !selected.has(accountKey(account)))
        .map(toAccountTarget),
    ];
  }

  /** 加载并检查设置中的全部采集源，首次默认勾选所有微信账号。 */
  async function loadSources(sources: CollectSourceDto[]) {
    collectSources.value = sources.map(createSourceItem);
    selectionsInitialized = false;
    await Promise.all(collectSources.value.map((_, index) => refreshSource(index)));
    selectedAccounts.value = allWechatAccounts.value.map(toAccountTarget);
    selectionsInitialized = true;
  }

  async function removeSource(index: number) {
    const previousSources = cloneSources(collectSources.value);
    const previousSelections = selectedAccounts.value.map((target) => ({ ...target }));
    collectSources.value = collectSources.value.filter((_, sourceIndex) => sourceIndex !== index);
    syncSelectedAccounts();
    await persistSources(previousSources, previousSelections);
  }

  async function persistSources(previousSources: CollectSourceItem[], previousSelections: WechatAccountTargetDto[]) {
    try {
      await setCollectSources(sourcePayload(collectSources.value));
      return true;
    } catch (err) {
      collectSources.value = previousSources;
      selectedAccounts.value = previousSelections;
      pushToast({ tone: "danger", title: "保存采集源失败", description: String(err) });
      return false;
    }
  }

  return {
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
  };
}
