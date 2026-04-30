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

const DEFAULT_SPOOFED_USER_AGENT =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) EmbyTheater/3.0.20 Chrome/124.0.0.0 Safari/537.36';

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
  displayMode: 'separate' as 'merge' | 'separate',
  spoofedUserAgent: DEFAULT_SPOOFED_USER_AGENT,
  enabled: true,
  strmOutputPath: '',
  syncMetadata: true,
  syncSubtitles: true,
  tokenRefreshIntervalSecs: 3600,
  proxyMode: 'proxy' as 'proxy' | 'redirect'
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
  displayMode: 'separate' as 'merge' | 'separate',
  spoofedUserAgent: DEFAULT_SPOOFED_USER_AGENT,
  enabled: true,
  strmOutputPath: '',
  syncMetadata: true,
  syncSubtitles: true,
  tokenRefreshIntervalSecs: 3600,
  proxyMode: 'proxy' as 'proxy' | 'redirect',
  mergedRemoteViews: [] as RemoteEmbyView[]
});

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
  { label: '独立媒体库（每个远端库单独建库）', value: 'separate' },
  { label: '并入现有媒体库', value: 'merge' }
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
  return mode === 'merge' ? '并入现有媒体库' : '独立媒体库';
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
  return `${running.SourceName} · ${running.Phase || running.Status} · ${Math.round(running.Progress || 0)}% · ${runtime} 秒`;
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
    return `${operation.Phase || operation.Status} (${progress}%)`;
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
    return `阶段 ${operation.Phase || operation.Status} · ${progress}% · 已运行 ${runtime} 秒`;
  }
  if (operation.Status === 'Succeeded') {
    const writtenFiles = operation.Result?.WrittenFiles ?? operation.WrittenFiles ?? 0;
    return `最近任务完成 · 入库 ${writtenFiles} 个条目`;
  }
  if (operation.Status === 'Failed') {
    return `最近任务失败 · 已运行 ${runtime} 秒`;
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

function buildOperationMap(operations: RemoteEmbySyncOperation[]) {
  const next: Record<string, RemoteEmbySyncOperation> = {};
  for (const operation of operations) {
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
    const results = await Promise.all(
      pending.map((operation) => api.remoteEmbySyncOperation(operation.Id))
    );
    const next = { ...operationBySourceId.value };
    let shouldRefreshSources = false;
    for (const operation of results) {
      next[operation.SourceId] = operation;
      if (operation.Done) {
        shouldRefreshSources = true;
      }
    }
    operationBySourceId.value = next;
    if (shouldRefreshSources) {
      await refreshSourcesOnly();
    }
    if (results.every((operation) => operation.Done)) {
      stopPolling();
    }
  } catch (err) {
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
    spoofedUserAgent: source.SpoofedUserAgent || DEFAULT_SPOOFED_USER_AGENT,
    enabled: source.Enabled,
    strmOutputPath: source.StrmOutputPath || '',
    syncMetadata: source.SyncMetadata !== false,
    syncSubtitles: source.SyncSubtitles !== false,
    tokenRefreshIntervalSecs: source.TokenRefreshIntervalSecs ?? 3600,
    proxyMode: (source.ProxyMode === 'redirect' ? 'redirect' : 'proxy') as 'proxy' | 'redirect',
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
  if (p.displayMode === 'merge' && !p.targetLibraryId) {
    error.value = '并入模式下请选择项目媒体库';
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
      SpoofedUserAgent: string;
      Enabled: boolean;
      StrmOutputPath: string;
      SyncMetadata: boolean;
      SyncSubtitles: boolean;
      TokenRefreshIntervalSecs: number;
      ProxyMode: 'proxy' | 'redirect';
    } = {
      Name: p.name.trim(),
      ServerUrl: p.serverUrl.trim(),
      Username: p.username.trim(),
      TargetLibraryId: p.targetLibraryId,
      DisplayMode: p.displayMode,
      RemoteViewIds: p.remoteViewIds,
      RemoteViews: selectedRemoteViews,
      SpoofedUserAgent: p.spoofedUserAgent.trim(),
      Enabled: p.enabled,
      StrmOutputPath: p.strmOutputPath.trim(),
      SyncMetadata: p.syncMetadata,
      SyncSubtitles: p.syncSubtitles,
      TokenRefreshIntervalSecs: Math.min(
        Math.max(Number(p.tokenRefreshIntervalSecs) || 3600, 300),
        86400 * 30
      ),
      ProxyMode: p.proxyMode
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
  if (payload.displayMode === 'merge' && !payload.targetLibraryId) {
    error.value = '并入模式下请选择项目媒体库';
    return;
  }
  if (!payload.spoofedUserAgent.trim()) {
    error.value = '请输入伪装 User-Agent';
    return;
  }

  // separate 模式下 targetLibraryId 可为空，后端会自动使用「远端 Emby 中转」库
  const selectedLocalLibraryId =
    payload.displayMode === 'merge'
      ? payload.targetLibraryId || localLibraries.value[0]?.ItemId || ''
      : payload.targetLibraryId || localLibraries.value[0]?.ItemId || '';

  if (payload.displayMode === 'merge' && !selectedLocalLibraryId) {
    error.value = '并入模式下请先创建本地媒体库并选择目标库';
    return;
  }

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
      SpoofedUserAgent: payload.spoofedUserAgent.trim(),
      Enabled: payload.enabled,
      StrmOutputPath: payload.strmOutputPath.trim() || undefined,
      SyncMetadata: payload.syncMetadata,
      SyncSubtitles: payload.syncSubtitles,
      TokenRefreshIntervalSecs: Math.min(
        Math.max(Number(payload.tokenRefreshIntervalSecs) || 3600, 300),
        86400 * 30
      ),
      ProxyMode: payload.proxyMode
    });
    saved.value = `已创建远端源：${payload.name.trim()}`;
    form.value.name = '';
    form.value.serverUrl = '';
    form.value.username = '';
    form.value.password = '';
    form.value.remoteViewIds = [];
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
      SpoofedUserAgent: payload.spoofedUserAgent.trim()
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
    const queued = await api.startRemoteEmbySync(source.Id);
    operationBySourceId.value[source.Id] = queued.Operation;
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
          <UFormField v-if="form.displayMode === 'merge'" label="并入项目媒体库">
            <USelect
              v-model="form.targetLibraryId"
              :items="localLibraryItems"
              value-key="value"
              class="w-full"
              placeholder="选择项目本地媒体库"
            />
          </UFormField>
          <UFormField v-else label="独立媒体库说明" class="lg:col-span-2">
            <p class="text-muted text-xs leading-relaxed">
              单独显示模式：同步时将为每个远端媒体库自动创建对应的本地独立媒体库，名称与远端媒体库相同。<br />
              无需手动指定目标库，首次同步后可在「媒体库管理」中查看自动创建的媒体库。
            </p>
          </UFormField>
          <UFormField label="启用状态">
            <USwitch v-model="form.enabled" />
          </UFormField>
          <UFormField class="lg:col-span-2" label="伪装 User-Agent">
            <UTextarea
              v-model="form.spoofedUserAgent"
              :rows="2"
              class="w-full"
              placeholder="填写你要用于远端请求的 UA"
            />
          </UFormField>
          <UFormField class="lg:col-span-2" label="STRM 输出根目录（可选）">
            <UInput
              v-model="form.strmOutputPath"
              class="w-full"
              placeholder="例如 D:\Media\remote-strm"
            />
            <p class="text-muted mt-1 text-xs">
              留空则仅使用虚拟路径入库，不写磁盘文件。<br />
              填写后，同步时将按以下层级结构写入 .strm / NFO / 图片：<br />
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
                    <span class="font-medium text-sm">302 直链重定向</span>
                    <p class="text-muted text-xs">返回 302 重定向到远端直链，节省本地带宽；客户端需能直接访问远端 Emby 服务器</p>
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
          <div class="lg:col-span-2 flex flex-wrap items-center justify-between gap-2">
            <UButton
              color="neutral"
              variant="subtle"
              icon="i-lucide-rotate-ccw"
              @click="form.spoofedUserAgent = DEFAULT_SPOOFED_USER_AGENT"
            >
              恢复默认 UA
            </UButton>
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
                  color="primary"
                  variant="soft"
                  size="sm"
                  icon="i-lucide-refresh-ccw"
                  :loading="isSourceSyncing(source) && !sourceOperation(source)?.CancelRequested"
                  :disabled="!canSyncSource(source)"
                  @click="syncSource(source)"
                >
                  {{ isSourceSyncing(source) ? '同步中' : '立即同步' }}
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
                {{ source.StrmOutputPath || '未配置 STRM 根目录（仅虚拟路径入库）' }}
              </p>
              <p class="text-muted mt-1 text-xs">
                侧车：元数据 {{ source.SyncMetadata !== false ? '开' : '关' }} · 外挂字幕
                {{ source.SyncSubtitles !== false ? '开' : '关' }} · 远端令牌刷新 {{ source.TokenRefreshIntervalSecs ?? 3600 }}
                秒
              </p>
              <p class="text-muted mt-1 text-xs">
                流量模式：
                <span :class="source.ProxyMode === 'redirect' ? 'text-warning font-medium' : 'text-success font-medium'">
                  {{ source.ProxyMode === 'redirect' ? '302 直链重定向（节省带宽）' : '本地中转（默认）' }}
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
                {{ sourceOperation(source)?.Phase || sourceOperation(source)?.Status }}
              </p>
            </div>
            <div>
              <p class="text-muted">远端抓取</p>
              <p class="text-highlighted mt-1 font-medium">
                {{ sourceOperation(source)?.FetchedItems || 0 }} / {{ sourceOperation(source)?.TotalItems || 0 }}
              </p>
            </div>
            <div>
              <p class="text-muted">入库条目</p>
              <p class="text-highlighted mt-1 font-medium">{{ sourceOperation(source)?.WrittenFiles || 0 }}</p>
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
              <p class="text-muted">并入项目媒体库: {{ targetLibraryName(source.TargetLibraryId) }}</p>
              <p class="text-muted break-all font-mono">目标库 ID: {{ source.TargetLibraryId }}</p>
              <p class="text-muted break-all font-mono">远端库 ID: {{ source.RemoteViewIds?.join(', ') || 'ALL' }}</p>
              <p class="text-muted break-all font-mono">UA: {{ source.SpoofedUserAgent }}</p>
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
            <UFormField v-if="editForm.displayMode === 'merge'" label="并入项目媒体库" class="lg:col-span-2">
              <USelect
                v-model="editForm.targetLibraryId"
                :items="localLibraryItems"
                value-key="value"
                class="w-full"
                placeholder="选择本地库"
              />
            </UFormField>
            <UFormField v-else label="独立媒体库说明" class="lg:col-span-2">
              <p class="text-muted text-xs leading-relaxed">
                单独显示模式：同步时将为每个远端媒体库自动创建对应的本地独立媒体库，名称与远端媒体库名相同。<br />
                无需手动指定目标库，已同步的媒体库可在「媒体库管理」中查看。
              </p>
            </UFormField>
            <UFormField label="启用">
              <USwitch v-model="editForm.enabled" />
            </UFormField>
            <UFormField class="lg:col-span-2" label="伪装 User-Agent">
              <UTextarea v-model="editForm.spoofedUserAgent" :rows="2" class="w-full" />
            </UFormField>
            <UFormField class="lg:col-span-2" label="STRM 输出根目录（可选）">
              <UInput
                v-model="editForm.strmOutputPath"
                class="w-full"
                placeholder="留空则清除 STRM 根路径配置（不再写磁盘文件）"
              />
              <p class="text-muted mt-1 text-xs">
                文件写入路径：<code class="bg-muted px-1 rounded text-xs font-mono">{根目录}/{源名称}/{远端媒体库名称}/{影片}.strm</code>。<br />
                保存为空即清除配置，后续同步仅使用虚拟路径，不写磁盘文件。
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
                      <span class="font-medium text-sm">302 直链重定向</span>
                      <p class="text-muted text-xs">返回 302 重定向到远端直链，节省本地带宽；客户端需能直接访问远端 Emby 服务器</p>
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
