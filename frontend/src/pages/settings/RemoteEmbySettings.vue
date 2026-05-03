<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type {
  RemoteEmbyView,
  RemoteEmbySource,
  RemoteEmbySyncOperation,
  RemoteEmbySyncResponse,
  VirtualFolderInfo
} from '../../api/emby';
import { api, isAdmin } from '../../store/app';
import { syncPhaseLabel } from '../../utils/syncPhaseLabel';


/** PB39：常见 Emby 真客户端预设。下拉一键填入「客户端 / 设备名 / 应用版本」三件套，
 * DeviceId 由后端自动派生 32 位 hex（不带项目名前缀），首次创建后**永不变**。
 * 默认 Infuse-Direct on Apple TV 是最不容易被远端管理员识别为"网关"的伪装组合。 */
const SPOOFED_CLIENT_PRESETS = [
  { label: 'Infuse-Direct on Apple TV（推荐）', client: 'Infuse-Direct', device: 'Apple TV', version: '8.2.4' },
  { label: 'Infuse-Direct on iPhone', client: 'Infuse-Direct', device: 'iPhone', version: '8.2.4' },
  { label: 'Emby Web on Chrome / Windows', client: 'Emby Web', device: 'Chrome on Windows', version: '4.7.10.0' },
  { label: 'Emby for iOS', client: 'Emby for iOS', device: 'iPhone', version: '2.0.86' },
  { label: 'Emby for Android', client: 'Emby for Android', device: 'Android', version: '3.5.20' },
  { label: 'Emby Theater on Apple TV', client: 'Emby Theater', device: 'Apple TV', version: '4.6.4' }
];
const DEFAULT_SPOOFED_PRESET = SPOOFED_CLIENT_PRESETS[0];

const loading = ref(true);
const saving = ref(false);
const error = ref('');
const saved = ref('');
const previewingViews = ref(false);
const polling = ref(false);
const sources = ref<RemoteEmbySource[]>([]);
const localLibraries = ref<VirtualFolderInfo[]>([]);
const remoteViews = ref<RemoteEmbyView[]>([]);
const operationBySourceId = ref<Record<string, RemoteEmbySyncOperation>>({});
let pollTimer = 0;
let pollingBusy = false;

const form = ref({
  name: '',
  serverUrl: '',
  username: '',
  password: '',
  remoteViewIds: [] as string[],
  targetLibraryId: '',
  displayMode: 'merge' as 'merge' | 'separate',
  viewLibraryMap: {} as Record<string, string>,
  spoofedUserAgent: '',
  enabled: true,
  strmOutputPath: '',
  syncMetadata: true,
  syncSubtitles: true,
  tokenRefreshIntervalSecs: 3600,
  proxyMode: 'proxy' as 'proxy' | 'redirect' | 'redirect_direct',
  autoSyncIntervalMinutes: 0,
  pageSize: 200,
  requestIntervalMs: 0,
  spoofedClient: DEFAULT_SPOOFED_PRESET.client,
  spoofedDeviceName: DEFAULT_SPOOFED_PRESET.device,
  spoofedAppVersion: DEFAULT_SPOOFED_PRESET.version,
  enableAutoDelete: false
});

const editOpen = ref(false);
const editSaving = ref(false);
const editForm = ref({
  sourceId: '',
  name: '',
  serverUrl: '',
  username: '',
  password: '',
  remoteViewIds: [] as string[],
  targetLibraryId: '',
  displayMode: 'merge' as 'merge' | 'separate',
  viewLibraryMap: {} as Record<string, string>,
  spoofedUserAgent: '',
  enabled: true,
  strmOutputPath: '',
  syncMetadata: true,
  syncSubtitles: true,
  tokenRefreshIntervalSecs: 3600,
  proxyMode: 'proxy' as 'proxy' | 'redirect' | 'redirect_direct',
  autoSyncIntervalMinutes: 0,
  pageSize: 200,
  requestIntervalMs: 0,
  /** PB39：身份伪装四元组。SpoofedDeviceId 在 source 创建后**永不展示编辑**——
   * 它是远端 Devices 表那行的稳定 ID，编辑一次=远端就出现一台"新设备"，反而引人注意。 */
  spoofedClient: DEFAULT_SPOOFED_PRESET.client,
  spoofedDeviceName: DEFAULT_SPOOFED_PRESET.device,
  spoofedAppVersion: DEFAULT_SPOOFED_PRESET.version,
  spoofedDeviceId: '',
  enableAutoDelete: false,
  mergedRemoteViews: [] as RemoteEmbyView[]
});

/** PB39：把表单的伪装预设应用到一组字段（client/device/version），DeviceId 不动。 */
function applySpoofedPreset(target: { spoofedClient: string; spoofedDeviceName: string; spoofedAppVersion: string }, preset: typeof SPOOFED_CLIENT_PRESETS[number]) {
  target.spoofedClient = preset.client;
  target.spoofedDeviceName = preset.device;
  target.spoofedAppVersion = preset.version;
}

const sourceCount = computed(() => sources.value.length);
const enabledCount = computed(() => sources.value.filter((source) => source.Enabled).length);
const lastSyncSuccessCount = computed(
  () => sources.value.filter((source) => source.LastSyncAt && !source.LastSyncError).length
);
const runningSyncCount = computed(
  () =>
    Object.values(operationBySourceId.value).filter(
      (operation) => operation && !operation.Done && (operation.Running || operation.Queued)
    ).length
);
const displayModeItems = [
  { label: '灵活映射（逐个指定目标本地库）', value: 'merge' },
  { label: '自动独立（每个远端库自动建库）', value: 'separate' }
];
/** 本地媒体库下拉项：排除内部虚拟中转库（不应让用户手动选择/删除） */
const TRANSIT_LIB_NAME = '远端 Emby 中转'
const localLibraryItems = computed(() =>
  localLibraries.value
    .filter((folder) => folder.Name !== TRANSIT_LIB_NAME)
    .map((folder) => ({
      label: `${folder.Name} · ${collectionTypeLabel(folder.CollectionType)}`,
      value: folder.ItemId
    }))
);
const remoteViewItems = computed(() =>
  remoteViews.value.map((view) => ({
    label: `${view.Name}${view.CollectionType ? ` · ${collectionTypeLabel(view.CollectionType)}` : ''}`,
    value: view.Id
  }))
);

/** 编辑弹窗下拉：预览列表 ∪ 条目自带 RemoteViews ∪ 占位 Id */
const editRemoteViewItems = computed(() => {
  const map = new Map<string, RemoteEmbyView>();
  for (const v of remoteViews.value) map.set(v.Id.toLowerCase(), v);
  for (const v of editForm.value.mergedRemoteViews) map.set(v.Id.toLowerCase(), v);
  return [...map.values()].map((view) => ({
    label: `${view.Name}${view.CollectionType ? ` · ${collectionTypeLabel(view.CollectionType)}` : ''}`,
    value: view.Id
  }));
});
const localLibraryNameMap = computed(() => {
  const map = new Map<string, string>();
  for (const folder of localLibraries.value) {
    map.set(folder.ItemId.toLowerCase(), folder.Name);
  }
  return map;
});
const remoteViewNameMap = computed(() => {
  const map = new Map<string, string>();
  for (const view of remoteViews.value) {
    map.set(view.Id.toLowerCase(), view.Name);
  }
  return map;
});
const editRemoteViewNameMap = computed(() => {
  const map = new Map<string, string>();
  for (const v of editForm.value.mergedRemoteViews) {
    map.set(v.Id.toLowerCase(), v.Name);
  }
  for (const v of remoteViews.value) {
    map.set(v.Id.toLowerCase(), v.Name);
  }
  return map;
});
function remoteViewCollectionType(vid: string): string | undefined {
  return remoteViews.value.find((v) => v.Id.toLowerCase() === vid.toLowerCase())?.CollectionType;
}
function editRemoteViewCollectionType(vid: string): string | undefined {
  const all = [...editForm.value.mergedRemoteViews, ...remoteViews.value];
  return all.find((v) => v.Id.toLowerCase() === vid.toLowerCase())?.CollectionType;
}

function collectionTypeLabel(type?: string) {
  const normalized = (type || '').toLowerCase();
  if (normalized === 'tvshows') return '电视剧';
  if (normalized === 'music') return '音乐';
  if (normalized === 'musicvideos') return '音乐视频';
  if (normalized === 'photos') return '照片';
  if (normalized === 'homevideos') return '家庭视频';
  if (normalized === 'mixed') return '混合';
  return '电影';
}

function displayModeLabel(mode?: string) {
  return mode === 'merge' ? '灵活映射' : '自动独立';
}

function targetLibraryName(libraryId?: string) {
  if (!libraryId) return '-';
  const name = localLibraryNameMap.value.get(libraryId.toLowerCase());
  if (name === TRANSIT_LIB_NAME || name == null) {
    // 中转库或未在本地库列表中：用 ID 缩写显示
    return name === TRANSIT_LIB_NAME ? '（自动中转库）' : libraryId;
  }
  return name;
}

function sourceRemoteViewsText(source: RemoteEmbySource) {
  if (source.RemoteViews?.length) {
    return source.RemoteViews.map((view) => view.Name || view.Id).join(' · ');
  }
  if (!source.RemoteViewIds?.length) return '全部远端媒体库';
  return source.RemoteViewIds.map((viewId) => remoteViewNameMap.value.get(viewId.toLowerCase()) || viewId).join(' · ');
}

const activeOperationDetail = computed(() => {
  const running = Object.values(operationBySourceId.value).find((operation) => !operation.Done);
  if (!running) return '';
  const runtime = operationRuntimeSeconds(running);
  const phase = syncPhaseLabel(running.Phase, { remotePrefix: false }) || running.Status;
  const seriesInfo = running.CurrentSeries ? ` · ${running.CurrentSeries}` : '';
  return `${running.SourceName} · ${phase} · ${Math.round(running.Progress || 0)}%${seriesInfo} · ${runtime} 秒`;
});

function formatDate(value?: string) {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '-';
  return date.toLocaleString();
}

function sourceOperation(source: RemoteEmbySource) {
  return operationBySourceId.value[source.Id];
}

function sourceStatus(source: RemoteEmbySource) {
  const operation = sourceOperation(source);
  if (operation && !operation.Done) {
    const progress = Number.isFinite(operation.Progress) ? Math.round(operation.Progress) : 0;
    const phase = syncPhaseLabel(operation.Phase, { remotePrefix: false }) || operation.Status;
    return `${phase} (${progress}%)`;
  }
  if (operation?.Done && operation.Status === 'Failed') return '任务失败';
  if (operation?.Done && operation.Status === 'Cancelled') return '已中断';
  if (!source.Enabled) return '已禁用';
  if (source.LastSyncError) return '上次失败';
  if (source.LastSyncAt) return '最近成功';
  return '未同步';
}

function sourceStatusColor(source: RemoteEmbySource) {
  const operation = sourceOperation(source);
  if (operation && !operation.Done) return 'warning';
  if (operation?.Done && operation.Status === 'Failed') return 'error';
  if (operation?.Done && operation.Status === 'Cancelled') return 'warning';
  if (!source.Enabled) return 'neutral';
  if (source.LastSyncError) return 'error';
  if (source.LastSyncAt) return 'success';
  return 'neutral';
}

function operationRuntimeSeconds(operation: RemoteEmbySyncOperation) {
  if (!operation.StartedAt) return 0;
  const startedAt = new Date(operation.StartedAt).getTime();
  if (!Number.isFinite(startedAt)) return 0;
  const endAt = operation.CompletedAt ? new Date(operation.CompletedAt).getTime() : Date.now();
  if (!Number.isFinite(endAt)) return 0;
  return Math.max(0, Math.floor((endAt - startedAt) / 1000));
}

function sourceProgressText(source: RemoteEmbySource) {
  const operation = sourceOperation(source);
  if (!operation) return '';
  const progress = Number.isFinite(operation.Progress) ? Math.round(operation.Progress) : 0;
  const runtime = operationRuntimeSeconds(operation);
  if (!operation.Done) {
    const phase = syncPhaseLabel(operation.Phase, { remotePrefix: false }) || operation.Status;
    const seriesInfo = operation.CurrentSeries
      ? ` · ${operation.CurrentSeries}`
      : '';
    const skippedInfo = (operation.SkippedUnchangedSeries || 0) > 0
      ? ` · 跳过 ${operation.SkippedUnchangedSeries} 部未变化剧集`
      : '';
    return `阶段 ${phase} · ${progress}%${seriesInfo}${skippedInfo} · 已运行 ${runtime} 秒`;
  }
  if (operation.Status === 'Succeeded') {
    const writtenFiles = operation.Result?.WrittenFiles ?? operation.WrittenFiles ?? 0;
    return `最近任务完成 · 入库 ${writtenFiles} 个条目`;
  }
  if (operation.Status === 'Failed') {
    return `最近任务失败 · 已运行 ${runtime} 秒 · 重试将跳过已入库条目`;
  }
  if (operation.Status === 'Cancelled') {
    const writtenFiles = operation.Result?.WrittenFiles ?? operation.WrittenFiles ?? 0;
    return `任务已中断 · 已入库 ${writtenFiles} 个条目 · 运行 ${runtime} 秒`;
  }
  return '';
}

function isSourceSyncing(source: RemoteEmbySource) {
  const operation = sourceOperation(source);
  return Boolean(operation && !operation.Done);
}

function canSyncSource(source: RemoteEmbySource) {
  return source.Enabled && !isSourceSyncing(source);
}

/** PB49：识别「上次同步失败 / 中断」的状态，用于把按钮文案从「立即同步」切到「重试同步」。
 *
 * 重试时后端会自动跳过已入库的条目（local_synced_ids fast path），并利用
 * view 级 etag 缓存跳过未变化的远端库，因此重试不会重新下载已完成的内容。 */
function isSourceFailedLastRun(source: RemoteEmbySource) {
  const operation = sourceOperation(source);
  if (operation?.Done && (operation.Status === 'Failed' || operation.Status === 'Cancelled')) {
    return true;
  }
  return Boolean(source.LastSyncError && !source.LastSyncAt);
}

function syncButtonLabel(source: RemoteEmbySource) {
  if (isSourceSyncing(source)) return '同步中';
  if (isSourceFailedLastRun(source)) return '重试同步';
  return '立即同步';
}

function buildOperationMap(operations: RemoteEmbySyncOperation[]) {
  const next: Record<string, RemoteEmbySyncOperation> = {};
  const sorted = [...operations].sort((a, b) => {
    if (a.Done !== b.Done) return a.Done ? 1 : -1;
    const bTime = new Date(b.CreatedAt || 0).getTime();
    const aTime = new Date(a.CreatedAt || 0).getTime();
    return (Number.isFinite(bTime) ? bTime : 0) - (Number.isFinite(aTime) ? aTime : 0);
  });
  for (const operation of sorted) {
    if (!operation?.SourceId) continue;
    if (!next[operation.SourceId]) {
      next[operation.SourceId] = operation;
    }
  }
  return next;
}

function startPolling() {
  if (pollTimer) return;
  pollTimer = window.setInterval(() => {
    void pollOperations();
  }, 2000);
  polling.value = true;
}

function stopPolling() {
  if (pollTimer) {
    window.clearInterval(pollTimer);
    pollTimer = 0;
  }
  polling.value = false;
}

async function refreshSourcesOnly() {
  const [sourceList, folders] = await Promise.all([api.remoteEmbySources(), api.virtualFolders()]);
  sources.value = sourceList;
  localLibraries.value = folders;
}

async function pollOperations() {
  if (pollingBusy) return;
  const pending = Object.values(operationBySourceId.value).filter((operation) => !operation.Done);
  if (!pending.length) {
    stopPolling();
    return;
  }

  pollingBusy = true;
  try {
    // PB49 (FX4)：用 allSettled 替代 all——容器重启后旧 operationId 会 404，
    // 旧实现里一个 404 会让整批 polling 失败、UI 永远停在「正在同步」。
    // 现在按单条结果处理：fulfilled 走原逻辑；rejected 且是 404/410 直接把
    // 那个 SourceId 从 operations 表里拿走（让按钮回到"立即同步/重试同步"），
    // 其它错误（5xx / 网络断）只在 console 提示一次，不打 toast。
    const settled = await Promise.allSettled(
      pending.map((operation) =>
        api
          .remoteEmbySyncOperation(operation.Id)
          .then((result) => ({ ok: true as const, sourceId: operation.SourceId, result }))
          .catch((err) => ({
            ok: false as const,
            sourceId: operation.SourceId,
            operationId: operation.Id,
            err: err as Error & { status?: number }
          }))
      )
    );

    const next = { ...operationBySourceId.value };
    let shouldRefreshSources = false;
    let allDoneOrGone = true;
    let nonRecoverableMessage = '';

    for (const settledResult of settled) {
      if (settledResult.status !== 'fulfilled') {
        // Promise.allSettled 包了一层；外层的 .catch 已经把 reject 转成 ok:false 对象了，
        // 这里几乎不会走到。保险起见忽略。
        allDoneOrGone = false;
        continue;
      }
      const value = settledResult.value;
      if (value.ok) {
        next[value.sourceId] = value.result;
        if (value.result.Done) {
          shouldRefreshSources = true;
        } else {
          allDoneOrGone = false;
        }
      } else {
        const status = value.err.status ?? 0;
        if (status === 404 || status === 410) {
          // 后端找不到这个 op：进程重启 / TTL 过期 / 被清理。从内存里拿走，
          // 让该源回到 sources 列表里以 last_sync_error / last_sync_at 决定按钮。
          delete next[value.sourceId];
          shouldRefreshSources = true;
          // eslint-disable-next-line no-console
          console.info(
            `[RemoteEmbySync] operation ${value.operationId} 404，已从前端状态移除（容器可能重启过）`
          );
        } else {
          // 5xx / 网络问题——保留旧记录，下一轮重试，仅记录最近一条非致命错误用于诊断
          allDoneOrGone = false;
          nonRecoverableMessage = value.err.message || `HTTP ${status}`;
        }
      }
    }

    operationBySourceId.value = next;
    if (shouldRefreshSources) {
      await refreshSourcesOnly();
    }
    if (allDoneOrGone || Object.values(next).every((op) => op.Done)) {
      stopPolling();
    }
    if (nonRecoverableMessage) {
      // eslint-disable-next-line no-console
      console.warn('[RemoteEmbySync] 轮询出现非 404 错误：', nonRecoverableMessage);
    }
  } catch (err) {
    // 兜底：极少数 throw 不在 inner catch 里时（如 JSON parse），仍然显示给用户
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    pollingBusy = false;
  }
}

async function hydrateOperations() {
  const operations = await api.remoteEmbySyncOperations(50);
  operationBySourceId.value = buildOperationMap(operations);
  if (Object.values(operationBySourceId.value).some((operation) => !operation.Done)) {
    startPolling();
  } else {
    stopPolling();
  }
}

async function load() {
  if (!isAdmin.value) return;
  loading.value = true;
  error.value = '';
  try {
    const [sourceList, folders] = await Promise.all([api.remoteEmbySources(), api.virtualFolders()]);
    sources.value = sourceList;
    localLibraries.value = folders;
    if (!form.value.targetLibraryId && folders.length) {
      form.value.targetLibraryId = folders[0].ItemId;
    }
    await hydrateOperations();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

function openEditor(source: RemoteEmbySource) {
  error.value = '';
  saved.value = '';
  const merged = new Map<string, RemoteEmbyView>();
  for (const v of remoteViews.value) merged.set(v.Id.toLowerCase(), v);
  for (const v of source.RemoteViews || []) merged.set(v.Id.toLowerCase(), v);
  for (const vid of source.RemoteViewIds || []) {
    const k = vid.toLowerCase();
    if (!merged.has(k)) merged.set(k, { Id: vid, Name: vid });
  }
  editForm.value = {
    sourceId: source.Id,
    name: source.Name,
    serverUrl: source.ServerUrl,
    username: source.Username,
    password: '',
    remoteViewIds: [...(source.RemoteViewIds || [])],
    targetLibraryId: source.TargetLibraryId,
    displayMode: source.DisplayMode === 'merge' ? 'merge' : 'separate',
    viewLibraryMap: { ...(source.ViewLibraryMap || {}) },
    spoofedUserAgent: source.SpoofedUserAgent || '',
    enabled: source.Enabled,
    strmOutputPath: source.StrmOutputPath || '',
    syncMetadata: source.SyncMetadata !== false,
    syncSubtitles: source.SyncSubtitles !== false,
    tokenRefreshIntervalSecs: source.TokenRefreshIntervalSecs ?? 3600,
    proxyMode: (['redirect', 'redirect_direct'].includes(source.ProxyMode) ? source.ProxyMode : 'proxy') as 'proxy' | 'redirect' | 'redirect_direct',
    autoSyncIntervalMinutes: Math.max(0, Number(source.AutoSyncIntervalMinutes ?? 0) || 0),
    pageSize: Math.max(50, Math.min(1000, Number(source.PageSize ?? 200) || 200)),
    requestIntervalMs: Math.max(0, Math.min(60000, Number(source.RequestIntervalMs ?? 0) || 0)),
    spoofedClient: source.SpoofedClient || DEFAULT_SPOOFED_PRESET.client,
    spoofedDeviceName: source.SpoofedDeviceName || DEFAULT_SPOOFED_PRESET.device,
    spoofedAppVersion: source.SpoofedAppVersion || DEFAULT_SPOOFED_PRESET.version,
    spoofedDeviceId: source.SpoofedDeviceId || '',
    enableAutoDelete: source.EnableAutoDelete === true,
    mergedRemoteViews: [...merged.values()]
  };
  editOpen.value = true;
}

async function saveEditor() {
  const p = editForm.value;
  if (!p.sourceId) return;
  if (!p.name.trim()) {
    error.value = '请输入远端源名称';
    return;
  }
  if (!p.serverUrl.trim()) {
    error.value = '请输入远端 Emby 地址';
    return;
  }
  if (!p.username.trim()) {
    error.value = '请输入远端用户名';
    return;
  }
  if (!p.remoteViewIds.length) {
    error.value = '请至少选择一个远端媒体库';
    return;
  }
  if (p.displayMode === 'merge') {
    const hasAnyMapping = p.remoteViewIds.some((vid) => p.viewLibraryMap[vid]);
    if (!hasAnyMapping && !p.targetLibraryId) {
      error.value = '灵活映射模式下，请为至少一个远端库指定目标本地库，或设置默认目标库';
      return;
    }
  }
  if (!p.strmOutputPath.trim()) {
    error.value = '请填写 STRM 输出根目录（必填，远端 strm/元数据/字幕都将写入此目录）';
    return;
  }
  editSaving.value = true;
  error.value = '';
  saved.value = '';
  try {
    const byId = new Map(p.mergedRemoteViews.map((v) => [v.Id.toLowerCase(), v]));
    const selectedRemoteViews = p.remoteViewIds
      .map((id) => byId.get(id.toLowerCase()) || ({ Id: id, Name: id } as RemoteEmbyView));

    const payload: {
      Name: string;
      ServerUrl: string;
      Username: string;
      Password?: string;
      TargetLibraryId: string;
      DisplayMode: 'merge' | 'separate';
      RemoteViewIds: string[];
      RemoteViews: RemoteEmbyView[];
      ViewLibraryMap: Record<string, string>;
      SpoofedUserAgent: string;
      Enabled: boolean;
      StrmOutputPath: string;
      SyncMetadata: boolean;
      SyncSubtitles: boolean;
      TokenRefreshIntervalSecs: number;
      ProxyMode: 'proxy' | 'redirect' | 'redirect_direct';
      AutoSyncIntervalMinutes: number;
      PageSize: number;
      RequestIntervalMs: number;
      SpoofedClient: string;
      SpoofedDeviceName: string;
      SpoofedAppVersion: string;
      EnableAutoDelete: boolean;
    } = {
      Name: p.name.trim(),
      ServerUrl: p.serverUrl.trim(),
      Username: p.username.trim(),
      TargetLibraryId: p.targetLibraryId,
      DisplayMode: p.displayMode,
      RemoteViewIds: p.remoteViewIds,
      RemoteViews: selectedRemoteViews,
      ViewLibraryMap: p.viewLibraryMap,
      SpoofedUserAgent: p.spoofedUserAgent.trim(),
      Enabled: p.enabled,
      StrmOutputPath: p.strmOutputPath.trim(),
      SyncMetadata: p.syncMetadata,
      SyncSubtitles: p.syncSubtitles,
      TokenRefreshIntervalSecs: Math.min(
        Math.max(Number(p.tokenRefreshIntervalSecs) || 3600, 300),
        86400 * 30
      ),
      ProxyMode: p.proxyMode,
      AutoSyncIntervalMinutes: Math.max(0, Math.min(60 * 24 * 7, Number(p.autoSyncIntervalMinutes) || 0)),
      PageSize: Math.max(50, Math.min(1000, Number(p.pageSize) || 200)),
      RequestIntervalMs: Math.max(0, Math.min(60000, Number(p.requestIntervalMs) || 0)),
      SpoofedClient: p.spoofedClient.trim(),
      SpoofedDeviceName: p.spoofedDeviceName.trim(),
      SpoofedAppVersion: p.spoofedAppVersion.trim(),
      EnableAutoDelete: p.enableAutoDelete
    };
    if (p.password.trim()) {
      payload.Password = p.password;
    }

    await api.updateRemoteEmbySource(p.sourceId, payload);
    editOpen.value = false;
    saved.value = `已保存远端源：${p.name.trim()}`;
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    editSaving.value = false;
  }
}

async function createSource() {
  const payload = form.value;
  if (!payload.name.trim()) {
    error.value = '请输入远端源名称';
    return;
  }
  if (!payload.serverUrl.trim()) {
    error.value = '请输入远端 Emby 地址';
    return;
  }
  if (!payload.username.trim()) {
    error.value = '请输入远端 Emby 用户名';
    return;
  }
  if (!payload.password.trim()) {
    error.value = '请输入远端 Emby 密码';
    return;
  }
  if (!payload.remoteViewIds.length) {
    error.value = '请选择至少一个远端媒体库';
    return;
  }
  if (payload.displayMode === 'merge') {
    const hasAnyMapping = payload.remoteViewIds.some((vid) => payload.viewLibraryMap[vid]);
    if (!hasAnyMapping && !payload.targetLibraryId) {
      error.value = '灵活映射模式下，请为至少一个远端库指定目标本地库，或设置默认目标库';
      return;
    }
  }
  if (!payload.strmOutputPath.trim()) {
    error.value = '请填写 STRM 输出根目录（必填，远端 strm/元数据/字幕都将写入此目录）';
    return;
  }

  const selectedLocalLibraryId =
    payload.targetLibraryId || localLibraries.value[0]?.ItemId || '';

  saving.value = true;
  error.value = '';
  saved.value = '';
  try {
    const selectedRemoteViews = remoteViews.value.filter((view) =>
      payload.remoteViewIds.some((selectedId) => selectedId.toLowerCase() === view.Id.toLowerCase())
    );
    await api.createRemoteEmbySource({
      Name: payload.name.trim(),
      ServerUrl: payload.serverUrl.trim(),
      Username: payload.username.trim(),
      Password: payload.password,
      TargetLibraryId: selectedLocalLibraryId,
      DisplayMode: payload.displayMode,
      RemoteViewIds: payload.remoteViewIds,
      RemoteViews: selectedRemoteViews,
      ViewLibraryMap: payload.viewLibraryMap,
      SpoofedUserAgent: payload.spoofedUserAgent.trim(),
      Enabled: payload.enabled,
      StrmOutputPath: payload.strmOutputPath.trim(),
      SyncMetadata: payload.syncMetadata,
      SyncSubtitles: payload.syncSubtitles,
      TokenRefreshIntervalSecs: Math.min(
        Math.max(Number(payload.tokenRefreshIntervalSecs) || 3600, 300),
        86400 * 30
      ),
      ProxyMode: payload.proxyMode,
      AutoSyncIntervalMinutes: Math.max(0, Math.min(60 * 24 * 7, Number(payload.autoSyncIntervalMinutes) || 0)),
      PageSize: Math.max(50, Math.min(1000, Number(payload.pageSize) || 200)),
      RequestIntervalMs: Math.max(0, Math.min(60000, Number(payload.requestIntervalMs) || 0)),
      SpoofedClient: payload.spoofedClient.trim(),
      SpoofedDeviceName: payload.spoofedDeviceName.trim(),
      SpoofedAppVersion: payload.spoofedAppVersion.trim(),
      EnableAutoDelete: payload.enableAutoDelete
    });
    saved.value = `已创建远端源：${payload.name.trim()}`;
    form.value.name = '';
    form.value.serverUrl = '';
    form.value.username = '';
    form.value.password = '';
    form.value.remoteViewIds = [];
    form.value.viewLibraryMap = {};
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    saving.value = false;
  }
}

async function previewRemoteViews() {
  const payload = form.value;
  if (!payload.serverUrl.trim()) {
    error.value = '请先输入远端地址';
    return;
  }
  if (!payload.username.trim()) {
    error.value = '请先输入远端 Emby 用户名';
    return;
  }
  if (!payload.password.trim()) {
    error.value = '请先输入远端 Emby 密码';
    return;
  }
  previewingViews.value = true;
  error.value = '';
  saved.value = '';
  try {
    const result = await api.previewRemoteEmbyViews({
      ServerUrl: payload.serverUrl.trim(),
      Username: payload.username.trim(),
      Password: payload.password,
      SpoofedClient: payload.spoofedClient?.trim() || undefined,
      SpoofedDeviceName: payload.spoofedDeviceName?.trim() || undefined,
      SpoofedAppVersion: payload.spoofedAppVersion?.trim() || undefined,
    });
    const views = result.Views;
    remoteViews.value = views;

    // 若源名称尚未填写，则自动使用远端服务器名称填充
    if (!form.value.name.trim() && result.ServerName.trim()) {
      form.value.name = result.ServerName.trim();
    }

    if (!views.length) {
      form.value.remoteViewIds = [];
      saved.value = `已连接「${result.ServerName}」，但未发现可同步媒体库`;
      return;
    }
    const existed = new Set(views.map((view) => view.Id.toLowerCase()));
    form.value.remoteViewIds = form.value.remoteViewIds.filter((id) =>
      existed.has(id.toLowerCase())
    );
    if (!form.value.remoteViewIds.length) {
      form.value.remoteViewIds = [views[0].Id];
    }
    saved.value = `已连接「${result.ServerName}」，获取到 ${views.length} 个媒体库`;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    remoteViews.value = [];
    form.value.remoteViewIds = [];
  } finally {
    previewingViews.value = false;
  }
}

async function removeSource(source: RemoteEmbySource) {
  if (!window.confirm(`确认删除远端源「${source.Name}」？`)) {
    return;
  }
  error.value = '';
  saved.value = '';
  try {
    await api.deleteRemoteEmbySource(source.Id);
    delete operationBySourceId.value[source.Id];
    saved.value = `已删除远端源：${source.Name}`;
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

function syncSummaryText(summary: RemoteEmbySyncResponse['ScanSummary']) {
  return `扫描库 ${summary.Libraries} / 文件 ${summary.ScannedFiles} / 入库 ${summary.ImportedItems}`;
}

async function syncSource(source: RemoteEmbySource) {
  if (!source.Enabled) {
    error.value = '该远端源已禁用，请先启用后再同步';
    return;
  }
  const operation = sourceOperation(source);
  if (operation && !operation.Done) {
    saved.value = `同步任务已在运行：${source.Name}`;
    startPolling();
    return;
  }

  error.value = '';
  saved.value = '';
  try {
    if (operation?.Done) {
      const next = { ...operationBySourceId.value };
      delete next[source.Id];
      operationBySourceId.value = next;
    }
    const queued = await api.startRemoteEmbySync(source.Id);
    operationBySourceId.value = {
      ...operationBySourceId.value,
      [source.Id]: queued.Operation
    };
    saved.value = queued.Message || `已提交同步任务：${source.Name}`;
    startPolling();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

async function cancelSync(source: RemoteEmbySource) {
  const operation = sourceOperation(source);
  if (!operation || operation.Done) return;
  error.value = '';
  try {
    const updated = await api.cancelRemoteEmbySync(operation.Id);
    operationBySourceId.value[source.Id] = updated;
    saved.value = `已请求中断同步：${source.Name}`;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

onMounted(load);
onBeforeUnmount(() => {
  stopPolling();
});
</script>

<template>
  <SettingsLayout>
    <div
      v-if="!isAdmin"
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
    >
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能管理远端 Emby 中转源。</p>
    </div>

    <div v-else class="space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">Remote Emby Transit</p>
          <h2 class="text-highlighted text-xl font-semibold">远端 Emby 中转源</h2>
        </div>
        <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-cw" :loading="loading" @click="load">
          刷新
        </UButton>
      </div>

      <div class="grid gap-3 sm:grid-cols-4">
        <UCard variant="soft">
          <p class="text-muted text-xs">源总数</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ sourceCount }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">启用中</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ enabledCount }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">最近成功</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ lastSyncSuccessCount }}</p>
          <p v-if="activeOperationDetail" class="text-muted text-xs">{{ activeOperationDetail }}</p>
          <p v-else-if="polling" class="text-muted text-xs">任务轮询中…</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">运行中任务</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ runningSyncCount }}</p>
          <p class="text-muted text-xs">自动轮询 operation 进度</p>
        </UCard>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">新增远端 Emby 源</h3>
        </template>
        <div class="grid gap-3 lg:grid-cols-2">
          <UFormField label="源名称">
            <UInput v-model="form.name" placeholder="例如：家庭 Emby / 朋友服 A" class="w-full" />
          </UFormField>
          <UFormField label="远端地址">
            <UInput
              v-model="form.serverUrl"
              placeholder="例如：https://remote.emby.example:8096"
              class="w-full"
            />
          </UFormField>
          <UFormField label="用户名">
            <UInput v-model="form.username" placeholder="远端 Emby 账号名" class="w-full" />
          </UFormField>
          <UFormField label="密码">
            <UInput v-model="form.password" type="password" placeholder="远端 Emby 密码" class="w-full" />
          </UFormField>
          <UFormField label="远端媒体库">
            <USelect
              v-model="form.remoteViewIds"
              :items="remoteViewItems"
              multiple
              value-key="value"
              class="w-full"
              placeholder="先点击“获取远端媒体库列表”"
            />
          </UFormField>
          <UFormField label="显示方式（本地）">
            <USelect v-model="form.displayMode" :items="displayModeItems" value-key="value" class="w-full" />
          </UFormField>
          <div class="lg:col-span-2 flex items-center justify-end">
            <UButton
              color="neutral"
              variant="soft"
              icon="i-lucide-list-restart"
              :loading="previewingViews"
              @click="previewRemoteViews"
            >
              获取远端媒体库列表
            </UButton>
          </div>
          <template v-if="form.displayMode === 'merge' && form.remoteViewIds.length">
            <UFormField label="默认目标本地库（可选）">
              <USelect
                v-model="form.targetLibraryId"
                :items="[{ label: '— 无 —', value: '' }, ...localLibraryItems]"
                value-key="value"
                class="w-full"
                placeholder="未单独指定的远端库将使用此库"
              />
            </UFormField>
            <div class="lg:col-span-2">
              <p class="text-muted mb-2 text-xs font-medium">为每个远端库指定目标本地库：</p>
              <div class="space-y-2">
                <div
                  v-for="vid in form.remoteViewIds"
                  :key="vid"
                  class="flex items-center gap-2 rounded-lg border border-default px-3 py-2"
                >
                  <span class="min-w-0 flex-1 truncate text-sm font-medium">
                    {{ remoteViewNameMap.get(vid.toLowerCase()) || vid }}
                    <span class="text-muted text-xs ml-1">{{ collectionTypeLabel(remoteViewCollectionType(vid)) }}</span>
                  </span>
                  <USelect
                    :model-value="form.viewLibraryMap[vid] || ''"
                    @update:model-value="(v: string) => { if (v) form.viewLibraryMap[vid] = v; else delete form.viewLibraryMap[vid]; }"
                    :items="[{ label: '使用默认', value: '' }, ...localLibraryItems]"
                    value-key="value"
                    class="w-48"
                    size="sm"
                  />
                </div>
              </div>
            </div>
          </template>
          <UFormField v-else-if="form.displayMode === 'separate'" label="自动独立说明" class="lg:col-span-2">
            <p class="text-muted text-xs leading-relaxed">
              自动独立模式：同步时将为每个远端媒体库自动创建对应的本地独立媒体库，名称与远端媒体库相同。<br />
              无需手动指定目标库，首次同步后可在「媒体库管理」中查看自动创建的媒体库。
            </p>
          </UFormField>
          <UFormField label="启用状态">
            <USwitch v-model="form.enabled" />
          </UFormField>
          <UFormField class="lg:col-span-2" label="身份伪装预设（远端 Devices 表显示的客户端）">
            <USelect
              :model-value="form.spoofedClient + ' | ' + form.spoofedDeviceName + ' | ' + form.spoofedAppVersion"
              @update:model-value="(value: string) => {
                const preset = SPOOFED_CLIENT_PRESETS.find((p) => `${p.client} | ${p.device} | ${p.version}` === value);
                if (preset) applySpoofedPreset(form, preset);
              }"
              :items="SPOOFED_CLIENT_PRESETS.map((p) => ({ label: p.label, value: `${p.client} | ${p.device} | ${p.version}` }))"
              value-key="value"
              class="w-full max-w-md"
            />
            <p class="text-muted mt-1 text-xs">
              选中预设后 <strong>客户端 / 设备 / 版本</strong> 三件套自动填入下方表单。<br />
              远端 Emby 在 <code>Devices</code> 表里只看见<strong>这一台</strong>「{{ form.spoofedClient }} on {{ form.spoofedDeviceName }} v{{ form.spoofedAppVersion }}」，
              不再带 <code>MovieRustTransit / movie-rust-{...}</code> 自爆字符串。
            </p>
          </UFormField>
          <UFormField label="伪装 Client（应用名）">
            <UInput v-model="form.spoofedClient" class="w-full" placeholder="Infuse-Direct" />
          </UFormField>
          <UFormField label="伪装 Device（设备名）">
            <UInput v-model="form.spoofedDeviceName" class="w-full" placeholder="Apple TV" />
          </UFormField>
          <UFormField label="伪装 App Version（版本号）" class="lg:col-span-2">
            <UInput v-model="form.spoofedAppVersion" class="w-full max-w-xs" placeholder="8.2.4" />
            <p class="text-muted mt-1 text-xs">
              <strong>DeviceId 不在此处编辑</strong>：source 创建时由后端自动派生 32 位 hex（不带项目名前缀），
              此后**永不改变**——同一个 source 在远端 Devices 表里永远是同一行设备，符合真人客户端长期使用画像。
            </p>
          </UFormField>
          <UFormField
            class="lg:col-span-2"
            label="STRM 输出根目录"
            required
            :error="!form.strmOutputPath.trim() ? '必填，远端 strm/元数据/字幕都将写入此目录' : undefined"
          >
            <UInput
              v-model="form.strmOutputPath"
              class="w-full"
              placeholder="例如 D:\Media\remote-strm"
              required
            />
            <p class="text-muted mt-1 text-xs">
              <strong>必填项：</strong>同步时将按以下层级结构写入 .strm / NFO / 图片 / 字幕：<br />
              <code class="bg-muted px-1 rounded text-xs font-mono">{根目录}/{源名称}/{远端媒体库名称}/{影片}.strm</code><br />
              "源名称"获取远端媒体库列表后会自动填充为对方服务器名，可手动修改。STRM 文件指向本地代理转发流量，无需直连远端服务器。
            </p>
          </UFormField>
          <UFormField label="同步元数据到侧车（NFO/图）">
            <USwitch v-model="form.syncMetadata" />
          </UFormField>
          <UFormField label="下载外挂字幕">
            <USwitch v-model="form.syncSubtitles" />
          </UFormField>
          <UFormField class="lg:col-span-2" label="流量模式">
            <div class="flex flex-col gap-2">
              <div class="flex items-start gap-3">
                <label class="flex cursor-pointer items-center gap-2">
                  <input type="radio" v-model="form.proxyMode" value="proxy" class="accent-primary" />
                  <div>
                    <span class="font-medium text-sm">本地中转（推荐）</span>
                    <p class="text-muted text-xs">所有媒体流通过本地服务器转发，客户端无需访问远端服务器</p>
                  </div>
                </label>
              </div>
              <div class="flex items-start gap-3">
                <label class="flex cursor-pointer items-center gap-2">
                  <input type="radio" v-model="form.proxyMode" value="redirect" class="accent-primary" />
                  <div>
                    <span class="font-medium text-sm">302 重定向</span>
                    <p class="text-muted text-xs">302 重定向到远端 Emby，客户端自行跟随 302 链获取最终直链。适合有 IP 绑定的 CDN（如115网盘）</p>
                  </div>
                </label>
              </div>
              <div class="flex items-start gap-3">
                <label class="flex cursor-pointer items-center gap-2">
                  <input type="radio" v-model="form.proxyMode" value="redirect_direct" class="accent-primary" />
                  <div>
                    <span class="font-medium text-sm">302 解析直链</span>
                    <p class="text-muted text-xs">服务端解析远端 302 链后返回最终 CDN 直链，减少一跳更快。不适合有 IP 绑定的 CDN</p>
                  </div>
                </label>
              </div>
            </div>
          </UFormField>
          <UFormField class="lg:col-span-2" label="远端令牌刷新间隔（秒）">
            <UInput v-model.number="form.tokenRefreshIntervalSecs" type="number" class="w-full max-w-xs" :min="300" />
            <p class="text-muted mt-1 text-xs">
              范围 300–2592000。STRM 文件指向本地代理（无需嵌入 api_key），此间隔控制后台任务刷新本地服务器缓存的远端访问令牌，确保代理能持续鉴权。
            </p>
          </UFormField>
          <UFormField class="lg:col-span-2" label="自动增量同步间隔（分钟）">
            <UInput
              v-model.number="form.autoSyncIntervalMinutes"
              type="number"
              class="w-full max-w-xs"
              :min="0"
              :max="60 * 24 * 7"
              placeholder="0 = 关闭，30 表示每 30 分钟一次"
            />
            <p class="text-muted mt-1 text-xs">
              0 = 关闭。后台每分钟检查一次该源距离上次同步的时间，达到该间隔即触发增量同步（增 / 改）。范围 1–10080（最长 7 天）。
            </p>
          </UFormField>
          <UFormField class="lg:col-span-2" label="自动删除远端已下架条目">
            <USwitch v-model="form.enableAutoDelete" />
            <p class="text-muted mt-1 text-xs">
              开启后，同步时会对比远端与本地的条目列表，自动删除远端已不存在的 Series / Movie 及其 STRM 文件。关闭时仅同步新增和变更，不执行任何删除操作。
            </p>
          </UFormField>
          <UFormField class="lg:col-span-2" label="拉取速率：单页条目数（PageSize）">
            <UInput
              v-model.number="form.pageSize"
              type="number"
              class="w-full max-w-xs"
              :min="50"
              :max="1000"
              placeholder="默认 200"
            />
            <p class="text-muted mt-1 text-xs">
              范围 50–1000，默认 200。影响 Series / Movie 列表的分页大小（Seasons 和 Episodes 由 Emby 一次返回，不受此值限制）。越大越省请求数但单次响应体越大；远端带宽紧张可调小。
            </p>
          </UFormField>
          <UFormField class="lg:col-span-2" label="拉取速率：请求最小间隔（毫秒）">
            <UInput
              v-model.number="form.requestIntervalMs"
              type="number"
              class="w-full max-w-xs"
              :min="0"
              :max="60000"
              placeholder="0 = 不限速"
            />
            <p class="text-muted mt-1 text-xs">
              范围 0–60000，默认 0（不限速）。所有远端 API 请求（Views / Series / Seasons / Episodes / Movies）均受此间隔约束；峰值 QPS ≈ 1000 / 该值（如 200 ms ≈ 5 req/s）。远端有 QPS 限制或频繁 429/502 时调大。
            </p>
          </UFormField>
          <div class="lg:col-span-2 flex flex-wrap items-center justify-end gap-2">
            <UButton icon="i-lucide-plus" :loading="saving" @click="createSource">新增远端源</UButton>
          </div>
        </div>
      </UCard>

      <div class="grid gap-3">
        <UCard v-for="source in sources" :key="source.Id">
          <template #header>
            <div class="flex flex-wrap items-center justify-between gap-3">
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <h3 class="text-highlighted truncate text-base font-semibold">{{ source.Name }}</h3>
                  <UBadge :color="sourceStatusColor(source)" variant="soft" size="xs">
                    {{ sourceStatus(source) }}
                  </UBadge>
                  <UBadge :color="source.Enabled ? 'success' : 'neutral'" variant="subtle" size="xs">
                    {{ source.Enabled ? '启用' : '禁用' }}
                  </UBadge>
                </div>
                <p class="text-muted mt-1 text-xs">{{ source.ServerUrl }}</p>
              </div>
              <div class="flex gap-2">
                <UButton
                  color="neutral"
                  variant="outline"
                  size="sm"
                  icon="i-lucide-pencil"
                  :disabled="isSourceSyncing(source)"
                  @click="openEditor(source)"
                >
                  编辑
                </UButton>
                <UButton
                  :color="isSourceFailedLastRun(source) ? 'warning' : 'primary'"
                  variant="soft"
                  size="sm"
                  :icon="isSourceFailedLastRun(source) ? 'i-lucide-refresh-cw-off' : 'i-lucide-refresh-ccw'"
                  :loading="isSourceSyncing(source) && !sourceOperation(source)?.CancelRequested"
                  :disabled="!canSyncSource(source)"
                  @click="syncSource(source)"
                >
                  {{ syncButtonLabel(source) }}
                </UButton>
                <UButton
                  v-if="isSourceSyncing(source)"
                  color="warning"
                  variant="soft"
                  size="sm"
                  icon="i-lucide-circle-stop"
                  :loading="sourceOperation(source)?.CancelRequested"
                  @click="cancelSync(source)"
                >
                  {{ sourceOperation(source)?.CancelRequested ? '取消中…' : '中断同步' }}
                </UButton>
                <UButton
                  color="error"
                  variant="soft"
                  size="sm"
                  icon="i-lucide-trash-2"
                  :disabled="isSourceSyncing(source)"
                  @click="removeSource(source)"
                >
                  删除
                </UButton>
              </div>
            </div>
          </template>

          <div class="grid gap-3 md:grid-cols-4">
            <div class="rounded-lg border border-default p-3">
              <p class="text-muted text-xs">远端账号</p>
              <p class="text-highlighted mt-1 text-sm font-medium">{{ source.Username }}</p>
            </div>
            <div class="rounded-lg border border-default p-3">
              <p class="text-muted text-xs">最近同步</p>
              <p class="text-highlighted mt-1 text-sm font-medium">{{ formatDate(source.LastSyncAt) }}</p>
            </div>
            <div class="rounded-lg border border-default p-3">
              <p class="text-muted text-xs">凭证状态</p>
              <p class="text-highlighted mt-1 text-sm font-medium">
                {{ source.HasAccessToken ? '已缓存 AccessToken' : '尚未缓存' }}
              </p>
              <p class="text-muted text-xs">
                RemoteUserId: {{ source.RemoteUserId || '-' }}
              </p>
            </div>
            <div class="rounded-lg border border-default p-3">
              <p class="text-muted text-xs">显示方式</p>
              <p class="text-highlighted mt-1 text-sm font-medium">
                {{ displayModeLabel(source.DisplayMode) }}
              </p>
            </div>
            <div class="rounded-lg border border-default p-3 md:col-span-2">
              <p class="text-muted text-xs">远端媒体库</p>
              <p class="text-highlighted mt-1 text-sm font-medium">
                {{ sourceRemoteViewsText(source) }}
              </p>
            </div>
            <div class="rounded-lg border border-default p-3 md:col-span-2">
              <p class="text-muted text-xs">STRM 文件 / 侧车同步</p>
              <p class="text-highlighted mt-1 break-all text-sm font-medium">
                {{ source.StrmOutputPath || '未配置（请编辑该源补填 STRM 输出根目录后再同步）' }}
              </p>
              <p class="text-muted mt-1 text-xs">
                侧车：元数据 {{ source.SyncMetadata !== false ? '开' : '关' }} · 外挂字幕
                {{ source.SyncSubtitles !== false ? '开' : '关' }} · 远端令牌刷新 {{ source.TokenRefreshIntervalSecs ?? 3600 }}
                秒
              </p>
              <p class="text-muted mt-1 text-xs">
                流量模式：
                <span :class="source.ProxyMode !== 'proxy' ? 'text-warning font-medium' : 'text-success font-medium'">
                  {{ source.ProxyMode === 'redirect_direct' ? '302 解析直链' : source.ProxyMode === 'redirect' ? '302 重定向' : '本地中转（默认）' }}
                </span>
              </p>
              <p class="text-muted mt-1 text-xs">
                自动增量同步：
                <span
                  :class="(source.AutoSyncIntervalMinutes ?? 0) > 0 ? 'text-success font-medium' : 'text-muted'"
                >
                  {{
                    (source.AutoSyncIntervalMinutes ?? 0) > 0
                      ? `每 ${source.AutoSyncIntervalMinutes} 分钟一次（增/改${source.EnableAutoDelete ? '/删' : ''}）`
                      : '已关闭（仅手动同步或全局计划任务）'
                  }}
                </span>
              </p>
              <p class="text-muted mt-1 text-xs">
                自动删除：
                <span :class="source.EnableAutoDelete ? 'text-warning font-medium' : 'text-muted'">
                  {{ source.EnableAutoDelete ? '已开启（远端下架条目将自动清理）' : '已关闭（仅增/改，不删除）' }}
                </span>
              </p>
              <p v-if="source.LastTokenRefreshAt" class="text-muted text-xs">
                上次远端令牌刷新：{{ formatDate(source.LastTokenRefreshAt) }}
              </p>
            </div>
          </div>

          <UAlert
            v-if="sourceProgressText(source)"
            class="mt-3"
            color="warning"
            icon="i-lucide-timer"
            :description="sourceProgressText(source)"
          />

          <div
            v-if="sourceOperation(source)"
            class="mt-3 grid gap-3 rounded-lg border border-default p-3 text-xs md:grid-cols-3"
          >
            <div>
              <p class="text-muted">阶段</p>
              <p class="text-highlighted mt-1 font-medium">
                {{ syncPhaseLabel(sourceOperation(source)?.Phase, { remotePrefix: false }) || sourceOperation(source)?.Status }}
              </p>
            </div>
            <!-- 层级同步模式（有 CurrentSeries 或 TotalSeries）-->
            <template v-if="sourceOperation(source)?.TotalSeries">
              <div>
                <p class="text-muted">剧集进度</p>
                <p class="text-highlighted mt-1 font-medium">
                  {{ sourceOperation(source)?.ProcessedSeries || 0 }} / {{ sourceOperation(source)?.TotalSeries || 0 }}
                </p>
              </div>
              <div>
                <p class="text-muted">已拉取集数</p>
                <p class="text-highlighted mt-1 font-medium">{{ sourceOperation(source)?.FetchedItems || 0 }}</p>
              </div>
              <div>
                <p class="text-muted">入库条目</p>
                <p class="text-highlighted mt-1 font-medium">{{ sourceOperation(source)?.WrittenFiles || 0 }}</p>
              </div>
              <div
                v-if="sourceOperation(source)?.CurrentSeries"
                class="md:col-span-3"
              >
                <p class="text-muted">正在处理</p>
                <p class="text-highlighted mt-1 font-medium truncate">
                  {{ sourceOperation(source)?.CurrentSeries }}
                </p>
              </div>
              <div
                v-if="(sourceOperation(source)?.SkippedUnchangedSeries || 0) > 0
                  || (sourceOperation(source)?.SkippedUnchangedSeasons || 0) > 0"
                class="md:col-span-3 flex flex-wrap gap-x-4 gap-y-1 text-muted"
              >
                <span v-if="(sourceOperation(source)?.SkippedUnchangedSeries || 0) > 0">
                  跳过未变化剧集：<span class="text-highlighted font-medium">
                    {{ sourceOperation(source)?.SkippedUnchangedSeries || 0 }}
                  </span>
                </span>
                <span v-if="(sourceOperation(source)?.SkippedUnchangedSeasons || 0) > 0">
                  跳过未变化季：<span class="text-highlighted font-medium">
                    {{ sourceOperation(source)?.SkippedUnchangedSeasons || 0 }}
                  </span>
                </span>
              </div>
            </template>
            <!-- 电影/平铺模式 -->
            <template v-else>
              <div>
                <p class="text-muted">已处理 / 总数</p>
                <p class="text-highlighted mt-1 font-medium">
                  {{ sourceOperation(source)?.FetchedItems || 0 }} / {{ sourceOperation(source)?.TotalItems || 0 }}
                </p>
              </div>
              <div>
                <p class="text-muted">入库条目</p>
                <p class="text-highlighted mt-1 font-medium">{{ sourceOperation(source)?.WrittenFiles || 0 }}</p>
              </div>
            </template>
            <!-- 跳过 / 自愈计数 -->
            <div
              v-if="(sourceOperation(source)?.SkippedExisting || 0) > 0
                || (sourceOperation(source)?.StrmMissingReprocessed || 0) > 0"
              class="md:col-span-3 flex flex-wrap gap-x-4 gap-y-1 text-muted"
            >
              <span v-if="(sourceOperation(source)?.SkippedExisting || 0) > 0">
                跳过已入库：<span class="text-highlighted font-medium">
                  {{ sourceOperation(source)?.SkippedExisting || 0 }}
                </span>
                <span class="ml-1">（fast path，本地已有则不重写）</span>
              </span>
              <span v-if="(sourceOperation(source)?.StrmMissingReprocessed || 0) > 0">
                STRM 自愈：<span class="text-highlighted font-medium">
                  {{ sourceOperation(source)?.StrmMissingReprocessed || 0 }}
                </span>
                <span class="ml-1">（本地 strm 文件被删，已重写）</span>
              </span>
            </div>
            <UProgress
              class="md:col-span-3"
              :model-value="sourceOperation(source)?.Progress || 0"
              :max="100"
              :color="sourceStatusColor(source)"
            />
          </div>

          <UAlert
            v-if="source.LastSyncError"
            class="mt-3"
            color="error"
            icon="i-lucide-badge-alert"
            :description="source.LastSyncError"
          />

          <template #footer>
            <div class="space-y-1 text-xs">
              <template v-if="source.ViewLibraryMap && Object.keys(source.ViewLibraryMap).length">
                <p class="text-muted font-medium">映射关系：</p>
                <div v-for="(libId, viewId) in source.ViewLibraryMap" :key="viewId" class="text-muted pl-2">
                  {{ remoteViewNameMap.get(String(viewId).toLowerCase()) || viewId }} → {{ targetLibraryName(String(libId)) }}
                </div>
              </template>
              <p v-else class="text-muted">默认目标库: {{ targetLibraryName(source.TargetLibraryId) }}</p>
              <p class="text-muted break-all font-mono">远端库: {{ source.RemoteViewIds?.join(', ') || 'ALL' }}</p>
              <p class="text-muted break-all font-mono">UA: {{ source.SpoofedClient || 'Infuse-Direct' }}/{{ source.SpoofedAppVersion || '8.2.4' }}</p>
            </div>
          </template>
        </UCard>

        <div
          v-if="!sources.length && !loading"
          class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
        >
          <UIcon name="i-lucide-waypoints" class="size-10 text-muted" />
          <p class="text-muted text-sm">还没有远端 Emby 源，先用上方表单添加一个。</p>
        </div>
      </div>

      <UModal v-model:open="editOpen" :ui="{ content: 'max-w-2xl max-h-[90vh] overflow-y-auto' }">
        <template #header>
          <h3 class="text-highlighted text-base font-semibold">编辑远端 Emby 源</h3>
        </template>
        <template #body>
          <div class="grid gap-3 lg:grid-cols-2">
            <UFormField label="源名称" class="lg:col-span-2">
              <UInput v-model="editForm.name" class="w-full" />
            </UFormField>
            <UFormField label="远端地址" class="lg:col-span-2">
              <UInput v-model="editForm.serverUrl" class="w-full" />
            </UFormField>
            <UFormField label="用户名">
              <UInput v-model="editForm.username" class="w-full" />
            </UFormField>
            <UFormField label="新密码（可选）">
              <UInput
                v-model="editForm.password"
                type="password"
                placeholder="留空则不修改远端密码字段"
                class="w-full"
              />
            </UFormField>
            <UFormField label="远端媒体库" class="lg:col-span-2">
              <USelect
                v-model="editForm.remoteViewIds"
                :items="editRemoteViewItems"
                multiple
                value-key="value"
                class="w-full"
              />
              <p class="text-muted mt-1 text-xs">若某项仅显示 GUID，可先在本页用「获取远端媒体库列表」再打开编辑。</p>
            </UFormField>
            <UFormField label="显示方式（本地）">
              <USelect v-model="editForm.displayMode" :items="displayModeItems" value-key="value" class="w-full" />
            </UFormField>
            <template v-if="editForm.displayMode === 'merge' && editForm.remoteViewIds.length">
              <UFormField label="默认目标本地库（可选）" class="lg:col-span-2">
                <USelect
                  v-model="editForm.targetLibraryId"
                  :items="[{ label: '— 无 —', value: '' }, ...localLibraryItems]"
                  value-key="value"
                  class="w-full"
                  placeholder="未单独指定的远端库将使用此库"
                />
              </UFormField>
              <div class="lg:col-span-2">
                <p class="text-muted mb-2 text-xs font-medium">为每个远端库指定目标本地库：</p>
                <div class="space-y-2">
                  <div
                    v-for="vid in editForm.remoteViewIds"
                    :key="vid"
                    class="flex items-center gap-2 rounded-lg border border-default px-3 py-2"
                  >
                    <span class="min-w-0 flex-1 truncate text-sm font-medium">
                      {{ editRemoteViewNameMap.get(vid.toLowerCase()) || vid }}
                      <span class="text-muted text-xs ml-1">{{ collectionTypeLabel(editRemoteViewCollectionType(vid)) }}</span>
                    </span>
                    <USelect
                      :model-value="editForm.viewLibraryMap[vid] || ''"
                      @update:model-value="(v: string) => { if (v) editForm.viewLibraryMap[vid] = v; else delete editForm.viewLibraryMap[vid]; }"
                      :items="[{ label: '使用默认', value: '' }, ...localLibraryItems]"
                      value-key="value"
                      class="w-48"
                      size="sm"
                    />
                  </div>
                </div>
              </div>
            </template>
            <UFormField v-else-if="editForm.displayMode === 'separate'" label="自动独立说明" class="lg:col-span-2">
              <p class="text-muted text-xs leading-relaxed">
                自动独立模式：同步时将为每个远端媒体库自动创建对应的本地独立媒体库，名称与远端媒体库名相同。<br />
                无需手动指定目标库，已同步的媒体库可在「媒体库管理」中查看。
              </p>
            </UFormField>
            <UFormField label="启用">
              <USwitch v-model="editForm.enabled" />
            </UFormField>
            <UFormField class="lg:col-span-2" label="身份伪装预设（远端 Devices 表显示的客户端）">
              <USelect
                :model-value="editForm.spoofedClient + ' | ' + editForm.spoofedDeviceName + ' | ' + editForm.spoofedAppVersion"
                @update:model-value="(value: string) => {
                  const preset = SPOOFED_CLIENT_PRESETS.find((p) => `${p.client} | ${p.device} | ${p.version}` === value);
                  if (preset) applySpoofedPreset(editForm, preset);
                }"
                :items="SPOOFED_CLIENT_PRESETS.map((p) => ({ label: p.label, value: `${p.client} | ${p.device} | ${p.version}` }))"
                value-key="value"
                class="w-full max-w-md"
              />
            </UFormField>
            <UFormField label="伪装 Client（应用名）">
              <UInput v-model="editForm.spoofedClient" class="w-full" placeholder="Infuse-Direct" />
            </UFormField>
            <UFormField label="伪装 Device（设备名）">
              <UInput v-model="editForm.spoofedDeviceName" class="w-full" placeholder="Apple TV" />
            </UFormField>
            <UFormField label="伪装 App Version" class="lg:col-span-2">
              <UInput v-model="editForm.spoofedAppVersion" class="w-full max-w-xs" placeholder="8.2.4" />
              <p class="text-muted mt-1 text-xs">
                DeviceId =
                <code class="bg-muted px-1 rounded text-xs font-mono">{{ editForm.spoofedDeviceId || '（保存后由后端派生）' }}</code>
                <span class="ml-2">— 一旦写入永不改变，避免远端 Devices 表频繁出现"新设备"触发管理员告警。</span>
              </p>
            </UFormField>
            <UFormField
              class="lg:col-span-2"
              label="STRM 输出根目录"
              required
              :error="!editForm.strmOutputPath.trim() ? '必填，远端 strm/元数据/字幕都将写入此目录' : undefined"
            >
              <UInput
                v-model="editForm.strmOutputPath"
                class="w-full"
                placeholder="例如 D:\Media\remote-strm"
                required
              />
              <p class="text-muted mt-1 text-xs">
                <strong>必填项：</strong>文件写入路径：<code class="bg-muted px-1 rounded text-xs font-mono">{根目录}/{源名称}/{远端媒体库名称}/{影片}.strm</code>。
              </p>
            </UFormField>
            <UFormField label="同步侧车元数据">
              <USwitch v-model="editForm.syncMetadata" />
            </UFormField>
            <UFormField label="外挂字幕下载">
              <USwitch v-model="editForm.syncSubtitles" />
            </UFormField>
            <UFormField class="lg:col-span-2" label="流量模式">
              <div class="flex flex-col gap-2">
                <div class="flex items-start gap-3">
                  <label class="flex cursor-pointer items-center gap-2">
                    <input type="radio" v-model="editForm.proxyMode" value="proxy" class="accent-primary" />
                    <div>
                      <span class="font-medium text-sm">本地中转（推荐）</span>
                      <p class="text-muted text-xs">所有媒体流通过本地服务器转发，客户端无需访问远端服务器</p>
                    </div>
                  </label>
                </div>
                <div class="flex items-start gap-3">
                  <label class="flex cursor-pointer items-center gap-2">
                    <input type="radio" v-model="editForm.proxyMode" value="redirect" class="accent-primary" />
                    <div>
                      <span class="font-medium text-sm">302 重定向</span>
                      <p class="text-muted text-xs">302 重定向到远端 Emby，客户端自行跟随 302 链获取最终直链。适合有 IP 绑定的 CDN（如115网盘）</p>
                    </div>
                  </label>
                </div>
                <div class="flex items-start gap-3">
                  <label class="flex cursor-pointer items-center gap-2">
                    <input type="radio" v-model="editForm.proxyMode" value="redirect_direct" class="accent-primary" />
                    <div>
                      <span class="font-medium text-sm">302 解析直链</span>
                      <p class="text-muted text-xs">服务端解析远端 302 链后返回最终 CDN 直链，减少一跳更快。不适合有 IP 绑定的 CDN</p>
                    </div>
                  </label>
                </div>
              </div>
            </UFormField>
            <UFormField class="lg:col-span-2" label="远端令牌刷新间隔（秒）">
              <UInput v-model.number="editForm.tokenRefreshIntervalSecs" type="number" class="max-w-xs" :min="300" />
              <p class="text-muted mt-1 text-xs">
                300–2592000。后台任务据此周期刷新本地缓存的远端鉴权令牌，STRM 文件指向本地代理无需嵌入 api_key。
              </p>
            </UFormField>
            <UFormField class="lg:col-span-2" label="自动增量同步间隔（分钟）">
              <UInput
                v-model.number="editForm.autoSyncIntervalMinutes"
                type="number"
                class="max-w-xs"
                :min="0"
                :max="60 * 24 * 7"
                placeholder="0 = 关闭，30 表示每 30 分钟一次"
              />
              <p class="text-muted mt-1 text-xs">
                0 = 关闭。后台每分钟检查该源，到达间隔即触发增量同步（增 / 改）。范围 1–10080（最长 7 天）。
              </p>
            </UFormField>
            <UFormField class="lg:col-span-2" label="自动删除远端已下架条目">
              <USwitch v-model="editForm.enableAutoDelete" />
              <p class="text-muted mt-1 text-xs">
                开启后，同步时会对比远端与本地的条目列表，自动删除远端已不存在的 Series / Movie 及其 STRM 文件。关闭时仅同步新增和变更，不执行任何删除操作。
              </p>
            </UFormField>
            <UFormField class="lg:col-span-2" label="拉取速率：单页条目数（PageSize）">
              <UInput
                v-model.number="editForm.pageSize"
                type="number"
                class="max-w-xs"
                :min="50"
                :max="1000"
                placeholder="默认 200"
              />
              <p class="text-muted mt-1 text-xs">
                范围 50–1000，默认 200。影响 Series / Movie 列表的分页大小（Seasons 和 Episodes 由 Emby 一次返回，不受此值限制）。越大越省请求数但单次响应体越大；远端带宽紧张可调小。
              </p>
            </UFormField>
            <UFormField class="lg:col-span-2" label="拉取速率：请求最小间隔（毫秒）">
              <UInput
                v-model.number="editForm.requestIntervalMs"
                type="number"
                class="max-w-xs"
                :min="0"
                :max="60000"
                placeholder="0 = 不限速"
              />
              <p class="text-muted mt-1 text-xs">
                范围 0–60000，默认 0（不限速）。所有远端 API 请求均受此间隔约束；峰值 QPS ≈ 1000 / 该值（如 200 ms ≈ 5 req/s）。远端有 QPS 限制或频繁 429/502 时调大。
              </p>
            </UFormField>
            <div class="lg:col-span-2 mt-2 flex flex-wrap justify-end gap-2">
              <UButton color="neutral" variant="subtle" @click="editOpen = false">取消</UButton>
              <UButton icon="i-lucide-save" :loading="editSaving" @click="saveEditor">保存</UButton>
            </div>
          </div>
        </template>
      </UModal>
    </div>
  </SettingsLayout>
</template>
