<script setup lang="ts">
import { computed, reactive, watch } from 'vue';
import {
  api,
  createLibrary,
  defaultLibraryOptions,
  loadLatestByLibrary,
  loadLibraries,
  loadRecentlyAddedTitles,
  loadVirtualFolders,
  metadataCultures,
  state
} from '../store/app';
import type {
  CreateLibraryPayload,
  FileSystemEntryInfo,
  LibraryOptions,
  VirtualFolderInfo
} from '../api/emby';

const props = defineProps<{
  folder?: VirtualFolderInfo | null;
}>();

const emit = defineEmits<{
  close: [];
}>();

const COUNTRY_OPTIONS = [
  { value: 'CN', label: '中国' },
  { value: 'US', label: '美国' },
  { value: 'JP', label: '日本' },
  { value: 'KR', label: '韩国' },
  { value: 'GB', label: '英国' },
  { value: 'FR', label: '法国' },
  { value: 'DE', label: '德国' },
  { value: 'HK', label: '中国香港' },
  { value: 'TW', label: '中国台湾' }
];

const form = reactive({
  name: '电影',
  collectionType: 'movies',
  paths: [] as string[],
  preferredMetadataLanguage: state.metadataLanguage || 'zh',
  preferredImageLanguage: state.metadataLanguage || 'zh',
  metadataCountryCode: state.metadataCountry || 'CN',
  seasonZeroDisplayName: 'Specials',
  automaticRefreshIntervalDays: 0,
  placeholderMetadataRefreshIntervalDays: 0,
  enabled: true,
  enablePhotos: true,
  enableInternetProviders: true,
  downloadImagesInAdvance: false,
  enableRealtimeMonitor: false,
  excludeFromSearch: false,
  ignoreHiddenFiles: true,
  saveLocalMetadata: true,
  saveMetadataHidden: false,
  mergeTopLevelFolders: false,
  importMissingEpisodes: false,
  importCollections: true,
  minCollectionItems: 2,
  enableChapterImageExtraction: false,
  extractChapterImagesDuringLibraryScan: false,
  enableAutomaticSeriesGrouping: true,
  enableEmbeddedTitles: false,
  enableEmbeddedEpisodeInfos: true,
  enableMultiVersionByFiles: true,
  enableMultiVersionByMetadata: false,
  enableMultiPartItems: true
});

const browser = reactive({
  open: false,
  loading: false,
  error: '',
  currentPath: '',
  entries: [] as FileSystemEntryInfo[]
});

const isEditing = computed(() => Boolean(props.folder));
const selectedCultureOptions = computed(() => {
  if (metadataCultures.value.length) {
    return metadataCultures.value.map((culture) => ({
      value: culture.TwoLetterISOLanguageName,
      label: culture.DisplayName
    }));
  }
  return [
    { value: 'zh', label: '中文' },
    { value: 'en', label: 'English' }
  ];
});
const cleanPaths = computed(() => form.paths.map((path) => path.trim()).filter(Boolean));
const canUseCurrentPath = computed(() => Boolean(browser.currentPath.trim()));
const contentType = computed(() => form.collectionType || 'mixed');
const showInternetMetadata = computed(
  () => !['homevideos', 'photos'].includes(contentType.value)
);
const showSaveLocal = computed(() => contentType.value !== 'photos');
const showChapterSettings = computed(() =>
  ['tvshows', 'movies', 'homevideos', 'musicvideos', 'mixed'].includes(contentType.value)
);
const showTvSettings = computed(() => contentType.value === 'tvshows');
const showMovieSettings = computed(() => contentType.value === 'movies');

watch(
  () => props.folder,
  (folder) => {
    const options = folder?.LibraryOptions;
    form.name = folder?.Name || '电影';
    form.collectionType = folder?.CollectionType || 'movies';
    form.paths = options?.PathInfos?.length
      ? options.PathInfos.map((item) => item.Path)
      : folder?.Locations?.slice() || [];
    form.preferredMetadataLanguage =
      options?.PreferredMetadataLanguage || state.metadataLanguage || 'zh';
    form.preferredImageLanguage =
      options?.PreferredImageLanguage || options?.PreferredMetadataLanguage || state.metadataLanguage || 'zh';
    form.metadataCountryCode = options?.MetadataCountryCode || state.metadataCountry || 'CN';
    form.seasonZeroDisplayName = options?.SeasonZeroDisplayName || 'Specials';
    form.automaticRefreshIntervalDays = options?.AutomaticRefreshIntervalDays || 0;
    form.placeholderMetadataRefreshIntervalDays =
      options?.PlaceholderMetadataRefreshIntervalDays || 0;
    form.enabled = options?.Enabled ?? true;
    form.enablePhotos = options?.EnablePhotos ?? true;
    form.enableInternetProviders = options?.EnableInternetProviders ?? true;
    form.downloadImagesInAdvance = options?.DownloadImagesInAdvance ?? false;
    form.enableRealtimeMonitor = options?.EnableRealtimeMonitor ?? false;
    form.excludeFromSearch = options?.ExcludeFromSearch ?? false;
    form.ignoreHiddenFiles = options?.IgnoreHiddenFiles ?? true;
    form.saveLocalMetadata = options?.SaveLocalMetadata ?? true;
    form.saveMetadataHidden = options?.SaveMetadataHidden ?? false;
    form.mergeTopLevelFolders = options?.MergeTopLevelFolders ?? false;
    form.importMissingEpisodes = options?.ImportMissingEpisodes ?? false;
    form.importCollections = options?.ImportCollections ?? true;
    form.minCollectionItems = options?.MinCollectionItems || 2;
    form.enableChapterImageExtraction = options?.EnableChapterImageExtraction ?? false;
    form.extractChapterImagesDuringLibraryScan =
      options?.ExtractChapterImagesDuringLibraryScan ?? false;
    form.enableAutomaticSeriesGrouping = options?.EnableAutomaticSeriesGrouping ?? true;
    form.enableEmbeddedTitles = options?.EnableEmbeddedTitles ?? false;
    form.enableEmbeddedEpisodeInfos = options?.EnableEmbeddedEpisodeInfos ?? true;
    form.enableMultiVersionByFiles = options?.EnableMultiVersionByFiles ?? true;
    form.enableMultiVersionByMetadata = options?.EnableMultiVersionByMetadata ?? false;
    form.enableMultiPartItems = options?.EnableMultiPartItems ?? true;
  },
  { immediate: true }
);

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
    EnableInternetProviders: form.enableInternetProviders,
    DownloadImagesInAdvance: form.downloadImagesInAdvance,
    EnableRealtimeMonitor: form.enableRealtimeMonitor,
    ExcludeFromSearch: form.excludeFromSearch,
    IgnoreHiddenFiles: form.ignoreHiddenFiles,
    SaveLocalMetadata: form.saveLocalMetadata,
    SaveMetadataHidden: form.saveMetadataHidden,
    MergeTopLevelFolders: form.mergeTopLevelFolders,
    PlaceholderMetadataRefreshIntervalDays:
      Number(form.placeholderMetadataRefreshIntervalDays) || 0,
    ImportMissingEpisodes: form.importMissingEpisodes,
    ImportCollections: form.importCollections,
    MinCollectionItems: Number(form.minCollectionItems) || 2,
    EnableChapterImageExtraction: form.enableChapterImageExtraction,
    ExtractChapterImagesDuringLibraryScan: form.extractChapterImagesDuringLibraryScan,
    EnableAutomaticSeriesGrouping: form.enableAutomaticSeriesGrouping,
    EnableEmbeddedTitles: form.enableEmbeddedTitles,
    EnableEmbeddedEpisodeInfos: form.enableEmbeddedEpisodeInfos,
    EnableMultiVersionByFiles: form.enableMultiVersionByFiles,
    EnableMultiVersionByMetadata: form.enableMultiVersionByMetadata,
    EnableMultiPartItems: form.enableMultiPartItems,
    AutomaticRefreshIntervalDays: Number(form.automaticRefreshIntervalDays) || 0,
    PreferredMetadataLanguage: form.preferredMetadataLanguage || 'zh',
    PreferredImageLanguage: form.preferredImageLanguage || form.preferredMetadataLanguage || 'zh',
    MetadataCountryCode: form.metadataCountryCode || 'CN',
    SeasonZeroDisplayName: form.seasonZeroDisplayName || 'Specials',
    PathInfos: cleanPaths.value.map((path) => ({ Path: path }))
  };
}

async function submit() {
  state.busy = true;
  state.error = '';
  try {
    if (isEditing.value && props.folder) {
      const trimmedName = form.name.trim();
      if (trimmedName && trimmedName !== props.folder.Name) {
        await api.renameVirtualFolder(props.folder.Name, trimmedName);
      }
      await api.updateLibraryOptions(props.folder.ItemId, options());
      await Promise.all([
        loadLibraries(),
        loadVirtualFolders(),
        loadRecentlyAddedTitles(),
        loadLatestByLibrary()
      ]);
      state.message = '媒体库设置已保存';
      emit('close');
      return;
    }

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
  } catch (error) {
    state.error = error instanceof Error ? error.message : '保存媒体库失败';
  } finally {
    state.busy = false;
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
          <h2>{{ isEditing ? '编辑媒体库' : '添加媒体库' }}</h2>
        </div>

        <div class="form-grid two">
          <label>
            名称
            <input v-model="form.name" required placeholder="电影" />
          </label>

          <label>
            内容类型
            <select v-model="form.collectionType" :disabled="isEditing">
              <option value="movies">电影</option>
              <option value="tvshows">电视剧</option>
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

        <div class="library-option-section" v-if="showInternetMetadata">
          <div class="section-heading compact">
            <h3>元数据</h3>
          </div>
          <div class="switch-grid">
            <label><input v-model="form.enableInternetProviders" type="checkbox" />下载互联网元数据</label>
            <label><input v-model="form.downloadImagesInAdvance" type="checkbox" />扫描时预下载图片</label>
          </div>
          <div class="form-grid three">
            <label>
              元数据语言
              <select v-model="form.preferredMetadataLanguage">
                <option v-for="culture in selectedCultureOptions" :key="culture.value" :value="culture.value">
                  {{ culture.label }}
                </option>
              </select>
            </label>

            <label>
              图片语言
              <select v-model="form.preferredImageLanguage">
                <option v-for="culture in selectedCultureOptions" :key="`image-${culture.value}`" :value="culture.value">
                  {{ culture.label }}
                </option>
              </select>
            </label>

            <label>
              国家/地区
              <select v-model="form.metadataCountryCode">
                <option v-for="country in COUNTRY_OPTIONS" :key="country.value" :value="country.value">
                  {{ country.label }}
                </option>
              </select>
            </label>

            <label v-if="showTvSettings">
              特别篇名称
              <input v-model="form.seasonZeroDisplayName" placeholder="Specials" />
            </label>
          </div>
        </div>

        <div class="library-option-section">
          <div class="section-heading compact">
            <h3>扫描与读取</h3>
          </div>
          <label>
            自动刷新间隔（天）
            <input v-model.number="form.automaticRefreshIntervalDays" min="0" type="number" />
          </label>
          <div class="switch-grid">
            <label><input v-model="form.enabled" type="checkbox" />启用媒体库</label>
            <label><input v-model="form.enableRealtimeMonitor" type="checkbox" />实时监控</label>
            <label><input v-model="form.enablePhotos" type="checkbox" />导入图片</label>
            <label v-if="showSaveLocal"><input v-model="form.saveLocalMetadata" type="checkbox" />保存图片和 NFO 到媒体目录</label>
            <label><input v-model="form.saveMetadataHidden" type="checkbox" />把元数据文件保存为隐藏文件</label>
            <label><input v-model="form.ignoreHiddenFiles" type="checkbox" />忽略隐藏文件和文件夹</label>
            <label><input v-model="form.excludeFromSearch" type="checkbox" />从全局搜索中排除该媒体库</label>
            <label><input v-model="form.mergeTopLevelFolders" type="checkbox" />合并顶层同名文件夹</label>
            <label><input v-model="form.enableEmbeddedTitles" type="checkbox" />读取内嵌标题</label>
            <label><input v-model="form.enableEmbeddedEpisodeInfos" type="checkbox" />读取内嵌集信息</label>
            <label><input v-model="form.enableMultiPartItems" type="checkbox" />识别多分段媒体</label>
            <label><input v-model="form.enableMultiVersionByFiles" type="checkbox" />按文件名识别多版本</label>
            <label><input v-model="form.enableMultiVersionByMetadata" type="checkbox" />按元数据合并多版本</label>
          </div>
          <div class="form-grid two">
            <label>
              占位条目元数据刷新间隔（天）
              <input v-model.number="form.placeholderMetadataRefreshIntervalDays" min="0" type="number" />
            </label>
          </div>
        </div>

        <div v-if="showTvSettings" class="library-option-section">
          <div class="section-heading compact">
            <h3>电视剧</h3>
          </div>
          <div class="switch-grid">
            <label><input v-model="form.importMissingEpisodes" type="checkbox" />导入缺失剧集占位条目</label>
            <label><input v-model="form.enableAutomaticSeriesGrouping" type="checkbox" />自动合并同名剧集</label>
          </div>
        </div>

        <div v-if="showMovieSettings" class="library-option-section">
          <div class="section-heading compact">
            <h3>电影集合</h3>
          </div>
          <div class="switch-grid">
            <label><input v-model="form.importCollections" type="checkbox" />导入电影合集信息</label>
          </div>
          <div class="form-grid two">
            <label>
              自动合集最少影片数
              <input v-model.number="form.minCollectionItems" min="2" type="number" />
            </label>
          </div>
        </div>

        <div v-if="showChapterSettings" class="library-option-section">
          <div class="section-heading compact">
            <h3>章节图片</h3>
          </div>
          <div class="switch-grid">
            <label><input v-model="form.enableChapterImageExtraction" type="checkbox" />提取章节图片</label>
            <label><input v-model="form.extractChapterImagesDuringLibraryScan" type="checkbox" />扫描时提取章节图片</label>
          </div>
        </div>

        <div class="button-row">
          <button class="secondary" type="button" @click="emit('close')">取消</button>
          <button :disabled="state.busy || !cleanPaths.length" type="submit">
            {{ isEditing ? '保存' : '创建' }}
          </button>
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
