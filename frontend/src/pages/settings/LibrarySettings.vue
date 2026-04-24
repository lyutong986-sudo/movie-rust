<script setup lang="ts">
import { onMounted } from 'vue';
import AddLibraryDialog from '../../components/AddLibraryDialog.vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import {
  deleteLibrary,
  isAdmin,
  loadAdminData,
  loadLibraries,
  loadVirtualFolders,
  scan,
  state,
  virtualFolders
} from '../../store/app';
import type { VirtualFolderInfo } from '../../api/emby';

onMounted(async () => {
  if (isAdmin.value) {
    await Promise.all([loadAdminData(), loadLibraries(), loadVirtualFolders()]);
  }
});

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
            @click="scan"
          >
            扫描所有媒体库
          </UButton>
        </div>
      </div>

      <UAlert v-if="state.error" color="error" icon="i-lucide-triangle-alert" :description="state.error" />
      <UAlert v-else-if="state.message" color="success" icon="i-lucide-check" :description="state.message" />

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
              <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-ccw" :disabled="state.busy" @click="scan">
                扫描
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
