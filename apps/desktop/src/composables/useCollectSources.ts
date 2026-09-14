// ChatVault 采集源状态与交互逻辑
// 负责按适配器注册表选择、校验、持久化、账号选择和状态刷新。
// 首屏用上次探测快照秒开，仅做目录存在性轻量校验；完整账号识别按需触发。

import { computed, ref } from "vue";
import {
  checkDirectory,
  pickDirectory,
  setCollectSelectedAccounts,
  setCollectSourceCache,
  setCollectSources,
} from "../api/tauri";
import {
  COLLECT_ADAPTERS,
  discoverableCollectAdapters,
  getCollectAdapter,
  isAccountAdapter,
} from "../adapters/collectAdapters";
import { pushToast } from "./useToast";
import type {
  CollectSourceCacheDto,
  CollectSourceDto,
  CollectSourceStatus,
  CollectSourceType,
  WechatAccountDto,
  WechatAccountTargetDto,
} from "../types";

type SourceStatus = CollectSourceStatus;

type CollectSourceItem = CollectSourceDto & {
  accounts: WechatAccountDto[];
  status: SourceStatus;
  errorMessage: string;
  inspecting: boolean;
};

/** 提供采集源列表、目录操作和账号选择状态。 */
export function useCollectSources() {
  const selectedAccounts = ref<WechatAccountTargetDto[]>([]);
  const detecting = ref(false);
  const collectSources = ref<CollectSourceItem[]>([]);
  let selectionsInitialized = false;

  const allWechatAccounts = computed(() =>
    collectSources.value
      .filter((source) => isAccountAdapter(source.sourceType))
      .flatMap((source) => source.accounts),
  );
  const canRun = computed(
    () =>
      selectedAccounts.value.length > 0 ||
      collectSources.value.some(
        (source) => source.sourceType === "generic-folder" && source.status === "ready",
      ),
  );

  /** 已注册适配器（添加菜单用）。 */
  const adapters = COLLECT_ADAPTERS;
  /** 支持自动发现的适配器。 */
  const discoverableAdapters = discoverableCollectAdapters();

  function sourceTypeLabel(sourceType: CollectSourceType | string) {
    return getCollectAdapter(sourceType)?.label ?? sourceType;
  }

  function sourceBadgeTone(sourceType: CollectSourceType | string) {
    return getCollectAdapter(sourceType)?.badgeTone ?? "neutral";
  }

  function sourceBadgeLabel(sourceType: CollectSourceType | string) {
    return getCollectAdapter(sourceType)?.badge ?? sourceType;
  }

  function sourceVideoHint(sourceType: CollectSourceType | string) {
    return getCollectAdapter(sourceType)?.videoHint ?? "";
  }

  function supportsVideos(sourceType: CollectSourceType | string) {
    return getCollectAdapter(sourceType)?.supportsVideos ?? false;
  }

  function sourceStatusLabel(status: SourceStatus) {
    const labels: Record<SourceStatus, string> = {
      checking: "检查中",
      ready: "可用",
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
    const adapter = getCollectAdapter(source.sourceType);
    if (!adapter) return "";
    if (adapter.kind === "account") {
      return source.accounts.length ? "" : (adapter.emptyAccountDetail ?? "");
    }
    return adapter.folderDetail ?? "";
  }

  function normalizePath(path: string) {
    return path.replace(/\//g, "\\").replace(/[/\\]+$/, "").toLowerCase();
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
    return `${normalizePath(account.sourceRoot)}|${account.sourceAccountId}`;
  }

  function targetKey(target: WechatAccountTargetDto) {
    return `${normalizePath(target.sourceRoot)}|${target.sourceAccountId}`;
  }

  function createSourceItem(source: CollectSourceDto): CollectSourceItem {
    const adapter = getCollectAdapter(source.sourceType);
    return {
      ...source,
      // 支持视频的适配器默认开启；目录型保持 true（后端忽略）。
      enableVideos: adapter?.supportsVideos ? source.enableVideos !== false : true,
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
    return sources.map(({ sourceType, path, enableVideos }) => ({
      sourceType,
      path,
      enableVideos: supportsVideos(sourceType) ? enableVideos !== false : true,
    }));
  }

  /** 将当前源状态序列化为探测快照。 */
  function sourceCachePayload(): CollectSourceCacheDto[] {
    const now = Date.now();
    return collectSources.value.map((source) => ({
      path: source.path,
      status: source.status,
      errorMessage: source.errorMessage,
      accounts: [...source.accounts],
      inspectedAt: now,
    }));
  }

  async function persistSourceCache() {
    try {
      await setCollectSourceCache(sourceCachePayload());
    } catch {
      // 快照失败不影响主流程；下次进页会退回全量识别。
    }
  }

  async function persistSelectedAccounts() {
    try {
      await setCollectSelectedAccounts(selectedAccounts.value.map((target) => ({ ...target })));
    } catch (err) {
      pushToast({ tone: "warning", title: "保存账号勾选失败", description: String(err) });
    }
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
    void persistSelectedAccounts();
  }

  /** 全选/清空单个源下账号。 */
  function setSourceAccountsSelected(source: CollectSourceItem, selectAll: boolean) {
    const keys = new Set(source.accounts.map(accountKey));
    if (selectAll) {
      const existing = new Set(selectedAccounts.value.map(targetKey));
      for (const account of source.accounts) {
        if (!existing.has(accountKey(account))) {
          selectedAccounts.value.push(toAccountTarget(account));
        }
      }
    } else {
      selectedAccounts.value = selectedAccounts.value.filter((target) => !keys.has(targetKey(target)));
    }
    void persistSelectedAccounts();
  }

  /** 切换账号型采集源的视频识别开关并持久化。 */
  async function toggleSourceVideos(source: CollectSourceItem) {
    if (!supportsVideos(source.sourceType)) return;
    const previousSources = cloneSources(collectSources.value);
    const previousSelections = selectedAccounts.value.map((target) => ({ ...target }));
    source.enableVideos = source.enableVideos === false;
    if (!(await persistSources(previousSources, previousSelections))) {
      return;
    }
  }

  /** 根据当前已识别账号清理失效的勾选项。 */
  function syncSelectedAccounts() {
    if (!selectionsInitialized) return;
    const available = new Set(allWechatAccounts.value.map(accountKey));
    const next = selectedAccounts.value.filter((target) => available.has(targetKey(target)));
    if (next.length !== selectedAccounts.value.length) {
      selectedAccounts.value = next;
      void persistSelectedAccounts();
    }
  }

  /** 选择并添加指定适配器的根目录。 */
  async function pickAndAddSource(sourceType: CollectSourceType) {
    const adapter = getCollectAdapter(sourceType);
    if (!adapter) return;
    try {
      const path = await pickDirectory(`选择${adapter.label}采集目录`);
      if (!path) return;
      if (hasPathConflict(path)) {
        pushToast({ tone: "warning", title: "目录已存在或与现有目录重叠" });
        return;
      }
      if (adapter.kind === "folder") {
        if (!(await checkDirectory(path))) {
          pushToast({ tone: "danger", title: "附件目录不可用" });
          return;
        }
        await addSource({
          sourceType,
          path,
          accounts: [],
          status: "ready",
          errorMessage: "",
          inspecting: false,
        });
        return;
      }
      const accounts = await adapter.inspect(path);
      await addSource({
        sourceType,
        path,
        enableVideos: true,
        accounts,
        status: accounts.length ? "ready" : "empty",
        errorMessage: "",
        inspecting: false,
      });
    } catch (err) {
      pushToast({
        tone: "danger",
        title: `添加${adapter.label}目录失败`,
        description: String(err),
      });
    }
  }

  /** 自动发现指定适配器的本机根目录并加入。 */
  async function discoverAndAdd(sourceType: CollectSourceType) {
    const adapter = getCollectAdapter(sourceType);
    if (!adapter?.discover) return;
    detecting.value = true;
    try {
      const accounts = await adapter.discover();
      if (!accounts.length) {
        pushToast({ tone: "warning", title: `未发现${adapter.label}目录` });
        return;
      }
      const path = accounts[0].sourceRoot;
      if (hasPathConflict(path)) {
        const existingIndex = collectSources.value.findIndex(
          (source) => normalizePath(source.path) === normalizePath(path),
        );
        if (existingIndex >= 0) {
          await refreshSource(existingIndex);
          pushToast({ tone: "success", title: `${adapter.label}目录已重新识别` });
        } else {
          pushToast({ tone: "warning", title: "自动发现的目录与现有目录重叠" });
        }
        return;
      }
      await addSource({
        sourceType,
        path,
        enableVideos: true,
        accounts,
        status: accounts.length ? "ready" : "empty",
        errorMessage: "",
        inspecting: false,
      });
    } catch (err) {
      pushToast({
        tone: "danger",
        title: `自动发现${adapter.label}失败`,
        description: String(err),
      });
    } finally {
      detecting.value = false;
    }
  }

  /** 保存采集源；保存失败时恢复列表和账号选择。 */
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
      void persistSelectedAccounts();
    }
    selectionsInitialized = true;
    await persistSourceCache();
  }

  /** 全量检查一个采集源：目录存在性 + 账号识别，并写回快照。 */
  async function refreshSource(index: number) {
    const source = collectSources.value[index];
    if (!source) return;
    const adapter = getCollectAdapter(source.sourceType);
    source.inspecting = true;
    source.status = "checking";
    source.errorMessage = "";
    try {
      if (!(await checkDirectory(source.path))) {
        source.accounts = [];
        source.status = "missing";
        return;
      }
      if (adapter && adapter.kind === "account") {
        source.accounts = await adapter.inspect(source.path);
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
      await persistSourceCache();
    }
  }

  /**
   * 轻量校验：仅确认目录是否仍存在。
   * 有可用快照（ready/empty）时不再重扫账号，避免进页卡顿。
   */
  async function validateSourceLightly(index: number) {
    const source = collectSources.value[index];
    if (!source || source.inspecting) return;

    let exists = false;
    try {
      exists = await checkDirectory(source.path);
    } catch {
      exists = false;
    }

    if (!exists) {
      if (source.status !== "missing" || source.accounts.length) {
        source.accounts = [];
        source.status = "missing";
        source.errorMessage = "";
        await persistSourceCache();
      }
      syncSelectedAccounts();
      return;
    }

    const adapter = getCollectAdapter(source.sourceType);
    if (adapter?.kind !== "account") {
      if (source.status !== "ready") {
        source.status = "ready";
        source.errorMessage = "";
        await persistSourceCache();
      }
      return;
    }

    // 账号型来源：无缓存/失败/刚恢复时才全量识别
    const needsFullInspect =
      source.status === "checking" || source.status === "error" || source.status === "missing";
    if (needsFullInspect) {
      await refreshSource(index);
    }
  }

  /** 为目录不存在的采集源重新选择路径，并在保存失败时恢复原记录。 */
  async function pickAndReplaceSource(index: number) {
    const source = collectSources.value[index];
    if (!source) return;
    const adapter = getCollectAdapter(source.sourceType);
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
        enableVideos: source.enableVideos !== false,
      });
      if (adapter && adapter.kind === "account") {
        replacement.accounts = await adapter.inspect(path);
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
    void persistSelectedAccounts();
    await persistSourceCache();
  }

  /**
   * 用配置 + 探测快照恢复列表并秒开首屏。
   * persistedSelections 为 null 表示从未配置，默认全选。
   */
  async function loadSources(
    sources: CollectSourceDto[],
    cache: CollectSourceCacheDto[] = [],
    persistedSelections: WechatAccountTargetDto[] | null = null,
  ) {
    const cacheByPath = new Map(
      cache.map((entry) => [normalizePath(entry.path), entry] as const),
    );
    collectSources.value = sources.map((source) => {
      const item = createSourceItem(source);
      const cached = cacheByPath.get(normalizePath(source.path));
      if (cached) {
        item.status = cached.status;
        item.errorMessage = cached.errorMessage || "";
        item.accounts = Array.isArray(cached.accounts) ? [...cached.accounts] : [];
      }
      return item;
    });

    selectionsInitialized = false;
    if (persistedSelections === null) {
      selectedAccounts.value = allWechatAccounts.value.map(toAccountTarget);
    } else {
      const available = new Set(allWechatAccounts.value.map(accountKey));
      selectedAccounts.value = persistedSelections.filter((target) =>
        available.has(targetKey(target)),
      );
    }
    selectionsInitialized = true;

    // 后台轻量校验，不阻塞首屏
    void Promise.all(collectSources.value.map((_, index) => validateSourceLightly(index)));
  }

  async function removeSource(index: number) {
    const previousSources = cloneSources(collectSources.value);
    const previousSelections = selectedAccounts.value.map((target) => ({ ...target }));
    collectSources.value = collectSources.value.filter((_, sourceIndex) => sourceIndex !== index);
    syncSelectedAccounts();
    if (await persistSources(previousSources, previousSelections)) {
      void persistSelectedAccounts();
      await persistSourceCache();
    }
  }

  async function persistSources(
    previousSources: CollectSourceItem[],
    previousSelections: WechatAccountTargetDto[],
  ) {
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
    sourceTypeLabel,
    sourceVideoHint,
    supportsVideos,
    toggleAccount,
    toggleSourceVideos,
  };
}

export type { CollectSourceItem };
