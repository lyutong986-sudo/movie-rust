<template>
  <SettingsPage>
    <template #title>
      {{ t('libraries') }}
    </template>
    <template #actions>
      <div class="uno-flex uno-gap-2">
        <VBtn
          variant="outlined"
          :loading="loading"
          @click="refreshAllLibraries">
          {{ t('refresh') }}
        </VBtn>
        <VBtn
          color="primary"
          variant="elevated"
          @click="createDialog = true">
          {{ t('add') }} {{ t('libraries') }}
        </VBtn>
      </div>
    </template>
    <template #content>
      <VCol cols="12">
        <VAlert
          v-if="errorMessage"
          type="error"
          variant="tonal"
          class="uno-mb-4">
          {{ errorMessage }}
        </VAlert>

        <VRow class="uno-mb-2">
          <VCol
            cols="12"
            md="4">
            <VCard variant="tonal">
              <VCardTitle>{{ t('libraries') }}</VCardTitle>
              <VCardText>{{ libraries.length }}</VCardText>
            </VCard>
          </VCol>
          <VCol
            cols="12"
            md="8">
            <VCard variant="tonal">
              <VCardTitle>Selectable media folders</VCardTitle>
              <VCardText>{{ selectableFolders.length }}</VCardText>
            </VCard>
          </VCol>
        </VRow>

        <VExpansionPanels v-if="libraries.length">
          <VExpansionPanel
            v-for="library in libraries"
            :key="library.ItemId">
            <VExpansionPanelTitle>
              <div class="uno-flex uno-w-full uno-items-center uno-justify-between uno-gap-4">
                <div class="uno-min-w-0">
                  <div class="uno-font-medium">
                    {{ library.Name }}
                  </div>
                  <div class="uno-text-sm text-medium-emphasis">
                    {{ formatLocations(library.LibraryOptions.PathInfos) || 'No paths configured' }}
                  </div>
                </div>
                <VChip
                  size="small"
                  variant="outlined">
                  {{ library.CollectionType || 'mixed' }}
                </VChip>
              </div>
            </VExpansionPanelTitle>
            <VExpansionPanelText>
              <VRow>
                <VCol
                  cols="12"
                  md="6">
                  <VTextField
                    v-model="library._draftName"
                    label="Library name"
                    density="comfortable"
                    variant="outlined" />
                </VCol>
                <VCol
                  cols="12"
                  md="6">
                  <VSelect
                    :model-value="library.CollectionType || 'mixed'"
                    label="Content type"
                    density="comfortable"
                    variant="outlined"
                    :items="collectionTypes"
                    item-title="title"
                    item-value="value"
                    readonly />
                </VCol>
              </VRow>

              <LibraryOptionsFields
                v-model="library.LibraryOptions"
                :collection-type="library.CollectionType" />

              <div class="uno-mt-4">
                <div class="uno-mb-2 uno-text-sm uno-font-medium">
                  Paths
                </div>
                <VList
                  density="comfortable"
                  class="uno-border uno-rounded">
                  <VListItem
                    v-for="pathInfo in library.LibraryOptions.PathInfos"
                    :key="`${library.ItemId}-${pathInfo.Path}`"
                    :title="pathInfo.Path"
                    :subtitle="pathInfo.NetworkPath || undefined">
                    <template #append>
                      <VBtn
                        variant="text"
                        color="error"
                        @click="removeLibraryPath(library, pathInfo.Path)">
                        {{ t('remove') }}
                      </VBtn>
                    </template>
                  </VListItem>
                  <VListItem v-if="!library.LibraryOptions.PathInfos.length">
                    <VListItemTitle>No paths</VListItemTitle>
                  </VListItem>
                </VList>
                <VRow class="uno-mt-3">
                  <VCol
                    cols="12"
                    md="5">
                    <VTextField
                      v-model="library._newPath"
                      label="Folder path"
                      density="comfortable"
                      variant="outlined"
                      hide-details />
                  </VCol>
                  <VCol
                    cols="12"
                    md="5">
                    <VTextField
                      v-model="library._newNetworkPath"
                      label="Network path"
                      density="comfortable"
                      variant="outlined"
                      hide-details />
                  </VCol>
                  <VCol
                    cols="12"
                    md="2">
                    <VBtn
                      block
                      variant="outlined"
                      @click="addLibraryPath(library)">
                      {{ t('add') }}
                    </VBtn>
                  </VCol>
                </VRow>
              </div>

              <div class="uno-mt-4 uno-flex uno-flex-wrap uno-gap-2">
                <VBtn
                  color="primary"
                  variant="elevated"
                  @click="saveLibrary(library)">
                  {{ t('save') }}
                </VBtn>
                <VBtn
                  color="error"
                  variant="text"
                  @click="removeLibrary(library)">
                  {{ t('remove') }}
                </VBtn>
              </div>
            </VExpansionPanelText>
          </VExpansionPanel>
        </VExpansionPanels>

        <VCard v-else>
          <VCardTitle>{{ t('libraries') }}</VCardTitle>
          <VCardText>No libraries yet. Add one to start scanning and managing media.</VCardText>
        </VCard>
      </VCol>

      <VDialog
        v-model="createDialog"
        max-width="920">
        <VCard>
          <VCardTitle>Create library</VCardTitle>
          <VCardText>
            <VRow>
              <VCol
                cols="12"
                md="6">
                <VTextField
                  v-model="createForm.name"
                  label="Library name"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
              <VCol
                cols="12"
                md="6">
                <VSelect
                  v-model="createForm.collectionType"
                  label="Content type"
                  density="comfortable"
                  variant="outlined"
                  :items="collectionTypes"
                  item-title="title"
                  item-value="value" />
              </VCol>
              <VCol cols="12">
                <VTextarea
                  v-model="createForm.pathsText"
                  label="Paths, one per line. Use path | network path when needed."
                  rows="4"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
            </VRow>

            <LibraryOptionsFields
              v-model="createForm.libraryOptions"
              :collection-type="createForm.collectionType" />
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn
              variant="text"
              @click="createDialog = false">
              {{ t('cancel') }}
            </VBtn>
            <VBtn
              color="primary"
              variant="elevated"
              @click="createLibrary">
              {{ t('save') }}
            </VBtn>
          </VCardActions>
        </VCard>
      </VDialog>
    </template>
  </SettingsPage>
</template>

<route lang="yaml">
meta:
  admin: true
</route>

<script setup lang="ts">
import { computed, defineComponent, h, ref, type PropType } from 'vue';
import {
  VCol,
  VCombobox,
  VRow,
  VSwitch,
  VTextField
} from 'vuetify/components';
import { useTranslation } from 'i18next-vue';
import { useSnackbar } from '#/composables/use-snackbar.ts';
import { remote } from '#/plugins/remote/index.ts';

interface MediaPathInfoDto {
  Path: string;
  NetworkPath?: string | null;
}

interface LibraryOptionsDto {
  Enabled: boolean;
  EnableArchiveMediaFiles: boolean;
  EnablePhotos: boolean;
  EnableRealtimeMonitor: boolean;
  EnableChapterImageExtraction: boolean;
  ExtractChapterImagesDuringLibraryScan: boolean;
  SaveLocalMetadata: boolean;
  EnableInternetProviders: boolean;
  DownloadImagesInAdvance: boolean;
  ImportMissingEpisodes: boolean;
  EnableAutomaticSeriesGrouping: boolean;
  EnableEmbeddedTitles: boolean;
  EnableEmbeddedEpisodeInfos: boolean;
  AutomaticRefreshIntervalDays: number;
  PreferredMetadataLanguage?: string | null;
  MetadataCountryCode?: string | null;
  SeasonZeroDisplayName: string;
  MetadataSavers: string[];
  DisabledLocalMetadataReaders: string[];
  LocalMetadataReaderOrder: string[];
  PathInfos: MediaPathInfoDto[];
}

interface VirtualFolderInfoDto {
  Name: string;
  CollectionType: string;
  ItemId: string;
  Locations: string[];
  LibraryOptions: LibraryOptionsDto;
  _draftName: string;
  _newPath: string;
  _newNetworkPath: string;
}

interface SelectableMediaFolderDto {
  Name: string;
  Id: string;
}

const { t } = useTranslation();
const libraries = ref<VirtualFolderInfoDto[]>([]);
const selectableFolders = ref<SelectableMediaFolderDto[]>([]);
const loading = ref(false);
const errorMessage = ref('');
const createDialog = ref(false);
const collectionTypes = [
  { title: 'Movies', value: 'movies' },
  { title: 'TV Shows', value: 'tvshows' },
  { title: 'Music', value: 'music' },
  { title: 'Music Videos', value: 'musicvideos' },
  { title: 'Home Videos', value: 'homevideos' },
  { title: 'Books', value: 'books' },
  { title: 'Mixed', value: 'mixed' }
];

const createForm = ref({
  name: '',
  collectionType: 'movies',
  pathsText: '',
  libraryOptions: defaultLibraryOptions()
});

const apiBase = computed(() => remote.sdk.api?.basePath ?? '');
const apiKey = computed(() => remote.auth.currentUserToken.value ?? '');

const LibraryOptionsFields = defineComponent({
  props: {
    modelValue: {
      type: Object as PropType<LibraryOptionsDto>,
      required: true
    },
    collectionType: {
      type: String,
      required: true
    }
  },
  emits: ['update:modelValue'],
  setup(props, { emit }) {
    const update = <K extends keyof LibraryOptionsDto>(key: K, value: LibraryOptionsDto[K]): void => {
      emit('update:modelValue', {
        ...props.modelValue,
        [key]: value
      });
    };
    const field = (key: keyof LibraryOptionsDto, label: string) => h(VSwitch, {
      modelValue: props.modelValue[key],
      label,
      color: 'primary',
      inset: true,
      'onUpdate:modelValue': (value: boolean) => update(key, value as never)
    });
    const showMetadata = !['homevideos', 'photos'].includes(props.collectionType);
    const showChapters = ['tvshows', 'movies', 'homevideos', 'musicvideos', 'mixed', ''].includes(props.collectionType);
    const showTv = props.collectionType === 'tvshows';

    return () => h('div', { class: 'uno-mt-4' }, [
      h(VRow, {}, () => [
        showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.PreferredMetadataLanguage,
          label: 'Metadata language',
          density: 'comfortable',
          variant: 'outlined',
          'onUpdate:modelValue': (value: string) => update('PreferredMetadataLanguage', value || null)
        })),
        showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.MetadataCountryCode,
          label: 'Country code',
          density: 'comfortable',
          variant: 'outlined',
          'onUpdate:modelValue': (value: string) => update('MetadataCountryCode', value || null)
        })),
        showTv && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.SeasonZeroDisplayName,
          label: 'Season zero display name',
          density: 'comfortable',
          variant: 'outlined',
          'onUpdate:modelValue': (value: string) => update('SeasonZeroDisplayName', value || 'Specials')
        })),
        h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.AutomaticRefreshIntervalDays,
          label: 'Automatic refresh interval days',
          density: 'comfortable',
          variant: 'outlined',
          type: 'number',
          min: 0,
          'onUpdate:modelValue': (value: string) => update('AutomaticRefreshIntervalDays', Number(value) || 0)
        })),
        h(VCol, { cols: '12', md: '4' }, () => field('Enabled', 'Enabled')),
        props.collectionType === 'homevideos' && h(VCol, { cols: '12', md: '4' }, () => field('EnablePhotos', 'Enable photos')),
        h(VCol, { cols: '12', md: '4' }, () => field('EnableRealtimeMonitor', 'Realtime monitor')),
        showMetadata && h(VCol, { cols: '12', md: '4' }, () => field('EnableInternetProviders', 'Download internet metadata')),
        showMetadata && h(VCol, { cols: '12', md: '4' }, () => field('DownloadImagesInAdvance', 'Download images in advance')),
        props.collectionType !== 'photos' && h(VCol, { cols: '12', md: '4' }, () => field('SaveLocalMetadata', 'Save local metadata')),
        showTv && h(VCol, { cols: '12', md: '4' }, () => field('ImportMissingEpisodes', 'Import missing episodes')),
        showTv && h(VCol, { cols: '12', md: '4' }, () => field('EnableAutomaticSeriesGrouping', 'Automatically group series')),
        showChapters && h(VCol, { cols: '12', md: '4' }, () => field('EnableChapterImageExtraction', 'Extract chapter images')),
        showChapters && h(VCol, { cols: '12', md: '4' }, () => field('ExtractChapterImagesDuringLibraryScan', 'Extract chapters during scan')),
        h(VCol, { cols: '12', md: '4' }, () => field('EnableEmbeddedTitles', 'Use embedded titles')),
        showTv && h(VCol, { cols: '12', md: '4' }, () => field('EnableEmbeddedEpisodeInfos', 'Use embedded episode info')),
        h(VCol, { cols: '12', md: '4' }, () => field('EnableArchiveMediaFiles', 'Archive media files')),
        h(VCol, { cols: '12', md: '4' }, () => h(VCombobox, {
          modelValue: props.modelValue.MetadataSavers,
          label: 'Metadata savers',
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: ['Nfo'],
          'onUpdate:modelValue': (value: string[]) => update('MetadataSavers', value)
        })),
        h(VCol, { cols: '12', md: '4' }, () => h(VCombobox, {
          modelValue: props.modelValue.LocalMetadataReaderOrder,
          label: 'Local metadata reader order',
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: ['Nfo'],
          'onUpdate:modelValue': (value: string[]) => update('LocalMetadataReaderOrder', value)
        })),
        h(VCol, { cols: '12', md: '4' }, () => h(VCombobox, {
          modelValue: props.modelValue.DisabledLocalMetadataReaders,
          label: 'Disabled local metadata readers',
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: ['Nfo'],
          'onUpdate:modelValue': (value: string[]) => update('DisabledLocalMetadataReaders', value)
        }))
      ])
    ]);
  }
});

function buildUrl(path: string): string {
  const url = new URL(`${apiBase.value}${path}`);

  if (apiKey.value) {
    url.searchParams.set('api_key', apiKey.value);
  }

  return url.toString();
}

function defaultLibraryOptions(): LibraryOptionsDto {
  return {
    Enabled: true,
    EnableArchiveMediaFiles: false,
    EnablePhotos: true,
    EnableRealtimeMonitor: false,
    EnableChapterImageExtraction: false,
    ExtractChapterImagesDuringLibraryScan: false,
    SaveLocalMetadata: false,
    EnableInternetProviders: true,
    DownloadImagesInAdvance: false,
    ImportMissingEpisodes: false,
    EnableAutomaticSeriesGrouping: true,
    EnableEmbeddedTitles: false,
    EnableEmbeddedEpisodeInfos: false,
    AutomaticRefreshIntervalDays: 0,
    PreferredMetadataLanguage: 'zh',
    MetadataCountryCode: 'CN',
    SeasonZeroDisplayName: 'Specials',
    MetadataSavers: ['Nfo'],
    DisabledLocalMetadataReaders: [],
    LocalMetadataReaderOrder: ['Nfo'],
    PathInfos: []
  };
}

function normalizeOptions(options: Partial<LibraryOptionsDto> | undefined, locations: string[]): LibraryOptionsDto {
  const defaults = defaultLibraryOptions();
  const pathInfos = options?.PathInfos?.length
    ? options.PathInfos
    : locations.map(path => ({ Path: path }));

  return {
    ...defaults,
    ...options,
    PathInfos: pathInfos.map(pathInfo => ({
      Path: pathInfo.Path,
      NetworkPath: pathInfo.NetworkPath || null
    }))
  };
}

function normalizeLibrary(library: Omit<VirtualFolderInfoDto, '_draftName' | '_newPath' | '_newNetworkPath'>): VirtualFolderInfoDto {
  return {
    ...library,
    _draftName: library.Name,
    _newPath: '',
    _newNetworkPath: '',
    LibraryOptions: normalizeOptions(library.LibraryOptions, library.Locations)
  };
}

function formatLocations(pathInfos: MediaPathInfoDto[]): string {
  return pathInfos.map(pathInfo => pathInfo.Path).filter(Boolean).join(' | ');
}

function parsePathInfos(text: string): MediaPathInfoDto[] {
  return text
    .split(/\r?\n/g)
    .map(line => line.trim())
    .filter(Boolean)
    .map((line) => {
      const [path, networkPath] = line.split('|').map(value => value.trim());

      return {
        Path: path ?? '',
        NetworkPath: networkPath || null
      };
    })
    .filter(pathInfo => pathInfo.Path);
}

async function requestJson<T>(path: string): Promise<T> {
  const response = await fetch(buildUrl(path));

  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }

  return await response.json() as T;
}

async function request(path: string, init?: RequestInit): Promise<void> {
  const response = await fetch(buildUrl(path), init);

  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
}

async function loadLibraries(): Promise<void> {
  if (!apiBase.value) {
    return;
  }

  loading.value = true;
  errorMessage.value = '';

  try {
    const [virtualFolders, mediaFolders] = await Promise.all([
      requestJson<Omit<VirtualFolderInfoDto, '_draftName' | '_newPath' | '_newNetworkPath'>[]>('/Library/VirtualFolders/Query'),
      requestJson<SelectableMediaFolderDto[]>('/Library/SelectableMediaFolders')
    ]);

    libraries.value = virtualFolders.map(normalizeLibrary);
    selectableFolders.value = mediaFolders;
  } catch (error) {
    console.error(error);
    libraries.value = [];
    selectableFolders.value = [];
    errorMessage.value = 'Failed to load libraries. Check EmbySDK-compatible Library endpoints.';
  } finally {
    loading.value = false;
  }
}

async function refreshAllLibraries(): Promise<void> {
  try {
    await request('/Library/Refresh', { method: 'POST' });
    useSnackbar('Library refresh started', 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar('Failed to refresh libraries', 'error');
  }
}

async function saveLibrary(library: VirtualFolderInfoDto): Promise<void> {
  const nextName = library._draftName.trim();

  try {
    if (nextName && nextName !== library.Name) {
      await request(
        `/Library/VirtualFolders/Name?Name=${encodeURIComponent(library.Name)}&NewName=${encodeURIComponent(nextName)}`,
        { method: 'POST' }
      );
      library.Name = nextName;
    }

    await fetch(buildUrl('/Library/VirtualFolders/LibraryOptions'), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        Id: library.ItemId,
        LibraryOptions: library.LibraryOptions
      })
    }).then(async (response) => {
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
    });

    useSnackbar(`${library.Name} saved`, 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar('Failed to save library', 'error');
  }
}

async function addLibraryPath(library: VirtualFolderInfoDto): Promise<void> {
  const path = library._newPath.trim();

  if (!path) {
    return;
  }

  const pathInfo = {
    Path: path,
    NetworkPath: library._newNetworkPath.trim() || null
  };

  try {
    await fetch(buildUrl('/Library/VirtualFolders/Paths'), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        Name: library.Name,
        PathInfo: pathInfo
      })
    }).then(async (response) => {
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
    });

    library._newPath = '';
    library._newNetworkPath = '';
    useSnackbar('Path added', 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar('Failed to add path', 'error');
  }
}

async function removeLibraryPath(library: VirtualFolderInfoDto, path: string): Promise<void> {
  try {
    await request(`/Library/VirtualFolders/Paths?Name=${encodeURIComponent(library.Name)}&Path=${encodeURIComponent(path)}`, {
      method: 'DELETE'
    });
    useSnackbar('Path removed', 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar('Failed to remove path', 'error');
  }
}

async function removeLibrary(library: VirtualFolderInfoDto): Promise<void> {
  try {
    await request(`/Library/VirtualFolders?Name=${encodeURIComponent(library.Name)}`, {
      method: 'DELETE'
    });
    useSnackbar(`${library.Name} removed`, 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar('Failed to remove library', 'error');
  }
}

async function createLibrary(): Promise<void> {
  const pathInfos = parsePathInfos(createForm.value.pathsText);

  if (!createForm.value.name.trim() || !pathInfos.length) {
    useSnackbar('Provide a name and at least one path', 'error');
    return;
  }

  try {
    const collectionType = createForm.value.collectionType === 'mixed' ? '' : createForm.value.collectionType;
    const response = await fetch(buildUrl(`/Library/VirtualFolders?Name=${encodeURIComponent(createForm.value.name.trim())}&CollectionType=${encodeURIComponent(collectionType)}`), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        LibraryOptions: {
          ...createForm.value.libraryOptions,
          PathInfos: pathInfos
        }
      })
    });

    if (!response.ok) {
      useSnackbar(`Create failed: HTTP ${response.status}`, 'error');
      return;
    }

    createDialog.value = false;
    createForm.value = {
      name: '',
      collectionType: 'movies',
      pathsText: '',
      libraryOptions: defaultLibraryOptions()
    };
    useSnackbar('Library created', 'success');
    await loadLibraries();
    await refreshAllLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar('Failed to create library', 'error');
  }
}

await loadLibraries();
</script>
