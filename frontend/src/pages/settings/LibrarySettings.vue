<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import AddLibraryDialog from '../../components/AddLibraryDialog.vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import {
  api,
  cancelCurrentScan,
  deleteLibrary,
  isAdmin,
  libraries,
  loadAdminData,
  loadLibraries,
  scanOperation,
  loadVirtualFolders,
  scan,
  state,
  totalLibraryItems,
  virtualFolders
} from '../../store/app';
import type { VirtualFolderInfo } from '../../api/emby';

onMounted(async () => {
  if (isAdmin.value) {
    await Promise.all([loadAdminData(), loadLibraries(), loadVirtualFolders()]);
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

async function remove(folder: VirtualFolderInfo) {
  const confirmed = window.confirm(`删除媒体库"${folder.Name}"？媒体文件不会被删除。`);
  if (!confirmed) {
    return;
  }
  await deleteLibrary(folder);
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
            <p class="text-muted text-xs">当前媒体库总片源</p>
            <p class="text-highlighted mt-1 text-2xl font-semibold">{{ totalLibraryItems }}</p>
          </div>
          <div class="rounded-lg border border-default p-3">
            <p class="text-muted text-xs">扫描进度</p>
            <p class="text-highlighted mt-1 text-2xl font-semibold">{{ scanProgressText }}</p>
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
        <UCard v-for="folder in virtualFolders" :key="folder.ItemId">
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
  </SettingsLayout>
</template>
