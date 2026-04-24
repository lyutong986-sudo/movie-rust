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
  open: boolean;
  folder?: VirtualFolderInfo | null;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
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

const COLLECTION_TYPES = [
  { value: 'movies', label: '电影' },
  { value: 'tvshows', label: '电视剧' }
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

function buildOptions(): LibraryOptions {
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
      await api.updateLibraryOptions(props.folder.ItemId, buildOptions());
      await Promise.all([
        loadLibraries(),
        loadVirtualFolders(),
        loadRecentlyAddedTitles(),
        loadLatestByLibrary()
      ]);
      state.message = '媒体库设置已保存';
      close();
      return;
    }

    const payload: CreateLibraryPayload = {
      Name: form.name.trim(),
      CollectionType: form.collectionType,
      Path: cleanPaths.value[0] || '',
      Paths: cleanPaths.value,
      LibraryOptions: buildOptions()
    };

    await createLibrary(payload);
    if (!state.error) {
      close();
    }
  } catch (error) {
    state.error = error instanceof Error ? error.message : '保存媒体库失败';
  } finally {
    state.busy = false;
  }
}

function close() {
  emit('update:open', false);
  emit('close');
}
</script>

<template>
  <UModal
    :open="props.open"
    :title="isEditing ? '编辑媒体库' : '添加媒体库'"
    description="选择文件夹、调整媒体库的元数据与扫描行为。"
    :ui="{ content: 'sm:max-w-3xl' }"
    @update:open="emit('update:open', $event)"
  >
    <template #body>
      <form class="space-y-6" @submit.prevent="submit">
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="名称" required>
            <UInput v-model="form.name" placeholder="电影" class="w-full" />
          </UFormField>
          <UFormField label="内容类型">
            <USelect
              v-model="form.collectionType"
              :items="COLLECTION_TYPES"
              :disabled="isEditing"
              class="w-full"
            />
          </UFormField>
        </div>

        <div class="space-y-3">
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">文件夹</h3>
            <UButton
              color="neutral"
              variant="soft"
              icon="i-lucide-folder-plus"
              size="xs"
              @click="openBrowser"
            >
              选择文件夹
            </UButton>
          </div>

          <div v-if="cleanPaths.length" class="flex flex-col gap-2">
            <div
              v-for="(path, index) in cleanPaths"
              :key="path"
              class="flex items-center gap-2 rounded-lg border border-default bg-elevated/30 px-3 py-2 text-sm"
            >
              <UIcon name="i-lucide-folder" class="size-4 text-primary" />
              <span class="text-default flex-1 truncate font-mono text-xs">{{ path }}</span>
              <UButton
                color="neutral"
                variant="ghost"
                size="xs"
                icon="i-lucide-x"
                square
                @click="removePath(index)"
              />
            </div>
          </div>
          <p
            v-else
            class="rounded-lg border border-dashed border-default px-3 py-4 text-center text-muted text-sm"
          >
            还没有选择媒体文件夹
          </p>
        </div>

        <section v-if="showInternetMetadata" class="space-y-3">
          <h3 class="text-highlighted text-sm font-semibold">元数据</h3>
          <div class="grid gap-3 sm:grid-cols-2">
            <USwitch v-model="form.enableInternetProviders" label="下载互联网元数据" />
            <USwitch v-model="form.downloadImagesInAdvance" label="扫描时预下载图片" />
          </div>
          <div class="grid gap-3 sm:grid-cols-3">
            <UFormField label="元数据语言">
              <USelect v-model="form.preferredMetadataLanguage" :items="selectedCultureOptions" class="w-full" />
            </UFormField>
            <UFormField label="图片语言">
              <USelect v-model="form.preferredImageLanguage" :items="selectedCultureOptions" class="w-full" />
            </UFormField>
            <UFormField label="国家/地区">
              <USelect v-model="form.metadataCountryCode" :items="COUNTRY_OPTIONS" class="w-full" />
            </UFormField>
            <UFormField v-if="showTvSettings" label="特别篇名称">
              <UInput v-model="form.seasonZeroDisplayName" placeholder="Specials" class="w-full" />
            </UFormField>
          </div>
        </section>

        <section class="space-y-3">
          <h3 class="text-highlighted text-sm font-semibold">扫描与读取</h3>
          <UFormField label="自动刷新间隔（天）">
            <UInput v-model.number="form.automaticRefreshIntervalDays" type="number" min="0" />
          </UFormField>
          <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <USwitch v-model="form.enabled" label="启用媒体库" />
            <USwitch v-model="form.enableRealtimeMonitor" label="实时监控" />
            <USwitch v-model="form.enablePhotos" label="导入图片" />
            <USwitch v-if="showSaveLocal" v-model="form.saveLocalMetadata" label="保存图片和 NFO 到媒体目录" />
            <USwitch v-model="form.saveMetadataHidden" label="元数据文件保存为隐藏" />
            <USwitch v-model="form.ignoreHiddenFiles" label="忽略隐藏文件" />
            <USwitch v-model="form.excludeFromSearch" label="从全局搜索排除" />
            <USwitch v-model="form.mergeTopLevelFolders" label="合并顶层同名文件夹" />
            <USwitch v-model="form.enableEmbeddedTitles" label="读取内嵌标题" />
            <USwitch v-model="form.enableEmbeddedEpisodeInfos" label="读取内嵌集信息" />
            <USwitch v-model="form.enableMultiPartItems" label="识别多分段媒体" />
            <USwitch v-model="form.enableMultiVersionByFiles" label="按文件名识别多版本" />
            <USwitch v-model="form.enableMultiVersionByMetadata" label="按元数据合并多版本" />
          </div>
          <UFormField label="占位条目元数据刷新间隔（天）">
            <UInput v-model.number="form.placeholderMetadataRefreshIntervalDays" type="number" min="0" />
          </UFormField>
        </section>

        <section v-if="showTvSettings" class="space-y-3">
          <h3 class="text-highlighted text-sm font-semibold">电视剧</h3>
          <div class="grid gap-3 sm:grid-cols-2">
            <USwitch v-model="form.importMissingEpisodes" label="导入缺失剧集占位" />
            <USwitch v-model="form.enableAutomaticSeriesGrouping" label="自动合并同名剧集" />
          </div>
        </section>

        <section v-if="showMovieSettings" class="space-y-3">
          <h3 class="text-highlighted text-sm font-semibold">电影集合</h3>
          <div class="grid gap-3 sm:grid-cols-2">
            <USwitch v-model="form.importCollections" label="导入电影合集" />
            <UFormField label="自动合集最少影片数">
              <UInput v-model.number="form.minCollectionItems" type="number" min="2" />
            </UFormField>
          </div>
        </section>

        <section v-if="showChapterSettings" class="space-y-3">
          <h3 class="text-highlighted text-sm font-semibold">章节图片</h3>
          <div class="grid gap-3 sm:grid-cols-2">
            <USwitch v-model="form.enableChapterImageExtraction" label="提取章节图片" />
            <USwitch v-model="form.extractChapterImagesDuringLibraryScan" label="扫描时提取章节图片" />
          </div>
        </section>

        <div class="flex justify-end gap-2 pt-2">
          <UButton color="neutral" variant="subtle" @click="close">取消</UButton>
          <UButton
            type="submit"
            :loading="state.busy"
            :disabled="!cleanPaths.length"
            icon="i-lucide-save"
          >
            {{ isEditing ? '保存' : '创建' }}
          </UButton>
        </div>
      </form>
    </template>
  </UModal>

  <!-- 文件夹选择器 -->
  <UModal
    :open="browser.open"
    title="服务器目录"
    :description="browser.currentPath || '选择驱动器'"
    :ui="{ content: 'sm:max-w-2xl' }"
    @update:open="browser.open = $event"
  >
    <template #body>
      <div class="space-y-3">
        <div class="flex flex-wrap gap-2">
          <UButton
            color="neutral"
            variant="soft"
            icon="i-lucide-hard-drive"
            :disabled="browser.loading"
            @click="loadDrives"
          >
            驱动器
          </UButton>
          <UButton
            color="neutral"
            variant="soft"
            icon="i-lucide-corner-left-up"
            :disabled="browser.loading || !browser.currentPath"
            @click="goUp"
          >
            上一级
          </UButton>
          <UButton
            icon="i-lucide-check"
            :disabled="browser.loading || !canUseCurrentPath"
            class="ms-auto"
            @click="useCurrentPath"
          >
            使用此文件夹
          </UButton>
        </div>

        <UAlert v-if="browser.error" color="error" variant="subtle" :title="browser.error" />

        <div
          class="max-h-[50vh] overflow-y-auto rounded-lg border border-default"
          :aria-busy="browser.loading"
        >
          <button
            v-for="entry in browser.entries"
            :key="entry.Path"
            type="button"
            :disabled="browser.loading"
            class="flex w-full items-center gap-3 border-b border-default px-3 py-2 text-start text-sm last:border-b-0 hover:bg-elevated disabled:opacity-60"
            @click="browse(entry.Path)"
          >
            <UIcon name="i-lucide-folder" class="size-4 text-primary" />
            <span class="text-highlighted font-medium">{{ entry.Name }}</span>
            <span class="text-muted ms-auto truncate font-mono text-xs">{{ entry.Path }}</span>
          </button>
          <div v-if="browser.loading" class="px-3 py-6 text-center text-muted text-sm">
            正在读取目录…
          </div>
          <div
            v-else-if="!browser.entries.length"
            class="px-3 py-6 text-center text-muted text-sm"
          >
            此目录没有可选择的子文件夹
          </div>
        </div>
      </div>
    </template>
  </UModal>
</template>
