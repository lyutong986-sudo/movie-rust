<script setup lang="ts">
import { computed, reactive } from 'vue';
import { api, createLibrary, defaultLibraryOptions, state } from '../store/app';
import type { CreateLibraryPayload, FileSystemEntryInfo, LibraryOptions } from '../api/emby';

const emit = defineEmits<{
  close: [];
}>();

const form = reactive({
  name: '电影',
  collectionType: 'movies',
  paths: [] as string[],
  preferredMetadataLanguage: state.metadataLanguage || 'zh',
  metadataCountryCode: state.metadataCountry || 'CN',
  seasonZeroDisplayName: 'Specials',
  automaticRefreshIntervalDays: 0,
  enabled: true,
  enablePhotos: true,
  enableRealtimeMonitor: false,
  saveLocalMetadata: true,
  enableChapterImageExtraction: false,
  extractChapterImagesDuringLibraryScan: false,
  enableAutomaticSeriesGrouping: true,
  enableEmbeddedTitles: false,
  enableEmbeddedEpisodeInfos: true
});

const browser = reactive({
  open: false,
  loading: false,
  error: '',
  currentPath: '',
  entries: [] as FileSystemEntryInfo[]
});

const cleanPaths = computed(() => form.paths.map((path) => path.trim()).filter(Boolean));
const canUseCurrentPath = computed(() => Boolean(browser.currentPath.trim()));

async function openBrowser() {
  browser.open = true;
  browser.error = '';
  browser.currentPath = '';
  await loadDrives();
}

async function loadDrives() {
  browser.loading = true;
  browser.error = '';
  try {
    browser.entries = await api.environmentDrives();
    browser.currentPath = '';
  } catch (error) {
    browser.error = error instanceof Error ? error.message : '无法读取服务器目录';
  } finally {
    browser.loading = false;
  }
}

async function browse(path: string) {
  browser.loading = true;
  browser.error = '';
  try {
    browser.entries = await api.directoryContents(path, false, true);
    browser.currentPath = path;
  } catch (error) {
    browser.error = error instanceof Error ? error.message : '无法打开目录';
  } finally {
    browser.loading = false;
  }
}

async function goUp() {
  if (!browser.currentPath) {
    return;
  }

  browser.loading = true;
  browser.error = '';
  try {
    const parent = await api.parentPath(browser.currentPath);
    if (parent) {
      await browse(parent);
    } else {
      await loadDrives();
    }
  } catch (error) {
    browser.error = error instanceof Error ? error.message : '无法返回上级目录';
    browser.loading = false;
  }
}

function useCurrentPath() {
  addSelectedPath(browser.currentPath);
}

function addSelectedPath(path: string) {
  const value = path.trim();
  if (!value) {
    return;
  }
  if (!form.paths.some((item) => item.toLowerCase() === value.toLowerCase())) {
    form.paths.push(value);
  }
  browser.open = false;
}

function removePath(index: number) {
  form.paths.splice(index, 1);
}

function options(): LibraryOptions {
  return {
    ...defaultLibraryOptions(cleanPaths.value),
    Enabled: form.enabled,
    EnablePhotos: form.enablePhotos,
    EnableRealtimeMonitor: form.enableRealtimeMonitor,
    SaveLocalMetadata: form.saveLocalMetadata,
    EnableChapterImageExtraction: form.enableChapterImageExtraction,
    ExtractChapterImagesDuringLibraryScan: form.extractChapterImagesDuringLibraryScan,
    EnableAutomaticSeriesGrouping: form.enableAutomaticSeriesGrouping,
    EnableEmbeddedTitles: form.enableEmbeddedTitles,
    EnableEmbeddedEpisodeInfos: form.enableEmbeddedEpisodeInfos,
    AutomaticRefreshIntervalDays: Number(form.automaticRefreshIntervalDays) || 0,
    PreferredMetadataLanguage: form.preferredMetadataLanguage || 'zh',
    MetadataCountryCode: form.metadataCountryCode || 'CN',
    SeasonZeroDisplayName: form.seasonZeroDisplayName || 'Specials',
    PathInfos: cleanPaths.value.map((path) => ({ Path: path }))
  };
}

async function submit() {
  const payload: CreateLibraryPayload = {
    Name: form.name.trim(),
    CollectionType: form.collectionType,
    Path: cleanPaths.value[0] || '',
    Paths: cleanPaths.value,
    LibraryOptions: options()
  };

  await createLibrary(payload);
  if (!state.error) {
    emit('close');
  }
}
</script>

<template>
  <div class="dialog-backdrop" @click.self="emit('close')">
    <section class="library-dialog">
      <button class="close" type="button" aria-label="关闭" @click="emit('close')">×</button>

      <form class="form-stack settings-form library-form" @submit.prevent="submit">
        <div>
          <p>媒体库</p>
          <h2>添加媒体库</h2>
        </div>

        <div class="form-grid two">
          <label>
            名称
            <input v-model="form.name" required placeholder="电影" />
          </label>

          <label>
            内容类型
            <select v-model="form.collectionType">
              <option value="movies">电影</option>
              <option value="tvshows">电视剧</option>
              <option value="music">音乐</option>
              <option value="homevideos">家庭视频</option>
              <option value="mixed">混合内容</option>
            </select>
          </label>
        </div>

        <div class="path-editor">
          <div class="section-heading compact">
            <h3>文件夹</h3>
            <button class="secondary" type="button" @click="openBrowser">选择文件夹</button>
          </div>

          <div v-if="cleanPaths.length" class="selected-path-list">
            <div v-for="(path, index) in cleanPaths" :key="path" class="path-row selected">
              <span>{{ path }}</span>
              <button class="secondary icon-button" type="button" title="移除路径" @click="removePath(index)">×</button>
            </div>
          </div>
          <div v-else class="path-empty">还没有选择媒体文件夹</div>
        </div>

        <div class="form-grid three">
          <label>
            元数据语言
            <input v-model="form.preferredMetadataLanguage" placeholder="zh" />
          </label>

          <label>
            国家/地区
            <input v-model="form.metadataCountryCode" placeholder="CN" />
          </label>

          <label>
            特别篇名称
            <input v-model="form.seasonZeroDisplayName" placeholder="Specials" />
          </label>
        </div>

        <label>
          自动刷新间隔（天）
          <input v-model.number="form.automaticRefreshIntervalDays" min="0" type="number" />
        </label>

        <div class="switch-grid">
          <label><input v-model="form.enabled" type="checkbox" />启用媒体库</label>
          <label><input v-model="form.enableRealtimeMonitor" type="checkbox" />实时监控</label>
          <label><input v-model="form.saveLocalMetadata" type="checkbox" />保存 NFO 元数据</label>
          <label><input v-model="form.enablePhotos" type="checkbox" />导入图片</label>
          <label><input v-model="form.enableChapterImageExtraction" type="checkbox" />提取章节图片</label>
          <label><input v-model="form.extractChapterImagesDuringLibraryScan" type="checkbox" />扫描时提取章节图片</label>
          <label><input v-model="form.enableAutomaticSeriesGrouping" type="checkbox" />电视剧自动分组</label>
          <label><input v-model="form.enableEmbeddedTitles" type="checkbox" />读取内嵌标题</label>
          <label><input v-model="form.enableEmbeddedEpisodeInfos" type="checkbox" />读取内嵌集信息</label>
        </div>

        <div class="button-row">
          <button class="secondary" type="button" @click="emit('close')">取消</button>
          <button :disabled="state.busy || !cleanPaths.length" type="submit">创建</button>
        </div>
      </form>
    </section>

    <section v-if="browser.open" class="folder-picker-backdrop" @click.self="browser.open = false">
      <div class="folder-picker">
        <div class="folder-picker-head">
          <div>
            <p>服务器目录</p>
            <h3>{{ browser.currentPath || '选择驱动器' }}</h3>
          </div>
          <button class="close" type="button" aria-label="关闭" @click="browser.open = false">×</button>
        </div>

        <div class="folder-picker-toolbar">
          <button class="secondary" type="button" :disabled="browser.loading" @click="loadDrives">驱动器</button>
          <button class="secondary" type="button" :disabled="browser.loading || !browser.currentPath" @click="goUp">上一级</button>
          <button type="button" :disabled="browser.loading || !canUseCurrentPath" @click="useCurrentPath">使用此文件夹</button>
        </div>

        <p v-if="browser.error" class="form-error">{{ browser.error }}</p>
        <div class="folder-list" :aria-busy="browser.loading">
          <button
            v-for="entry in browser.entries"
            :key="entry.Path"
            class="folder-entry"
            type="button"
            :disabled="browser.loading"
            @click="browse(entry.Path)"
          >
            <span class="folder-entry-icon">▸</span>
            <span>{{ entry.Name }}</span>
            <small>{{ entry.Path }}</small>
          </button>
          <div v-if="browser.loading" class="folder-empty">正在读取目录...</div>
          <div v-else-if="!browser.entries.length" class="folder-empty">此目录没有可选择的子文件夹</div>
        </div>
      </div>
    </section>
  </div>
</template>
