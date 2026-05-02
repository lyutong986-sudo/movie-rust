<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import AddLibraryDialog from '../../components/AddLibraryDialog.vue';
import ContextMenu from '../../components/ContextMenu.vue';
import type { ContextMenuItem } from '../../components/ContextMenu.vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import {
  api,
  cancelCurrentScan,
  deleteLibrary,
  hydrateScanOperation,
  isAdmin,
  libraries,
  loadAdminData,
  loadLibraries,
  scanOperation,
  loadVirtualFolders,
  scan,
  scanIncremental,
  state,
  totalLibraryItems,
  virtualFolders
} from '../../store/app';
import { useAppToast } from '../../composables/toast';
import type { VirtualFolderInfo } from '../../api/emby';

const toast = useAppToast();

onMounted(async () => {
  if (isAdmin.value) {
    await Promise.all([
      loadAdminData(),
      loadLibraries(),
      loadVirtualFolders(),
      hydrateScanOperation()
    ]);
  }
});

const scanStatusColor = computed(() => {
  const status = scanOperation.value?.Status;
  if (!status) return 'neutral';
  if (status === 'Succeeded') return 'success';
  if (status === 'Failed') return 'error';
  if (status === 'Cancelled') return 'neutral';
  return 'warning';
});

const canCancelScan = computed(() => {
  const status = scanOperation.value?.Status;
  return status === 'Queued' || status === 'Running' || status === 'Cancelling';
});
const scanRunning = computed(() => canCancelScan.value);
const requestedLibraryId = ref<string | null>(null);
watch(scanRunning, (running) => {
  if (!running) requestedLibraryId.value = null;
});

const libraryCountById = computed(() => {
  const map = new Map<string, number>();
  for (const library of libraries.value) {
    if (library?.Id) {
      map.set(library.Id, library.ChildCount || 0);
    }
  }
  return map;
});

const scanProgressText = computed(() => {
  const progress = scanOperation.value?.Progress;
  if (typeof progress !== 'number' || !Number.isFinite(progress)) return '0%';
  return `${Math.round(progress)}%`;
});

const scanPhaseLabel = computed(() => {
  const phase = scanOperation.value?.Phase ?? '';
  // PB49：后端在自动重试时把 phase 设成 Retrying(N/M)
  const retryMatch = /^Retrying\((\d+)\/(\d+)\)$/.exec(phase);
  if (retryMatch) return `重试中 ${retryMatch[1]}/${retryMatch[2]}`;
  switch (phase) {
    case 'CollectingFiles':
      return '收集文件中';
    case 'Importing':
      return '入库中';
    case 'PostProcessing':
      return '后处理';
    case 'Completed':
      return '已完成';
    case 'Cancelled':
      return '已取消';
    case 'Failed':
      return '已失败';
    case 'Queued':
      return '排队中';
    default:
      return phase || '—';
  }
});

const scanFileCounter = computed(() => {
  const op = scanOperation.value;
  if (!op) return '';
  const total = op.TotalFiles || 0;
  const scanned = op.ScannedFiles || 0;
  if (total === 0) return scanned ? `${scanned} 个` : '';
  return `${scanned} / ${total}`;
});

const scanImportedCounter = computed(() => {
  const imported = scanOperation.value?.ImportedItems || 0;
  return imported ? `已入库 ${imported} 个` : '';
});

// PB49 (S1)：scanner 远端 STRM 短路计数。本地 scanner 在 Phase B 入口会用一次
// 批量 SQL 查到所有「DB 已存在 + 由远端 source 管理」的 path，对其中文件 mtime
// 不晚于 DB updated_at 的直接跳过整套 import 链路。这里把跳过数量露出来，让用户
// 看到「为什么远端 sync 跑完后这么快就完事」。
const scanSkippedRemoteText = computed(() => {
  const skipped = scanOperation.value?.SkippedRemoteStrm || 0;
  return skipped > 0 ? `跳过远端已同步 ${skipped} 个（fast path）` : '';
});

const scanRateText = computed(() => {
  const rate = scanOperation.value?.ScanRatePerSec;
  if (typeof rate !== 'number' || !Number.isFinite(rate) || rate <= 0) return '';
  return `${rate.toFixed(1)} 文件/秒`;
});

/**
 * PB49：媒体库扫描任务支持自动重试（最多 3 次），重试时计数器单调推进，
 * 不再像旧版本一样回到 0。这里把「正在重试 N/M」的状态文案露出来给用户，
 * 避免用户看到 phase=Retrying(2/3) 时再次以为扫描卡住或回退。
 */
const scanRetryHint = computed(() => {
  const op = scanOperation.value;
  if (!op) return '';
  const phase = op.Phase || '';
  const match = /^Retrying\((\d+)\/(\d+)\)$/.exec(phase);
  if (match) {
    return `第 ${match[1]} / ${match[2]} 次重试中（计数器为已扫高水位线，不会回到 0）`;
  }
  if (op.Status === 'Running' && (op.Attempts || 0) > 1) {
    return `重试 ${op.Attempts} / ${op.MaxAttempts || 3} 次中`;
  }
  return '';
});

function formatTime(value?: string | null) {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '-';
  return date.toLocaleString();
}

function monitorUrl() {
  const id = scanOperation.value?.Id;
  if (!id) return '-';
  return `/api/admin/scan/operations/${id}`;
}

async function openMonitor() {
  const id = scanOperation.value?.Id;
  if (!id) return;
  const operation = await api.scanOperation(id);
  scanOperation.value = operation;
}

function collectionLabel(type: string) {
  if (type === 'tvshows') return '电视剧';
  if (type === 'music') return '音乐';
  if (type === 'musicvideos') return '音乐视频';
  if (type === 'photos') return '照片';
  if (type === 'homevideos') return '家庭视频';
  if (type === 'mixed') return '混合内容';
  return '电影';
}

function collectionIcon(type: string) {
  if (type === 'tvshows') return 'i-lucide-tv';
  if (type === 'music') return 'i-lucide-music';
  if (type === 'musicvideos') return 'i-lucide-music-4';
  if (type === 'photos') return 'i-lucide-image';
  if (type === 'homevideos') return 'i-lucide-video';
  if (type === 'mixed') return 'i-lucide-layers';
  return 'i-lucide-film';
}

function edit(folder: VirtualFolderInfo) {
  state.editingLibrary = folder;
}

function folderCount(folder: VirtualFolderInfo) {
  return libraryCountById.value.get(folder.ItemId) || 0;
}

function folderScanStatus(folder: VirtualFolderInfo) {
  if (!scanRunning.value) return 'Idle';
  const activeLibraryId = scanOperation.value?.LibraryId || requestedLibraryId.value;
  if (!activeLibraryId) return 'Running (All)';
  if (activeLibraryId === folder.ItemId) return scanOperation.value?.Status || 'Running';
  return 'Queued';
}

function isFolderScanButtonDisabled(folder: VirtualFolderInfo) {
  if (state.busy) return true;
  if (!scanRunning.value) return false;
  const activeLibraryId = scanOperation.value?.LibraryId || requestedLibraryId.value;
  if (!activeLibraryId) return true;
  return activeLibraryId === folder.ItemId;
}

async function scanFolder(folder: VirtualFolderInfo) {
  requestedLibraryId.value = folder.ItemId;
  await scan(folder.ItemId);
}

async function scanAllLibraries() {
  requestedLibraryId.value = null;
  await scan();
}

async function scanFolderIncremental(folder: VirtualFolderInfo) {
  requestedLibraryId.value = folder.ItemId;
  await scanIncremental(folder.Locations);
}

async function scanAllLibrariesIncremental() {
  requestedLibraryId.value = null;
  const allPaths = virtualFolders.value.flatMap((folder) => folder.Locations || []);
  await scanIncremental(allPaths);
}

async function remove(folder: VirtualFolderInfo) {
  const confirmed = window.confirm(`删除媒体库"${folder.Name}"？媒体文件不会被删除。`);
  if (!confirmed) {
    return;
  }
  await deleteLibrary(folder);
}

const libCtxMenu = ref<InstanceType<typeof ContextMenu> | null>(null);
const libCtxFolder = ref<VirtualFolderInfo | null>(null);
const renameDialogOpen = ref(false);
const renameValue = ref('');

function openLibraryContextMenu(e: MouseEvent, folder: VirtualFolderInfo) {
  libCtxFolder.value = folder;
  libCtxMenu.value?.show(e);
}

const libMenuItems = computed<ContextMenuItem[][]>(() => {
  const folder = libCtxFolder.value;
  if (!folder) return [];
  return [
    [
      { label: '编辑', icon: 'i-lucide-pencil', onSelect: () => edit(folder) },
      {
        label: '重命名',
        icon: 'i-lucide-text-cursor-input',
        onSelect: () => {
          renameValue.value = folder.Name;
          renameDialogOpen.value = true;
        }
      }
    ],
    [
      {
        label: '刷新元数据',
        icon: 'i-lucide-refresh-cw',
        onSelect: async () => {
          try {
            await api.refreshItemMetadata(folder.ItemId);
            toast.success('元数据刷新已提交');
          } catch {
            toast.error('刷新元数据失败');
          }
        }
      },
      {
        label: '扫描媒体库文件',
        icon: 'i-lucide-refresh-ccw',
        disabled: isFolderScanButtonDisabled(folder),
        onSelect: () => scanFolder(folder)
      },
      {
        label: '增量扫描',
        icon: 'i-lucide-git-compare-arrows',
        disabled: isFolderScanButtonDisabled(folder),
        onSelect: () => scanFolderIncremental(folder)
      }
    ],
    [
      {
        label: '删除',
        icon: 'i-lucide-trash-2',
        color: 'error',
        onSelect: () => remove(folder)
      }
    ]
  ];
});

async function submitRename() {
  const folder = libCtxFolder.value;
  if (!folder || !renameValue.value.trim()) return;
  const newName = renameValue.value.trim();
  if (newName === folder.Name) {
    renameDialogOpen.value = false;
    return;
  }
  try {
    await api.renameVirtualFolder(folder.Name, newName);
    toast.success(`已重命名为"${newName}"`);
    renameDialogOpen.value = false;
    await loadVirtualFolders();
  } catch (err: any) {
    toast.error('重命名失败: ' + (err?.message || err));
  }
}
</script>

<template>
  <SettingsLayout>
    <div
      v-if="!isAdmin"
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
    >
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能创建、编辑或删除媒体库。</p>
    </div>

    <div v-else class="space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">Libraries</p>
          <h2 class="text-highlighted text-xl font-semibold">媒体库管理</h2>
        </div>
        <div class="flex gap-2">
          <UButton icon="i-lucide-plus" @click="state.showAddLibrary = true">添加媒体库</UButton>
          <UButton
            color="neutral"
            variant="outline"
            icon="i-lucide-git-compare-arrows"
            :loading="state.busy"
            @click="scanAllLibrariesIncremental"
          >
            增量扫描
          </UButton>
          <UButton
            color="neutral"
            variant="subtle"
            icon="i-lucide-refresh-ccw"
            :loading="state.busy"
            @click="scanAllLibraries"
          >
            扫描所有媒体库
          </UButton>
        </div>
      </div>

      <UAlert v-if="state.error" color="error" icon="i-lucide-triangle-alert" :description="state.error" />
      <UAlert v-else-if="state.message" color="success" icon="i-lucide-check" :description="state.message" />

      <UCard variant="soft">
        <template #header>
          <div class="flex items-center justify-between gap-3">
            <div>
              <p class="text-muted text-xs uppercase tracking-wider">Tasks</p>
              <h3 class="text-highlighted text-base font-semibold">媒体库扫描任务</h3>
            </div>
            <div class="flex items-center gap-2">
              <UBadge :color="scanStatusColor" variant="soft" size="sm">
                {{ scanOperation?.Status || 'Idle' }}
              </UBadge>
              <UButton
                color="error"
                variant="soft"
                size="sm"
                icon="i-lucide-square"
                :disabled="!canCancelScan || state.busy"
                @click="cancelCurrentScan"
              >
                取消
              </UButton>
            </div>
          </div>
        </template>
        <div class="grid gap-3 md:grid-cols-2">
          <div class="rounded-lg border border-default p-3">
            <p class="text-muted text-xs">数据库片源（实时统计）</p>
            <p class="text-highlighted mt-1 text-2xl font-semibold">{{ totalLibraryItems }}</p>
            <p class="text-muted mt-1 text-xs">
              所有媒体库的 media_items 行数总和；与本次扫描进度独立。
            </p>
          </div>
          <div class="rounded-lg border border-default p-3">
            <p class="text-muted text-xs">扫描进度 · {{ scanPhaseLabel }}</p>
            <p class="text-highlighted mt-1 text-2xl font-semibold">{{ scanProgressText }}</p>
            <p v-if="scanFileCounter" class="text-muted mt-1 text-xs">
              文件 {{ scanFileCounter }}<span v-if="scanImportedCounter"> · {{ scanImportedCounter }}</span><span v-if="scanRateText"> · {{ scanRateText }}</span>
            </p>
            <p v-if="scanSkippedRemoteText" class="text-muted mt-1 text-xs">
              {{ scanSkippedRemoteText }}
            </p>
            <p v-if="scanOperation?.CurrentLibrary" class="text-muted text-xs">
              当前：{{ scanOperation.CurrentLibrary }}
            </p>
            <p v-if="scanRetryHint" class="text-warning mt-1 text-xs">
              {{ scanRetryHint }}
            </p>
            <UProgress
              class="mt-2"
              :model-value="scanOperation?.Progress || 0"
              :max="100"
              :color="scanStatusColor"
            />
          </div>
          <div class="rounded-lg border border-default p-3">
            <p class="text-muted text-xs">开始时间</p>
            <p class="text-highlighted mt-1 text-sm font-medium">{{ formatTime(scanOperation?.StartedAt) }}</p>
          </div>
          <div class="rounded-lg border border-default p-3">
            <p class="text-muted text-xs">结束时间</p>
            <p class="text-highlighted mt-1 text-sm font-medium">{{ formatTime(scanOperation?.CompletedAt) }}</p>
          </div>
        </div>

        <div class="mt-3 space-y-2">
          <p class="text-muted text-xs">错误详情</p>
          <UAlert
            v-if="scanOperation?.Error"
            color="error"
            icon="i-lucide-badge-alert"
            :description="scanOperation.Error"
          />
          <p v-else class="text-muted rounded-lg border border-default bg-elevated/40 p-3 text-xs">
            当前无错误
          </p>
        </div>

        <template #footer>
          <div class="flex flex-wrap items-center justify-between gap-2 text-xs">
            <span class="text-muted font-mono">Monitor: {{ monitorUrl() }}</span>
            <UButton color="neutral" variant="subtle" size="sm" icon="i-lucide-refresh-cw" @click="openMonitor">
              刷新状态
            </UButton>
          </div>
        </template>
      </UCard>

      <div v-if="virtualFolders.length" class="grid gap-3 md:grid-cols-2">
        <UCard v-for="folder in virtualFolders" :key="folder.ItemId" @contextmenu="openLibraryContextMenu($event, folder)">
          <div class="flex items-start gap-3">
            <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <UIcon :name="collectionIcon(folder.CollectionType)" class="size-5" />
            </div>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <h3 class="text-highlighted truncate text-base font-semibold">{{ folder.Name }}</h3>
                <UBadge variant="soft" color="primary" size="xs">
                  {{ collectionLabel(folder.CollectionType) }}
                </UBadge>
                <UBadge variant="subtle" color="neutral" size="xs">
                  片源 {{ folderCount(folder) }}
                </UBadge>
                <UBadge
                  v-if="scanRunning"
                  variant="soft"
                  :color="(scanOperation?.LibraryId || requestedLibraryId) === folder.ItemId ? scanStatusColor : 'neutral'"
                  size="xs"
                >
                  {{ folderScanStatus(folder) }}
                </UBadge>
              </div>
              <p class="text-muted mt-1 break-all font-mono text-xs">
                {{ folder.Locations.join(' · ') }}
              </p>
            </div>
          </div>

          <div class="mt-3 flex flex-wrap gap-1.5">
            <UBadge :color="folder.LibraryOptions.Enabled ? 'success' : 'neutral'" variant="subtle" size="xs">启用</UBadge>
            <UBadge :color="folder.LibraryOptions.EnableRealtimeMonitor ? 'success' : 'neutral'" variant="subtle" size="xs">实时监控</UBadge>
            <UBadge :color="folder.LibraryOptions.EnableInternetProviders ? 'success' : 'neutral'" variant="subtle" size="xs">互联网元数据</UBadge>
            <UBadge :color="folder.LibraryOptions.DownloadImagesInAdvance ? 'success' : 'neutral'" variant="subtle" size="xs">预下载图片</UBadge>
            <UBadge :color="folder.LibraryOptions.SaveLocalMetadata ? 'success' : 'neutral'" variant="subtle" size="xs">保存 NFO</UBadge>
            <UBadge :color="folder.LibraryOptions.ImportMissingEpisodes ? 'success' : 'neutral'" variant="subtle" size="xs">缺失剧集</UBadge>
            <UBadge :color="folder.LibraryOptions.ImportCollections ? 'success' : 'neutral'" variant="subtle" size="xs">电影合集</UBadge>
            <UBadge :color="!folder.LibraryOptions.ExcludeFromSearch ? 'success' : 'neutral'" variant="subtle" size="xs">参与搜索</UBadge>
            <UBadge :color="folder.LibraryOptions.EnableChapterImageExtraction ? 'success' : 'neutral'" variant="subtle" size="xs">章节图片</UBadge>
            <UBadge :color="folder.LibraryOptions.EnableAutomaticSeriesGrouping ? 'success' : 'neutral'" variant="subtle" size="xs">剧集自动分组</UBadge>
            <UBadge variant="outline" color="neutral" size="xs">
              {{ folder.LibraryOptions.PreferredMetadataLanguage || 'zh' }}-{{ folder.LibraryOptions.MetadataCountryCode || 'CN' }}
            </UBadge>
          </div>

          <template #footer>
            <div class="flex flex-wrap gap-2">
              <UButton color="neutral" variant="subtle" icon="i-lucide-pencil" :disabled="state.busy" @click="edit(folder)">
                编辑
              </UButton>
              <UButton
                color="neutral"
                variant="outline"
                icon="i-lucide-git-compare-arrows"
                :disabled="isFolderScanButtonDisabled(folder)"
                @click="scanFolderIncremental(folder)"
              >
                增量
              </UButton>
              <UButton
                color="neutral"
                variant="subtle"
                icon="i-lucide-refresh-ccw"
                :disabled="isFolderScanButtonDisabled(folder)"
                @click="scanFolder(folder)"
              >
                {{ (scanOperation?.LibraryId || requestedLibraryId) === folder.ItemId && scanRunning ? '扫描中' : '扫描' }}
              </UButton>
              <UButton color="error" variant="soft" icon="i-lucide-trash-2" :disabled="state.busy" @click="remove(folder)">
                删除
              </UButton>
            </div>
          </template>
        </UCard>
      </div>

      <div
        v-else
        class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
      >
        <UIcon name="i-lucide-library" class="size-10 text-muted" />
        <h3 class="text-highlighted text-lg font-semibold">还没有媒体库</h3>
        <p class="text-muted max-w-md text-sm">
          添加电影或电视剧目录后，首页和播放器就会按 Emby 的媒体库结构加载内容。
        </p>
        <UButton icon="i-lucide-plus" @click="state.showAddLibrary = true">添加媒体库</UButton>
      </div>
    </div>

    <AddLibraryDialog
      v-if="state.showAddLibrary"
      :open="state.showAddLibrary"
      @update:open="(v: boolean) => (state.showAddLibrary = v)"
      @close="state.showAddLibrary = false"
    />
    <AddLibraryDialog
      v-if="state.editingLibrary"
      :open="!!state.editingLibrary"
      :folder="state.editingLibrary"
      @update:open="(v: boolean) => { if (!v) state.editingLibrary = null }"
      @close="state.editingLibrary = null"
    />

    <ContextMenu
      ref="libCtxMenu"
      :items="libMenuItems"
      :preview-title="libCtxFolder?.Name"
      :preview-subtitle="libCtxFolder ? collectionLabel(libCtxFolder.CollectionType) : undefined"
    />

    <UModal v-model:open="renameDialogOpen">
      <template #content>
        <div class="p-6 space-y-4">
          <h3 class="text-lg font-semibold text-highlighted">重命名媒体库</h3>
          <UInput v-model="renameValue" autofocus placeholder="新名称" class="w-full" @keydown.enter="submitRename" />
          <div class="flex justify-end gap-2">
            <UButton color="neutral" variant="outline" @click="renameDialogOpen = false">取消</UButton>
            <UButton @click="submitRename">确认</UButton>
          </div>
        </div>
      </template>
    </UModal>
  </SettingsLayout>
</template>
