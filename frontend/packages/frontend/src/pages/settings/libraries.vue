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
              <VCardTitle>{{ t('selectableMediaFolders') }}</VCardTitle>
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
                    {{ formatLocations(library.LibraryOptions.PathInfos) || t('noPathsConfigured') }}
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
                    :label="t('libraryName')"
                    density="comfortable"
                    variant="outlined" />
                </VCol>
                <VCol
                  cols="12"
                  md="6">
                  <VSelect
                    :model-value="library.CollectionType || 'mixed'"
                    :label="t('contentType')"
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
                  {{ t('paths') }}
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
                    <VListItemTitle>{{ t('noPaths') }}</VListItemTitle>
                  </VListItem>
                </VList>
                <VRow class="uno-mt-3">
                  <VCol
                    cols="12"
                    md="5">
                    <VTextField
                      v-model="library._newPath"
                      :label="t('folderPath')"
                      density="comfortable"
                      variant="outlined"
                      hide-details />
                  </VCol>
                  <VCol
                    cols="12"
                    md="5">
                    <VTextField
                      v-model="library._newNetworkPath"
                      :label="t('networkPath')"
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
          <VCardText>{{ t('noLibrariesYet') }}</VCardText>
        </VCard>
      </VCol>

      <VDialog
        v-model="createDialog"
        max-width="920">
        <VCard>
          <VCardTitle>{{ t('createLibrary') }}</VCardTitle>
          <VCardText>
            <VRow>
              <VCol
                cols="12"
                md="6">
                <VTextField
                  v-model="createForm.name"
                  :label="t('libraryName')"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
              <VCol
                cols="12"
                md="6">
                <VSelect
                  v-model="createForm.collectionType"
                  :label="t('contentType')"
                  density="comfortable"
                  variant="outlined"
                  :items="collectionTypes"
                  item-title="title"
                  item-value="value" />
              </VCol>
              <VCol cols="12">
                <VTextarea
                  v-model="createForm.pathsText"
                  :label="t('pathsOnePerLine')"
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
import type {
  SettingsLibraryOptionInfo,
  SettingsLibraryOptions,
  SettingsLibraryOptionsResult,
  SettingsMediaPathInfo,
  SettingsSelectableMediaFolder,
  SettingsVirtualFolderInfo
} from '#/composables/use-settings-sdk.ts';
import { useSettingsSdk } from '#/composables/use-settings-sdk.ts';
import { useSnackbar } from '#/composables/use-snackbar.ts';

type MediaPathInfoDto = SettingsMediaPathInfo;
type EditableLibraryOptionsFields = Required<Pick<SettingsLibraryOptions,
  'Enabled'
  | 'EnableArchiveMediaFiles'
  | 'EnablePhotos'
  | 'EnableRealtimeMonitor'
  | 'EnableMarkerDetection'
  | 'EnableMarkerDetectionDuringLibraryScan'
  | 'EnableChapterImageExtraction'
  | 'ExtractChapterImagesDuringLibraryScan'
  | 'CacheImages'
  | 'ExcludeFromSearch'
  | 'IgnoreHiddenFiles'
  | 'IgnoreFileExtensions'
  | 'SaveLocalMetadata'
  | 'SaveMetadataHidden'
  | 'SaveLocalThumbnailSets'
  | 'EnableInternetProviders'
  | 'DownloadImagesInAdvance'
  | 'ImportPlaylists'
  | 'ImportMissingEpisodes'
  | 'EnableAutomaticSeriesGrouping'
  | 'ShareEmbeddedMusicAlbumImages'
  | 'EnableEmbeddedTitles'
  | 'EnableAudioResume'
  | 'AutoGenerateChapters'
  | 'MergeTopLevelFolders'
  | 'EnableEmbeddedEpisodeInfos'
  | 'AutomaticRefreshIntervalDays'
  | 'PlaceholderMetadataRefreshIntervalDays'
  | 'SeasonZeroDisplayName'
  | 'MetadataSavers'
  | 'DisabledLocalMetadataReaders'
  | 'LocalMetadataReaderOrder'
  | 'DisabledLyricsFetchers'
  | 'SaveLyricsWithMedia'
  | 'DisabledSubtitleFetchers'
  | 'SaveSubtitlesWithMedia'
  | 'CollapseSingleItemFolders'
  | 'ForceCollapseSingleItemFolders'
  | 'ImportCollections'
  | 'EnableMultiVersionByFiles'
  | 'EnableMultiVersionByMetadata'
  | 'EnableMultiPartItems'
  | 'PathInfos'
>> & Pick<SettingsLibraryOptions,
  'PreferredMetadataLanguage'
  | 'PreferredImageLanguage'
  | 'MetadataCountryCode'
  | 'MinResumePct'
  | 'MaxResumePct'
  | 'MinResumeDurationSeconds'
>;
type LibraryOptionsDto = SettingsLibraryOptions & EditableLibraryOptionsFields;

interface VirtualFolderInfoDto extends Omit<SettingsVirtualFolderInfo, 'LibraryOptions'> {
  LibraryOptions: LibraryOptionsDto;
  _draftName: string;
  _newPath: string;
  _newNetworkPath: string;
}

interface SelectableMediaFolderDto extends SettingsSelectableMediaFolder {}

const { t } = useTranslation();
const { librariesApi } = useSettingsSdk();
const libraries = ref<VirtualFolderInfoDto[]>([]);
const selectableFolders = ref<SelectableMediaFolderDto[]>([]);
const availableLibraryOptions = ref<SettingsLibraryOptionsResult>({});
const loading = ref(false);
const errorMessage = ref('');
const createDialog = ref(false);
const collectionTypes = computed(() => [
  { title: t('movies'), value: 'movies' },
  { title: t('shows'), value: 'tvshows' },
  { title: t('music'), value: 'music' },
  { title: t('musicVideos'), value: 'musicvideos' },
  { title: t('homeVideos'), value: 'homevideos' },
  { title: t('books'), value: 'books' },
  { title: t('mixedContent'), value: 'mixed' }
]);

const createForm = ref({
  name: '',
  collectionType: 'movies',
  pathsText: '',
  libraryOptions: defaultLibraryOptions()
});

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
    const showMovieLike = ['movies', 'tvshows', 'homevideos', 'musicvideos', 'mixed', ''].includes(props.collectionType);
    const showMusic = props.collectionType === 'music';
    const metadataSaverItems = computed(() => libraryOptionNames(availableLibraryOptions.value.MetadataSavers, props.modelValue.MetadataSavers));
    const metadataReaderItems = computed(() => libraryOptionNames(availableLibraryOptions.value.MetadataReaders, [
      ...props.modelValue.LocalMetadataReaderOrder,
      ...props.modelValue.DisabledLocalMetadataReaders
    ]));
    const subtitleFetcherItems = computed(() => libraryOptionNames(availableLibraryOptions.value.SubtitleFetchers, props.modelValue.DisabledSubtitleFetchers));
    const lyricsFetcherItems = computed(() => libraryOptionNames(availableLibraryOptions.value.LyricsFetchers, props.modelValue.DisabledLyricsFetchers));
    const optionalNumber = (key: keyof LibraryOptionsDto, label: string) => h(VTextField, {
      modelValue: props.modelValue[key] ?? '',
      label,
      density: 'comfortable',
      variant: 'outlined',
      type: 'number',
      min: 0,
      'onUpdate:modelValue': (value: string) => {
        const trimmed = value?.toString().trim() ?? '';
        update(key, (trimmed === '' ? null : Number(trimmed)) as never);
      }
    });

    return () => h('div', { class: 'uno-mt-4' }, [
      h(VRow, {}, () => [
        showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.PreferredMetadataLanguage,
          label: t('metadataLanguage'),
          density: 'comfortable',
          variant: 'outlined',
          'onUpdate:modelValue': (value: string) => update('PreferredMetadataLanguage', value || null)
        })),
        showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.PreferredImageLanguage,
          label: t('preferredImageLanguage'),
          density: 'comfortable',
          variant: 'outlined',
          'onUpdate:modelValue': (value: string) => update('PreferredImageLanguage', value || null)
        })),
        showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.MetadataCountryCode,
          label: t('countryCode'),
          density: 'comfortable',
          variant: 'outlined',
          'onUpdate:modelValue': (value: string) => update('MetadataCountryCode', value || null)
        })),
        showTv && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.SeasonZeroDisplayName,
          label: t('seasonZeroDisplayName'),
          density: 'comfortable',
          variant: 'outlined',
          'onUpdate:modelValue': (value: string) => update('SeasonZeroDisplayName', value || 'Specials')
        })),
        h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.AutomaticRefreshIntervalDays,
          label: t('automaticRefreshIntervalDays'),
          density: 'comfortable',
          variant: 'outlined',
          type: 'number',
          min: 0,
          'onUpdate:modelValue': (value: string) => update('AutomaticRefreshIntervalDays', Number(value) || 0)
        })),
        showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.PlaceholderMetadataRefreshIntervalDays,
          label: t('placeholderMetadataRefreshIntervalDays'),
          density: 'comfortable',
          variant: 'outlined',
          type: 'number',
          min: 0,
          'onUpdate:modelValue': (value: string) => update('PlaceholderMetadataRefreshIntervalDays', Number(value) || 0)
        })),
        h(VCol, { cols: '12', md: '4' }, () => field('Enabled', t('enabled'))),
        props.collectionType === 'homevideos' && h(VCol, { cols: '12', md: '4' }, () => field('EnablePhotos', t('enablePhotos'))),
        h(VCol, { cols: '12', md: '4' }, () => field('EnableRealtimeMonitor', t('realtimeMonitor'))),
        h(VCol, { cols: '12', md: '4' }, () => field('CacheImages', t('cacheImages'))),
        h(VCol, { cols: '12', md: '4' }, () => field('ExcludeFromSearch', t('excludeFromSearch'))),
        h(VCol, { cols: '12', md: '4' }, () => field('IgnoreHiddenFiles', t('ignoreHiddenFiles'))),
        showMetadata && h(VCol, { cols: '12', md: '4' }, () => field('EnableInternetProviders', t('downloadInternetMetadata'))),
        showMetadata && h(VCol, { cols: '12', md: '4' }, () => field('DownloadImagesInAdvance', t('downloadImagesInAdvance'))),
        props.collectionType !== 'photos' && h(VCol, { cols: '12', md: '4' }, () => field('SaveLocalMetadata', t('saveLocalMetadata'))),
        props.collectionType !== 'photos' && h(VCol, { cols: '12', md: '4' }, () => field('SaveMetadataHidden', t('saveMetadataHidden'))),
        props.collectionType !== 'photos' && h(VCol, { cols: '12', md: '4' }, () => field('SaveLocalThumbnailSets', t('saveLocalThumbnailSets'))),
        showTv && h(VCol, { cols: '12', md: '4' }, () => field('ImportMissingEpisodes', t('importMissingEpisodes'))),
        showTv && h(VCol, { cols: '12', md: '4' }, () => field('EnableAutomaticSeriesGrouping', t('automaticallyGroupSeries'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMarkerDetection', t('enableMarkerDetection'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMarkerDetectionDuringLibraryScan', t('enableMarkerDetectionDuringLibraryScan'))),
        showChapters && h(VCol, { cols: '12', md: '4' }, () => field('EnableChapterImageExtraction', t('extractChapterImages'))),
        showChapters && h(VCol, { cols: '12', md: '4' }, () => field('ExtractChapterImagesDuringLibraryScan', t('extractChaptersDuringScan'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('AutoGenerateChapters', t('autoGenerateChapters'))),
        h(VCol, { cols: '12', md: '4' }, () => field('EnableEmbeddedTitles', t('useEmbeddedTitles'))),
        showTv && h(VCol, { cols: '12', md: '4' }, () => field('EnableEmbeddedEpisodeInfos', t('useEmbeddedEpisodeInfo'))),
        h(VCol, { cols: '12', md: '4' }, () => field('EnableArchiveMediaFiles', t('archiveMediaFiles'))),
        showMusic && h(VCol, { cols: '12', md: '4' }, () => field('ImportPlaylists', t('importPlaylists'))),
        showMusic && h(VCol, { cols: '12', md: '4' }, () => field('ShareEmbeddedMusicAlbumImages', t('shareEmbeddedMusicAlbumImages'))),
        showMusic && h(VCol, { cols: '12', md: '4' }, () => field('EnableAudioResume', t('enableAudioResume'))),
        showMusic && h(VCol, { cols: '12', md: '4' }, () => field('SaveLyricsWithMedia', t('saveLyricsWithMedia'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('SaveSubtitlesWithMedia', t('saveSubtitlesWithMedia'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('MergeTopLevelFolders', t('mergeTopLevelFolders'))),
        h(VCol, { cols: '12', md: '4' }, () => field('CollapseSingleItemFolders', t('collapseSingleItemFolders'))),
        h(VCol, { cols: '12', md: '4' }, () => field('ForceCollapseSingleItemFolders', t('forceCollapseSingleItemFolders'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('ImportCollections', t('importCollections'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMultiVersionByFiles', t('enableMultiVersionByFiles'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMultiVersionByMetadata', t('enableMultiVersionByMetadata'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMultiPartItems', t('enableMultiPartItems'))),
        h(VCol, { cols: '12', md: '4' }, () => h(VCombobox, {
          modelValue: props.modelValue.MetadataSavers,
          label: t('metadataSavers'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: metadataSaverItems.value,
          'onUpdate:modelValue': (value: string[]) => update('MetadataSavers', value)
        })),
        h(VCol, { cols: '12', md: '4' }, () => h(VCombobox, {
          modelValue: props.modelValue.LocalMetadataReaderOrder,
          label: t('localMetadataReaderOrder'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: metadataReaderItems.value,
          'onUpdate:modelValue': (value: string[]) => update('LocalMetadataReaderOrder', value)
        })),
        h(VCol, { cols: '12', md: '4' }, () => h(VCombobox, {
          modelValue: props.modelValue.DisabledLocalMetadataReaders,
          label: t('disabledLocalMetadataReaders'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: metadataReaderItems.value,
          'onUpdate:modelValue': (value: string[]) => update('DisabledLocalMetadataReaders', value)
        })),
        h(VCol, { cols: '12', md: '6' }, () => h(VCombobox, {
          modelValue: props.modelValue.IgnoreFileExtensions,
          label: t('ignoreFileExtensions'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          'onUpdate:modelValue': (value: string[]) => update('IgnoreFileExtensions', value)
        })),
        showMusic && h(VCol, { cols: '12', md: '6' }, () => h(VCombobox, {
          modelValue: props.modelValue.DisabledLyricsFetchers,
          label: t('disabledLyricsFetchers'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: lyricsFetcherItems.value,
          'onUpdate:modelValue': (value: string[]) => update('DisabledLyricsFetchers', value)
        })),
        showMovieLike && h(VCol, { cols: '12', md: '6' }, () => h(VCombobox, {
          modelValue: props.modelValue.DisabledSubtitleFetchers,
          label: t('disabledSubtitleFetchers'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: subtitleFetcherItems.value,
          'onUpdate:modelValue': (value: string[]) => update('DisabledSubtitleFetchers', value)
        })),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => optionalNumber('MinResumePct', t('minResumePct'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => optionalNumber('MaxResumePct', t('maxResumePct'))),
        showMovieLike && h(VCol, { cols: '12', md: '4' }, () => optionalNumber('MinResumeDurationSeconds', t('minResumeDurationSeconds')))
      ])
    ]);
  }
});

function libraryOptionNames(options: SettingsLibraryOptionInfo[] | undefined, fallback: string[] = []): string[] {
  const values = [
    ...(options?.map(option => option.Name).filter((value): value is string => Boolean(value)) ?? []),
    ...fallback
  ];

  return [...new Set(values.filter(Boolean))];
}

function defaultLibraryOptions(base?: Partial<SettingsLibraryOptions>): LibraryOptionsDto {
  return {
    Enabled: true,
    EnableArchiveMediaFiles: false,
    EnablePhotos: true,
    EnableRealtimeMonitor: false,
    EnableMarkerDetection: false,
    EnableMarkerDetectionDuringLibraryScan: false,
    EnableChapterImageExtraction: false,
    ExtractChapterImagesDuringLibraryScan: false,
    CacheImages: false,
    ExcludeFromSearch: false,
    IgnoreHiddenFiles: true,
    IgnoreFileExtensions: [],
    SaveLocalMetadata: false,
    SaveMetadataHidden: false,
    SaveLocalThumbnailSets: false,
    EnableInternetProviders: true,
    DownloadImagesInAdvance: false,
    ImportPlaylists: false,
    ImportMissingEpisodes: false,
    EnableAutomaticSeriesGrouping: true,
    ShareEmbeddedMusicAlbumImages: false,
    EnableEmbeddedTitles: false,
    EnableAudioResume: false,
    AutoGenerateChapters: false,
    MergeTopLevelFolders: false,
    EnableEmbeddedEpisodeInfos: false,
    AutomaticRefreshIntervalDays: 0,
    PlaceholderMetadataRefreshIntervalDays: 0,
    PreferredMetadataLanguage: 'zh',
    PreferredImageLanguage: null,
    MetadataCountryCode: 'CN',
    SeasonZeroDisplayName: 'Specials',
    MetadataSavers: ['Nfo'],
    DisabledLocalMetadataReaders: [],
    LocalMetadataReaderOrder: ['Nfo'],
    DisabledLyricsFetchers: [],
    SaveLyricsWithMedia: false,
    DisabledSubtitleFetchers: [],
    SaveSubtitlesWithMedia: false,
    CollapseSingleItemFolders: false,
    ForceCollapseSingleItemFolders: false,
    ImportCollections: false,
    EnableMultiVersionByFiles: false,
    EnableMultiVersionByMetadata: false,
    EnableMultiPartItems: false,
    MinResumePct: null,
    MaxResumePct: null,
    MinResumeDurationSeconds: null,
    PathInfos: [],
    ...base
  };
}

function normalizeOptions(options: Partial<LibraryOptionsDto> | undefined, locations: string[]): LibraryOptionsDto {
  const defaults = defaultLibraryOptions(availableLibraryOptions.value.DefaultLibraryOptions);
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

async function loadLibraries(): Promise<void> {
  loading.value = true;
  errorMessage.value = '';

  try {
    const [virtualFolders, mediaFolders, availableOptions] = await Promise.all([
      librariesApi.getLibraryVirtualfoldersQuery(),
      librariesApi.getLibrarySelectablemediafolders(),
      librariesApi.getLibrariesAvailableoptions()
    ]);

    availableLibraryOptions.value = availableOptions;
    if (!createForm.value.name && !createForm.value.pathsText) {
      createForm.value.libraryOptions = defaultLibraryOptions(availableOptions.DefaultLibraryOptions);
    }
    libraries.value = virtualFolders.map(normalizeLibrary);
    selectableFolders.value = mediaFolders;
  } catch (error) {
    console.error(error);
    libraries.value = [];
    selectableFolders.value = [];
    availableLibraryOptions.value = {};
    errorMessage.value = t('failedToLoadLibraries');
  } finally {
    loading.value = false;
  }
}

async function refreshAllLibraries(): Promise<void> {
  try {
    await librariesApi.postLibraryRefresh();
    useSnackbar(t('libraryRefreshStarted'), 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar(t('failedToRefreshLibraries'), 'error');
  }
}

async function saveLibrary(library: VirtualFolderInfoDto): Promise<void> {
  const nextName = library._draftName.trim();

  try {
    if (nextName && nextName !== library.Name) {
      await librariesApi.postLibraryVirtualfoldersName({
        Id: library.ItemId,
        Name: library.Name,
        NewName: nextName
      });
      library.Name = nextName;
    }

    await librariesApi.postLibraryVirtualfoldersLibraryoptions({
      Id: library.ItemId,
      LibraryOptions: library.LibraryOptions
    });

    useSnackbar(t('librarySaved', { name: library.Name }), 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar(t('failedToSaveLibrary'), 'error');
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
    await librariesApi.postLibraryVirtualfoldersPaths({
      Id: library.ItemId,
      Name: library.Name,
      PathInfo: pathInfo
    });

    library._newPath = '';
    library._newNetworkPath = '';
    useSnackbar(t('pathAdded'), 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar(t('failedToAddPath'), 'error');
  }
}

async function removeLibraryPath(library: VirtualFolderInfoDto, path: string): Promise<void> {
  try {
    await librariesApi.deleteLibraryVirtualfoldersPaths({
      Id: library.ItemId,
      Name: library.Name,
      Path: path
    });
    useSnackbar(t('pathRemoved'), 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar(t('failedToRemovePath'), 'error');
  }
}

async function removeLibrary(library: VirtualFolderInfoDto): Promise<void> {
  try {
    await librariesApi.deleteLibraryVirtualfolders({
      Id: library.ItemId,
      Name: library.Name
    });
    useSnackbar(t('libraryRemoved', { name: library.Name }), 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar(t('failedToRemoveLibrary'), 'error');
  }
}

async function createLibrary(): Promise<void> {
  const pathInfos = parsePathInfos(createForm.value.pathsText);

  if (!createForm.value.name.trim() || !pathInfos.length) {
    useSnackbar(t('provideNameAndPath'), 'error');
    return;
  }

  try {
    const collectionType = createForm.value.collectionType === 'mixed' ? '' : createForm.value.collectionType;
    await librariesApi.postLibraryVirtualfolders({
      Name: createForm.value.name.trim(),
      CollectionType: collectionType,
      RefreshLibrary: false,
      Paths: pathInfos.map(pathInfo => pathInfo.Path),
      LibraryOptions: {
        ...createForm.value.libraryOptions,
        PathInfos: pathInfos
      }
    });

    createDialog.value = false;
    createForm.value = {
      name: '',
      collectionType: 'movies',
      pathsText: '',
      libraryOptions: defaultLibraryOptions(availableLibraryOptions.value.DefaultLibraryOptions)
    };
    useSnackbar(t('libraryCreated'), 'success');
    await loadLibraries();
    await refreshAllLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar(t('failedToCreateLibrary'), 'error');
  }
}

await loadLibraries();
</script>
