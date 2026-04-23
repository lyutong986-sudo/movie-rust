<script setup lang="ts">
import { onMounted } from 'vue';
import AddLibraryDialog from '../../components/AddLibraryDialog.vue';
import SettingsNav from '../../components/SettingsNav.vue';
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

function edit(folder: VirtualFolderInfo) {
  state.editingLibrary = folder;
}

async function remove(folder: VirtualFolderInfo) {
  const confirmed = window.confirm(`删除媒体库“${folder.Name}”？媒体文件不会被删除。`);
  if (!confirmed) {
    return;
  }

  await deleteLibrary(folder);
}
</script>

<template>
  <section class="settings-shell">
    <SettingsNav />

    <div class="settings-content">
      <div v-if="!isAdmin" class="empty">
        <p>媒体库</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能创建、编辑或删除媒体库。</p>
      </div>

      <div v-else class="settings-page">
        <div class="settings-heading-row">
          <div>
            <p>媒体库</p>
            <h2>媒体库管理</h2>
          </div>
          <div class="button-row">
            <button type="button" @click="state.showAddLibrary = true">添加媒体库</button>
            <button class="secondary" :disabled="state.busy" type="button" @click="scan">扫描所有媒体库</button>
          </div>
        </div>

        <div v-if="virtualFolders.length" class="library-admin-list">
          <article v-for="folder in virtualFolders" :key="folder.ItemId" class="library-admin-card">
            <div class="library-admin-card-main">
              <span class="library-type-pill">{{ collectionLabel(folder.CollectionType) }}</span>
              <div>
                <h3>{{ folder.Name }}</h3>
                <p>{{ folder.Locations.join(' · ') }}</p>
              </div>
            </div>

            <div class="library-option-grid">
              <span :class="{ enabled: folder.LibraryOptions.Enabled }">启用</span>
              <span :class="{ enabled: folder.LibraryOptions.EnableRealtimeMonitor }">实时监控</span>
              <span :class="{ enabled: folder.LibraryOptions.EnableInternetProviders }">互联网元数据</span>
              <span :class="{ enabled: folder.LibraryOptions.DownloadImagesInAdvance }">预下载图片</span>
              <span :class="{ enabled: folder.LibraryOptions.SaveLocalMetadata }">保存 NFO</span>
              <span :class="{ enabled: folder.LibraryOptions.ImportMissingEpisodes }">缺失剧集</span>
              <span :class="{ enabled: folder.LibraryOptions.ImportCollections }">电影合集</span>
              <span :class="{ enabled: !folder.LibraryOptions.ExcludeFromSearch }">参与搜索</span>
              <span :class="{ enabled: folder.LibraryOptions.EnableChapterImageExtraction }">章节图片</span>
              <span :class="{ enabled: folder.LibraryOptions.EnableAutomaticSeriesGrouping }">剧集自动分组</span>
              <span>{{ folder.LibraryOptions.PreferredMetadataLanguage || 'zh' }}-{{ folder.LibraryOptions.MetadataCountryCode || 'CN' }}</span>
            </div>

            <div class="button-row">
              <button class="secondary" type="button" :disabled="state.busy" @click="edit(folder)">编辑</button>
              <button class="secondary" type="button" :disabled="state.busy" @click="scan">扫描</button>
              <button class="danger" type="button" :disabled="state.busy" @click="remove(folder)">删除</button>
            </div>
          </article>
        </div>

        <div v-else class="empty">
          <p>媒体库</p>
          <h2>还没有媒体库</h2>
          <p>添加电影或电视剧目录后，首页和播放器就会按 Emby 的媒体库结构加载内容。</p>
        </div>
      </div>
    </div>

    <AddLibraryDialog
      v-if="state.showAddLibrary"
      @close="state.showAddLibrary = false"
    />
    <AddLibraryDialog
      v-if="state.editingLibrary"
      :folder="state.editingLibrary"
      @close="state.editingLibrary = null"
    />
  </section>
</template>
