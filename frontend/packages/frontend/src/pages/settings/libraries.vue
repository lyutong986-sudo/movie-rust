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
          @click="openCreateDialog">
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

        <div class="library-card-grid">
          <article
            v-for="library in libraries"
            :key="library.ItemId"
            class="library-card">
            <button
              type="button"
              class="library-card-visual"
              @click="openEditDialog(library)">
              <img
                v-if="libraryPrimaryImageUrl(library)"
                :src="libraryPrimaryImageUrl(library)"
                :alt="library.Name"
                class="library-card-image">
              <JIcon
                v-else
                class="library-card-icon"
                :class="libraryIconClass(library.CollectionType)" />
            </button>
            <div class="library-card-footer">
              <VMenu>
                <template #activator="{ props }">
                  <VBtn
                    v-bind="props"
                    icon
                    size="small"
                    variant="text"
                    class="library-card-menu"
                    @click.stop>
                    <JIcon class="i-mdi:dots-vertical" />
                  </VBtn>
                </template>
                <VList density="compact">
                  <VListItem
                    :title="t('edit')"
                    @click="openEditDialog(library)">
                    <template #prepend>
                      <JIcon class="i-mdi:folder-open" />
                    </template>
                  </VListItem>
                  <VListItem
                    :title="t('editMetadata')"
                    @click="editLibraryImages(library)">
                    <template #prepend>
                      <JIcon class="i-mdi:image" />
                    </template>
                  </VListItem>
                  <VListItem
                    :title="t('contentType')"
                    @click="showChangeTypeNotice(library)">
                    <template #prepend>
                      <JIcon class="i-mdi:movie-cog" />
                    </template>
                  </VListItem>
                  <VListItem
                    :title="t('remove')"
                    @click="confirmRemoveLibrary(library)">
                    <template #prepend>
                      <JIcon class="i-mdi:delete" />
                    </template>
                  </VListItem>
                </VList>
              </VMenu>
              <button
                type="button"
                class="library-card-text library-card-name"
                @click="openEditDialog(library)">
                {{ library.Name }}
              </button>
              <div class="library-card-text">
                {{ collectionTypeLabel(library.CollectionType) }}
              </div>
              <div
                class="library-card-text"
                :class="{ 'library-card-warning': !libraryPathCount(library) }">
                {{ libraryLocationSummary(library) }}
              </div>
            </div>
          </article>

          <button
            type="button"
            class="library-card library-card-add"
            @click="openCreateDialog">
            <div class="library-card-visual">
              <JIcon class="i-mdi:plus-circle library-card-add-icon" />
            </div>
            <div class="library-card-footer">
              <div class="library-card-text library-card-name">
                {{ t('add') }} {{ t('libraries') }}
              </div>
              <div class="library-card-text">&nbsp;</div>
              <div class="library-card-text">&nbsp;</div>
            </div>
          </button>
        </div>

        <div
          v-if="!libraries.length"
          class="uno-mt-4 text-medium-emphasis">
          {{ t('noLibrariesYet') }}
        </div>
      </VCol>

      <VDialog
        v-model="editDialog"
        max-width="920">
        <VCard v-if="selectedLibrary">
          <VCardTitle class="uno-flex uno-items-center uno-justify-between uno-gap-4">
            <span>{{ selectedLibrary.Name }}</span>
            <VSwitch
              v-model="editAdvanced"
              color="primary"
              hide-details
              inset
              :label="t('showAdvancedSettings')" />
          </VCardTitle>
          <VCardText>
            <VRow>
              <VCol
                cols="12"
                md="6">
                <VTextField
                  v-model="selectedLibrary._draftName"
                  :label="t('libraryName')"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
              <VCol
                cols="12"
                md="6">
                <VSelect
                  :model-value="selectedLibrary.CollectionType || 'mixed'"
                  :label="t('contentType')"
                  density="comfortable"
                  variant="outlined"
                  :items="collectionTypes"
                  item-title="title"
                  item-value="value"
                  readonly />
              </VCol>
            </VRow>

            <div class="uno-mt-4">
              <div class="uno-mb-2 uno-text-sm uno-font-medium">
                {{ t('paths') }}
              </div>
              <VList
                density="comfortable"
                class="uno-border uno-rounded">
                <VListItem
                  v-for="pathInfo in selectedLibrary.LibraryOptions.PathInfos"
                  :key="`${selectedLibrary.ItemId}-${pathInfo.Path}`"
                  :title="pathInfo.Path"
                  :subtitle="pathInfo.NetworkPath || undefined">
                  <template #prepend>
                    <JIcon class="i-mdi:folder" />
                  </template>
                  <template #append>
                    <VBtn
                      icon
                      variant="text"
                      color="error"
                      @click="removeLibraryPath(selectedLibrary, pathInfo.Path)">
                      <JIcon class="i-mdi:remove-circle" />
                    </VBtn>
                  </template>
                </VListItem>
                <VListItem v-if="!selectedLibrary.LibraryOptions.PathInfos.length">
                  <VListItemTitle>{{ t('noPaths') }}</VListItemTitle>
                </VListItem>
              </VList>
              <VRow class="uno-mt-3">
                <VCol
                  cols="12"
                  md="5">
                  <VTextField
                    v-model="selectedLibrary._newPath"
                    :label="t('folderPath')"
                    density="comfortable"
                    variant="outlined"
                    hide-details />
                </VCol>
                <VCol
                  cols="12"
                  md="5">
                  <VTextField
                    v-model="selectedLibrary._newNetworkPath"
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
                    @click="addLibraryPath(selectedLibrary)">
                    {{ t('add') }}
                  </VBtn>
                </VCol>
              </VRow>
            </div>

            <LibraryOptionsFields
              v-model="selectedLibrary.LibraryOptions"
              :collection-type="selectedLibrary.CollectionType"
              :advanced-visible="editAdvanced" />
            <LibraryTypeOptionsFields
              v-model="selectedLibrary.LibraryOptions"
              :advanced-visible="editAdvanced" />
          </VCardText>
          <VCardActions>
            <VBtn
              color="error"
              variant="text"
              @click="confirmRemoveLibrary(selectedLibrary)">
              {{ t('remove') }}
            </VBtn>
            <VSpacer />
            <VBtn
              variant="text"
              @click="closeEditDialog">
              {{ t('cancel') }}
            </VBtn>
            <VBtn
              color="primary"
              variant="elevated"
              @click="saveSelectedLibrary">
              {{ t('save') }}
            </VBtn>
          </VCardActions>
        </VCard>
      </VDialog>

      <VDialog
        v-model="createDialog"
        max-width="920">
        <VCard>
          <VCardTitle class="uno-flex uno-items-center uno-justify-between uno-gap-4">
            <span>{{ t('createLibrary') }}</span>
            <VSwitch
              v-model="createAdvanced"
              color="primary"
              hide-details
              inset
              :label="t('showAdvancedSettings')" />
          </VCardTitle>
          <VCardText>
            <VRow>
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
                <div
                  v-if="createCollectionTypeDescription"
                  class="uno-mt-2 uno-text-sm text-medium-emphasis">
                  {{ createCollectionTypeDescription }}
                </div>
              </VCol>
              <VCol
                cols="12"
                md="6">
                <VTextField
                  v-model="createForm.name"
                  :label="t('libraryName')"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
            </VRow>

            <div class="uno-mt-4">
              <div class="uno-mb-2 uno-flex uno-items-center uno-justify-between uno-gap-3">
                <div class="uno-text-sm uno-font-medium">
                  {{ t('paths') }}
                </div>
                <VBtn
                  size="small"
                  variant="outlined"
                  @click="createPathDialog = true">
                  {{ t('add') }}
                </VBtn>
              </div>
              <VList
                density="comfortable"
                class="uno-border uno-rounded">
                <VListItem
                  v-for="pathInfo in createForm.pathInfos"
                  :key="`${pathInfo.Path}-${pathInfo.NetworkPath ?? ''}`"
                  :title="pathInfo.Path"
                  :subtitle="pathInfo.NetworkPath || undefined">
                  <template #append>
                    <VBtn
                      variant="text"
                      color="error"
                      @click="removeCreatePath(pathInfo.Path)">
                      {{ t('remove') }}
                    </VBtn>
                  </template>
                </VListItem>
                <VListItem v-if="!createForm.pathInfos.length">
                  <VListItemTitle>{{ t('noPaths') }}</VListItemTitle>
                </VListItem>
              </VList>
              <div
                v-if="selectablePathOptions.length"
                class="uno-mt-3">
                <div class="uno-mb-2 uno-text-sm text-medium-emphasis">
                  {{ t('selectableMediaFolders') }}
                </div>
                <div class="uno-flex uno-flex-wrap uno-gap-2">
                  <VChip
                    v-for="pathInfo in selectablePathOptions.slice(0, 12)"
                    :key="`preset-${pathInfo.Path}`"
                    size="small"
                    variant="outlined"
                    @click="appendCreatePath(pathInfo)">
                    {{ pathInfo.Path }}
                  </VChip>
                </div>
              </div>
            </div>
            <LibraryOptionsFields
              v-model="createForm.libraryOptions"
              :collection-type="createForm.collectionType"
              :advanced-visible="createAdvanced" />
            <LibraryTypeOptionsFields
              v-model="createForm.libraryOptions"
              :advanced-visible="createAdvanced" />
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

      <VDialog
        v-model="createPathDialog"
        max-width="760">
        <VCard>
          <VCardTitle>{{ t('add') }} {{ t('paths') }}</VCardTitle>
          <VCardText>
            <VRow>
              <VCol
                cols="12"
                md="6">
                <VTextField
                  v-model="createForm.path"
                  :label="t('folderPath')"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
              <VCol
                cols="12"
                md="6">
                <VTextField
                  v-model="createForm.networkPath"
                  :label="t('networkPath')"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
            </VRow>
            <div
              v-if="selectablePathOptions.length"
              class="uno-mt-2">
              <div class="uno-mb-2 uno-text-sm text-medium-emphasis">
                {{ t('selectableMediaFolders') }}
              </div>
              <VList
                density="comfortable"
                class="uno-border uno-rounded">
                <VListItem
                  v-for="pathInfo in selectablePathOptions"
                  :key="`picker-${pathInfo.Path}`"
                  :title="pathInfo.Path">
                  <template #append>
                    <VBtn
                      variant="text"
                      color="primary"
                      @click="chooseSuggestedCreatePath(pathInfo)">
                      {{ t('add') }}
                    </VBtn>
                  </template>
                </VListItem>
              </VList>
            </div>
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn
              variant="text"
              @click="createPathDialog = false">
              {{ t('cancel') }}
            </VBtn>
            <VBtn
              color="primary"
              variant="elevated"
              @click="commitCreatePath">
              {{ t('add') }}
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
import { computed, defineComponent, h, ref, watch, type PropType } from 'vue';
import {
  VBtn,
  VCard,
  VCardActions,
  VCardText,
  VCardTitle,
  VCol,
  VCombobox,
  VDialog,
  VList,
  VListItem,
  VRow,
  VSelect,
  VSpacer,
  VSwitch,
  VTextField
} from 'vuetify/components';
import { useTranslation } from 'i18next-vue';
import { ImageType } from '@jellyfin/sdk/lib/generated-client';
import type {
  SettingsCountryInfo,
  SettingsCultureInfo,
  SettingsLibraryImageOption,
  SettingsLibraryAvailableTypeOptions,
  SettingsLibraryItemTypeOptions,
  SettingsLibraryOptionInfo,
  SettingsLibraryOptions,
  SettingsLibraryOptionsResult,
  SettingsMediaPathInfo,
  SettingsSelectableMediaFolder,
  SettingsVirtualFolderInfo
} from '#/composables/use-settings-sdk.ts';
import { useSettingsSdk } from '#/composables/use-settings-sdk.ts';
import { useSnackbar } from '#/composables/use-snackbar.ts';
import { useConfirmDialog } from '#/composables/use-confirm-dialog.ts';
import { getItemImageUrl } from '#/utils/images.ts';

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
  | 'SubtitleFetcherOrder'
  | 'SubtitleDownloadLanguages'
  | 'SkipSubtitlesIfEmbeddedSubtitlesPresent'
  | 'SkipSubtitlesIfAudioTrackMatches'
  | 'RequirePerfectSubtitleMatch'
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
  PrimaryImageItemId?: string;
  LibraryOptions: LibraryOptionsDto;
  _draftName: string;
  _newPath: string;
  _newNetworkPath: string;
}

interface SelectableMediaFolderDto extends SettingsSelectableMediaFolder {}

const { t } = useTranslation();
const { librariesApi, localizationApi } = useSettingsSdk();
const libraries = ref<VirtualFolderInfoDto[]>([]);
const selectableFolders = ref<SelectableMediaFolderDto[]>([]);
const availableLibraryOptions = ref<SettingsLibraryOptionsResult>({});
const cultureOptions = ref<SettingsCultureInfo[]>([]);
const countryOptions = ref<SettingsCountryInfo[]>([]);
const loading = ref(false);
const errorMessage = ref('');
const createDialog = ref(false);
const createAdvanced = ref(false);
const createPathDialog = ref(false);
const editDialog = ref(false);
const editAdvanced = ref(true);
const selectedLibrary = ref<VirtualFolderInfoDto>();
const lastAutoName = ref('');
const collectionTypes = computed(() => [
  { title: t('movies'), value: 'movies' },
  { title: t('shows'), value: 'tvshows' },
  { title: t('music'), value: 'music' },
  { title: t('musicVideos'), value: 'musicvideos' },
  { title: t('homeVideos'), value: 'homevideos' },
  { title: t('books'), value: 'books' },
  { title: t('mixedContent'), value: 'mixed' }
]);
const collectionTypeDescriptions = computed<Record<string, string>>(() => ({
  movies: '电影资料将按电影库方式整理和抓取元数据。',
  tvshows: '电视剧资料将按剧集与季的结构整理。',
  music: '音乐资料将启用音乐专辑、艺术家与歌词相关选项。',
  musicvideos: '音乐视频资料会按视频媒体库方式整理。',
  homevideos: '家庭视频资料适合个人录像和普通视频文件。',
  books: '图书资料会按书籍媒体库方式整理。',
  mixed: '混合内容库适合暂不区分媒体类型的文件夹。'
}));
const createCollectionTypeDescription = computed(() =>
  collectionTypeDescriptions.value[createForm.value.collectionType] ?? ''
);
const selectablePathOptions = computed(() => selectableFolders.value.flatMap((folder) => {
  const options: MediaPathInfoDto[] = [];

  for (const subFolder of folder.SubFolders ?? []) {
    options.push({
      Path: subFolder.Path || subFolder.Id,
      NetworkPath: null
    });
  }

  return options.filter(pathInfo => pathInfo.Path);
}));

const createForm = ref({
  name: '',
  collectionType: 'movies',
  pathInfos: [] as MediaPathInfoDto[],
  path: '',
  networkPath: '',
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
    },
    advancedVisible: {
      type: Boolean,
      default: true
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
    const showAdvanced = props.advancedVisible;
    const metadataSaverItems = computed(() => libraryOptionNames(availableLibraryOptions.value.MetadataSavers, props.modelValue.MetadataSavers));
    const metadataReaderItems = computed(() => libraryOptionNames(availableLibraryOptions.value.MetadataReaders, [
      ...props.modelValue.LocalMetadataReaderOrder,
      ...props.modelValue.DisabledLocalMetadataReaders
    ]));
    const subtitleFetcherItems = computed(() => libraryOptionNames(availableLibraryOptions.value.SubtitleFetchers, props.modelValue.DisabledSubtitleFetchers));
    const lyricsFetcherItems = computed(() => libraryOptionNames(availableLibraryOptions.value.LyricsFetchers, props.modelValue.DisabledLyricsFetchers));
    const subtitleLanguageItems = computed(() => cultureOptions.value
      .filter(culture => culture.ThreeLetterISOLanguageName)
      .map(culture => ({
        title: culture.DisplayName || culture.Name || culture.ThreeLetterISOLanguageName!,
        value: culture.ThreeLetterISOLanguageName!.toLowerCase()
      })));
    const showSubtitleDownloads = showMovieLike && Boolean(availableLibraryOptions.value.SubtitleFetchers?.length);
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
        showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(
          cultureOptions.value.length ? VSelect : VTextField,
          cultureOptions.value.length
            ? {
              modelValue: props.modelValue.PreferredMetadataLanguage,
              label: t('metadataLanguage'),
              density: 'comfortable',
              variant: 'outlined',
              items: cultureOptions.value,
              itemTitle: 'DisplayName',
              itemValue: 'TwoLetterISOLanguageName',
              'onUpdate:modelValue': (value: string) => update('PreferredMetadataLanguage', value || null)
            }
            : {
              modelValue: props.modelValue.PreferredMetadataLanguage,
              label: t('metadataLanguage'),
              density: 'comfortable',
              variant: 'outlined',
              'onUpdate:modelValue': (value: string) => update('PreferredMetadataLanguage', value || null)
            }
        )),
        showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
          modelValue: props.modelValue.PreferredImageLanguage,
          label: t('preferredImageLanguage'),
          density: 'comfortable',
          variant: 'outlined',
          'onUpdate:modelValue': (value: string) => update('PreferredImageLanguage', value || null)
        })),
        showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(
          countryOptions.value.length ? VSelect : VTextField,
          countryOptions.value.length
            ? {
              modelValue: props.modelValue.MetadataCountryCode,
              label: t('countryCode'),
              density: 'comfortable',
              variant: 'outlined',
              items: countryOptions.value,
              itemTitle: 'DisplayName',
              itemValue: 'TwoLetterISORegionName',
              'onUpdate:modelValue': (value: string) => update('MetadataCountryCode', value || null)
            }
            : {
              modelValue: props.modelValue.MetadataCountryCode,
              label: t('countryCode'),
              density: 'comfortable',
              variant: 'outlined',
              'onUpdate:modelValue': (value: string) => update('MetadataCountryCode', value || null)
            }
        )),
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
        showAdvanced && showMetadata && h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
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
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('EnableRealtimeMonitor', t('realtimeMonitor'))),
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('CacheImages', t('cacheImages'))),
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('ExcludeFromSearch', t('excludeFromSearch'))),
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('IgnoreHiddenFiles', t('ignoreHiddenFiles'))),
        showMetadata && h(VCol, { cols: '12', md: '4' }, () => field('EnableInternetProviders', t('downloadInternetMetadata'))),
        showAdvanced && showMetadata && h(VCol, { cols: '12', md: '4' }, () => field('DownloadImagesInAdvance', t('downloadImagesInAdvance'))),
        props.collectionType !== 'photos' && h(VCol, { cols: '12', md: '4' }, () => field('SaveLocalMetadata', t('saveLocalMetadata'))),
        showAdvanced && props.collectionType !== 'photos' && h(VCol, { cols: '12', md: '4' }, () => field('SaveMetadataHidden', t('saveMetadataHidden'))),
        showAdvanced && props.collectionType !== 'photos' && h(VCol, { cols: '12', md: '4' }, () => field('SaveLocalThumbnailSets', t('saveLocalThumbnailSets'))),
        showAdvanced && showTv && h(VCol, { cols: '12', md: '4' }, () => field('ImportMissingEpisodes', t('importMissingEpisodes'))),
        showAdvanced && showTv && h(VCol, { cols: '12', md: '4' }, () => field('EnableAutomaticSeriesGrouping', t('automaticallyGroupSeries'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMarkerDetection', t('enableMarkerDetection'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMarkerDetectionDuringLibraryScan', t('enableMarkerDetectionDuringLibraryScan'))),
        showChapters && h(VCol, { cols: '12', md: '4' }, () => field('EnableChapterImageExtraction', t('extractChapterImages'))),
        showAdvanced && showChapters && h(VCol, { cols: '12', md: '4' }, () => field('ExtractChapterImagesDuringLibraryScan', t('extractChaptersDuringScan'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('AutoGenerateChapters', t('autoGenerateChapters'))),
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('EnableEmbeddedTitles', t('useEmbeddedTitles'))),
        showAdvanced && showTv && h(VCol, { cols: '12', md: '4' }, () => field('EnableEmbeddedEpisodeInfos', t('useEmbeddedEpisodeInfo'))),
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('EnableArchiveMediaFiles', t('archiveMediaFiles'))),
        showAdvanced && showMusic && h(VCol, { cols: '12', md: '4' }, () => field('ImportPlaylists', t('importPlaylists'))),
        showAdvanced && showMusic && h(VCol, { cols: '12', md: '4' }, () => field('ShareEmbeddedMusicAlbumImages', t('shareEmbeddedMusicAlbumImages'))),
        showAdvanced && showMusic && h(VCol, { cols: '12', md: '4' }, () => field('EnableAudioResume', t('enableAudioResume'))),
        showAdvanced && showMusic && h(VCol, { cols: '12', md: '4' }, () => field('SaveLyricsWithMedia', t('saveLyricsWithMedia'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('SaveSubtitlesWithMedia', t('saveSubtitlesWithMedia'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('MergeTopLevelFolders', t('mergeTopLevelFolders'))),
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('CollapseSingleItemFolders', t('collapseSingleItemFolders'))),
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('ForceCollapseSingleItemFolders', t('forceCollapseSingleItemFolders'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('ImportCollections', t('importCollections'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMultiVersionByFiles', t('enableMultiVersionByFiles'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMultiVersionByMetadata', t('enableMultiVersionByMetadata'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => field('EnableMultiPartItems', t('enableMultiPartItems'))),
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
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => h(VCombobox, {
          modelValue: props.modelValue.LocalMetadataReaderOrder,
          label: t('localMetadataReaderOrder'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: metadataReaderItems.value,
          'onUpdate:modelValue': (value: string[]) => update('LocalMetadataReaderOrder', value)
        })),
        showAdvanced && h(VCol, { cols: '12', md: '4' }, () => h(VCombobox, {
          modelValue: props.modelValue.DisabledLocalMetadataReaders,
          label: t('disabledLocalMetadataReaders'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: metadataReaderItems.value,
          'onUpdate:modelValue': (value: string[]) => update('DisabledLocalMetadataReaders', value)
        })),
        showAdvanced && h(VCol, { cols: '12', md: '6' }, () => h(VCombobox, {
          modelValue: props.modelValue.IgnoreFileExtensions,
          label: t('ignoreFileExtensions'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          'onUpdate:modelValue': (value: string[]) => update('IgnoreFileExtensions', value)
        })),
        showAdvanced && showMusic && h(VCol, { cols: '12', md: '6' }, () => h(VCombobox, {
          modelValue: props.modelValue.DisabledLyricsFetchers,
          label: t('disabledLyricsFetchers'),
          density: 'comfortable',
          variant: 'outlined',
          multiple: true,
          chips: true,
          items: lyricsFetcherItems.value,
          'onUpdate:modelValue': (value: string[]) => update('DisabledLyricsFetchers', value)
        })),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => optionalNumber('MinResumePct', t('minResumePct'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => optionalNumber('MaxResumePct', t('maxResumePct'))),
        showAdvanced && showMovieLike && h(VCol, { cols: '12', md: '4' }, () => optionalNumber('MinResumeDurationSeconds', t('minResumeDurationSeconds')))
      ]),
      showSubtitleDownloads
        ? h('div', { class: 'uno-mt-6 uno-space-y-4' }, [
          h('div', { class: 'uno-text-base uno-font-medium' }, t('subtitleDownloads')),
          h(VRow, {}, () => [
            h(VCol, { cols: '12' }, () => h(VCombobox, {
              modelValue: props.modelValue.SubtitleDownloadLanguages,
              label: t('subtitleDownloadLanguages'),
              density: 'comfortable',
              variant: 'outlined',
              multiple: true,
              chips: true,
              items: subtitleLanguageItems.value,
              itemTitle: 'title',
              itemValue: 'value',
              'onUpdate:modelValue': (value: string[]) => update('SubtitleDownloadLanguages', value)
            })),
            h(VCol, { cols: '12', md: '4' }, () => field('RequirePerfectSubtitleMatch', t('requirePerfectSubtitleMatch'))),
            showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('SkipSubtitlesIfAudioTrackMatches', t('skipSubtitlesIfAudioTrackMatches'))),
            showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('SkipSubtitlesIfEmbeddedSubtitlesPresent', t('skipSubtitlesIfEmbeddedSubtitlesPresent'))),
            showAdvanced && h(VCol, { cols: '12', md: '4' }, () => field('SaveSubtitlesWithMedia', t('saveSubtitlesWithMedia'))),
            showAdvanced && h(VCol, { cols: '12', md: '8' }, () => h(VCombobox, {
              modelValue: props.modelValue.DisabledSubtitleFetchers,
              label: t('disabledSubtitleFetchers'),
              density: 'comfortable',
              variant: 'outlined',
              multiple: true,
              chips: true,
              items: subtitleFetcherItems.value,
              'onUpdate:modelValue': (value: string[]) => update('DisabledSubtitleFetchers', value)
            }))
          ])
        ])
        : null
    ]);
  }
});

const LibraryTypeOptionsFields = defineComponent({
  props: {
    modelValue: {
      type: Object as PropType<LibraryOptionsDto>,
      required: true
    },
    advancedVisible: {
      type: Boolean,
      default: true
    }
  },
  emits: ['update:modelValue'],
  setup(props, { emit }) {
    const imageOptionsDialog = ref(false);
    const imageOptionsType = ref<string | null>(null);
    const updateTypeOptions = (nextTypeOptions: SettingsLibraryItemTypeOptions[]): void => {
      emit('update:modelValue', {
        ...props.modelValue,
        TypeOptions: nextTypeOptions
      });
    };

    const reorder = (
      items: string[],
      index: number,
      direction: -1 | 1
    ): string[] => {
      const next = [...items];
      const target = index + direction;

      if (target < 0 || target >= next.length) {
        return next;
      }

      [next[index], next[target]] = [next[target], next[index]];
      return next;
    };

    const setChecked = (
      availableType: SettingsLibraryAvailableTypeOptions,
      field: 'MetadataFetchers' | 'ImageFetchers',
      name: string,
      checked: boolean
    ): void => {
      const type = availableType.Type || 'default';
      const next = normalizeTypeOptions(props.modelValue.TypeOptions, availableLibraryOptions.value.TypeOptions);
      const target = ensureLibraryTypeOption({ ...props.modelValue, TypeOptions: next } as LibraryOptionsDto, type);
      const values = new Set((target[field] ?? []) as string[]);

      if (checked) {
        values.add(name);
      } else {
        values.delete(name);
      }

      target[field] = [...values];
      updateTypeOptions(next);
    };

    const move = (
      availableType: SettingsLibraryAvailableTypeOptions,
      field: 'MetadataFetcherOrder' | 'ImageFetcherOrder',
      index: number,
      direction: -1 | 1
    ): void => {
      const type = availableType.Type || 'default';
      const next = normalizeTypeOptions(props.modelValue.TypeOptions, availableLibraryOptions.value.TypeOptions);
      const target = ensureLibraryTypeOption({ ...props.modelValue, TypeOptions: next } as LibraryOptionsDto, type);
      target[field] = reorder([...(target[field] ?? [])], index, direction);
      updateTypeOptions(next);
    };

    const updateImageOptions = (type: string, nextImageOptions: SettingsLibraryImageOption[]): void => {
      const next = normalizeTypeOptions(props.modelValue.TypeOptions, availableLibraryOptions.value.TypeOptions);
      const target = ensureLibraryTypeOption({ ...props.modelValue, TypeOptions: next } as LibraryOptionsDto, type);
      target.ImageOptions = nextImageOptions;
      updateTypeOptions(next);
    };

    const itemsForType = (
      availableType: SettingsLibraryAvailableTypeOptions,
      field: 'MetadataFetchers' | 'ImageFetchers',
      orderField: 'MetadataFetcherOrder' | 'ImageFetcherOrder'
    ): Array<{ name: string; checked: boolean }> => {
      const type = availableType.Type || 'default';
      const normalized = normalizeTypeOptions(props.modelValue.TypeOptions, availableLibraryOptions.value.TypeOptions);
      const target = normalized.find(option => (option.Type || 'default') === type);
      const optionMap = new Map(
        typeOptionNames(field === 'MetadataFetchers' ? availableType.MetadataFetchers : availableType.ImageFetchers)
          .map(name => [name, name])
      );
      const ordered = target?.[orderField]?.filter((name): name is string => optionMap.has(name)) ?? [];
      const rest = [...optionMap.keys()].filter(name => !ordered.includes(name));
      const names = [...ordered, ...rest];
      const checkedValues = new Set((target?.[field] ?? []) as string[]);

      return names.map(name => ({
        name,
        checked: checkedValues.has(name)
      }));
    };

    const openImageOptions = (type: string): void => {
      imageOptionsType.value = type;
      imageOptionsDialog.value = true;
    };

    return () => {
      const types = availableLibraryOptions.value.TypeOptions ?? [];
      const selectedType = types.find(type => (type.Type || 'default') === (imageOptionsType.value || 'default'));
      const selectedTypeOptions = selectedType
        ? normalizeImageOptions(
          ensureLibraryTypeOption(
            { ...props.modelValue, TypeOptions: normalizeTypeOptions(props.modelValue.TypeOptions, availableLibraryOptions.value.TypeOptions) } as LibraryOptionsDto,
            selectedType.Type || 'default'
          ).ImageOptions,
          selectedType
        )
        : [];

      if (!types.length) {
        return null;
      }

      return h('div', { class: 'uno-mt-6 uno-space-y-4' }, [
        ...types.map((availableType) => {
          const metadataItems = itemsForType(availableType, 'MetadataFetchers', 'MetadataFetcherOrder');
          const imageItems = itemsForType(availableType, 'ImageFetchers', 'ImageFetcherOrder');

          return h('div', {
            class: 'uno-border uno-rounded uno-p-4'
          }, [
            h('div', { class: 'uno-mb-3 uno-text-sm uno-font-medium' }, availableType.Type || t('contentType')),
            metadataItems.length
              ? h('div', { class: 'uno-mb-4' }, [
                h('div', { class: 'uno-mb-2 uno-text-sm text-medium-emphasis' }, t('metadataFetchers')),
                h(VList, { density: 'compact', class: 'uno-border uno-rounded' }, () =>
                  metadataItems.map((item, index) => h(VListItem, {
                    key: `metadata-${availableType.Type}-${item.name}`,
                    title: item.name
                  }, {
                    prepend: () => h(VSwitch, {
                      modelValue: item.checked,
                      color: 'primary',
                      inset: true,
                      hideDetails: true,
                      'onUpdate:modelValue': (value: boolean) =>
                        setChecked(availableType, 'MetadataFetchers', item.name, value)
                    }),
                    append: () => h('div', { class: 'uno-flex uno-gap-1' }, [
                      h(VBtn, {
                        size: 'small',
                        variant: 'text',
                        icon: 'mdi-arrow-up',
                        disabled: index === 0,
                        onClick: () => move(availableType, 'MetadataFetcherOrder', index, -1)
                      }),
                      h(VBtn, {
                        size: 'small',
                        variant: 'text',
                        icon: 'mdi-arrow-down',
                        disabled: index === metadataItems.length - 1,
                        onClick: () => move(availableType, 'MetadataFetcherOrder', index, 1)
                      })
                    ])
                  }))
                )
              ])
              : null,
            props.advancedVisible && imageItems.length
              ? h('div', [
                h('div', { class: 'uno-mb-2 uno-flex uno-items-center uno-justify-between uno-gap-2' }, [
                  h('div', { class: 'uno-text-sm text-medium-emphasis' }, t('imageFetchers')),
                  h(VBtn, {
                    size: 'small',
                    variant: 'outlined',
                    disabled: !(availableType.SupportedImageTypes?.length),
                    onClick: () => openImageOptions(availableType.Type || 'default')
                  }, () => t('fetcherSettings'))
                ]),
                h(VList, { density: 'compact', class: 'uno-border uno-rounded' }, () =>
                  imageItems.map((item, index) => h(VListItem, {
                    key: `image-${availableType.Type}-${item.name}`,
                    title: item.name,
                    subtitle: availableType.SupportedImageTypes?.join(', ') || undefined
                  }, {
                    prepend: () => h(VSwitch, {
                      modelValue: item.checked,
                      color: 'primary',
                      inset: true,
                      hideDetails: true,
                      'onUpdate:modelValue': (value: boolean) =>
                        setChecked(availableType, 'ImageFetchers', item.name, value)
                    }),
                    append: () => h('div', { class: 'uno-flex uno-gap-1' }, [
                      h(VBtn, {
                        size: 'small',
                        variant: 'text',
                        icon: 'mdi-arrow-up',
                        disabled: index === 0,
                        onClick: () => move(availableType, 'ImageFetcherOrder', index, -1)
                      }),
                      h(VBtn, {
                        size: 'small',
                        variant: 'text',
                        icon: 'mdi-arrow-down',
                        disabled: index === imageItems.length - 1,
                        onClick: () => move(availableType, 'ImageFetcherOrder', index, 1)
                      })
                    ])
                  }))
                )
              ])
              : null
          ]);
        }),
        h(VDialog, {
          modelValue: imageOptionsDialog.value,
          maxWidth: '760',
          'onUpdate:modelValue': (value: boolean) => {
            imageOptionsDialog.value = value;
            if (!value) {
              imageOptionsType.value = null;
            }
          }
        }, () => selectedType ? h(VCard, {}, {
          default: () => [
            h(VCardTitle, {}, () => t('imageFetchOptions')),
            h(VCardText, {}, () => [
              h(VRow, {}, () => selectedTypeOptions.map((option) => {
                const imageType = option.Type || '';
                const title = imageTypeLabel(imageType);
                const isBackdrop = imageType === 'Backdrop';
                const isScreenshot = imageType === 'Screenshot';

                return h(VCol, { cols: '12', md: isBackdrop || isScreenshot ? '12' : '6' }, () => h('div', {
                  class: 'uno-border uno-rounded uno-p-4'
                }, [
                  h(VSwitch, {
                    modelValue: Number(option.Limit ?? 0) > 0,
                    color: 'primary',
                    inset: true,
                    label: title,
                    'onUpdate:modelValue': (value: boolean) => {
                      updateImageOptions(selectedType.Type || 'default', selectedTypeOptions.map(item =>
                        (item.Type || '') === imageType
                          ? { ...item, Limit: value ? Math.max(1, Number(item.Limit ?? 1)) : 0 }
                          : item
                      ));
                    }
                  }),
                  (isBackdrop || isScreenshot) && Number(option.Limit ?? 0) > 0
                    ? h(VRow, { class: 'uno-mt-2' }, () => [
                      h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
                        modelValue: option.Limit ?? 0,
                        label: isBackdrop ? t('maxBackdropsPerItem') : t('maxScreenshotsPerItem'),
                        density: 'comfortable',
                        variant: 'outlined',
                        type: 'number',
                        min: 0,
                        'onUpdate:modelValue': (value: string) => {
                          const numericValue = Math.max(0, Number(value) || 0);
                          updateImageOptions(selectedType.Type || 'default', selectedTypeOptions.map(item =>
                            (item.Type || '') === imageType
                              ? { ...item, Limit: numericValue }
                              : item
                          ));
                        }
                      })),
                      h(VCol, { cols: '12', md: '6' }, () => h(VTextField, {
                        modelValue: option.MinWidth ?? 0,
                        label: isBackdrop ? t('minBackdropDownloadWidth') : t('minScreenshotDownloadWidth'),
                        density: 'comfortable',
                        variant: 'outlined',
                        type: 'number',
                        min: 0,
                        'onUpdate:modelValue': (value: string) => {
                          const numericValue = Math.max(0, Number(value) || 0);
                          updateImageOptions(selectedType.Type || 'default', selectedTypeOptions.map(item =>
                            (item.Type || '') === imageType
                              ? { ...item, MinWidth: numericValue }
                              : item
                          ));
                        }
                      }))
                    ])
                    : null
                ]));
              }))
            ]),
            h(VCardActions, {}, () => [
              h(VSpacer),
              h(VBtn, {
                variant: 'text',
                onClick: () => {
                  imageOptionsDialog.value = false;
                  imageOptionsType.value = null;
                }
              }, () => t('close'))
            ])
          ]
        }) : null)
      ]);
    };
  }
});

function libraryOptionNames(options: SettingsLibraryOptionInfo[] | undefined, fallback: string[] = []): string[] {
  const values = [
    ...(options?.map(option => option.Name).filter((value): value is string => Boolean(value)) ?? []),
    ...fallback
  ];

  return [...new Set(values.filter(Boolean))];
}

function typeOptionNames(options: SettingsLibraryOptionInfo[] | undefined): string[] {
  return options?.map(option => option.Name).filter((value): value is string => Boolean(value)) ?? [];
}

function imageTypeLabel(type: string): string {
  const labels: Record<string, string> = {
    Primary: t('downloadPrimaryImage'),
    Art: t('downloadArtImage'),
    BoxRear: t('downloadBackImage'),
    Banner: t('downloadBannerImage'),
    Box: t('downloadBoxImage'),
    Disc: t('downloadDiscImage'),
    Logo: t('downloadLogoImage'),
    Menu: t('downloadMenuImage'),
    Thumb: t('downloadThumbImage'),
    Backdrop: t('downloadBackdropImage'),
    Screenshot: t('downloadScreenshotImage')
  };

  return labels[type] ?? type;
}

function normalizeImageOptions(
  imageOptions: SettingsLibraryImageOption[] | undefined,
  availableType: SettingsLibraryAvailableTypeOptions
): SettingsLibraryImageOption[] {
  const supportedTypes = availableType.SupportedImageTypes ?? [];
  const defaultOptions = availableType.DefaultImageOptions ?? [];
  const existing = new Map(
    (imageOptions ?? []).map(option => [option.Type || '', { ...option }])
  );

  return supportedTypes.map((type) => {
    const current = existing.get(type);
    const fallback = defaultOptions.find((option): option is SettingsLibraryImageOption =>
      typeof option === 'object' && option !== null && 'Type' in option && option.Type === type
    );

    return {
      Type: type,
      Limit: current?.Limit ?? fallback?.Limit ?? (type === 'Primary' ? 1 : 0),
      MinWidth: current?.MinWidth ?? fallback?.MinWidth ?? 0
    };
  });
}

function ensureLibraryTypeOption(
  options: LibraryOptionsDto,
  type: string
): SettingsLibraryItemTypeOptions {
  const normalized = type || 'default';
  options.TypeOptions ??= [];
  let result = options.TypeOptions.find(option => (option.Type || 'default') === normalized);

  if (!result) {
    result = {
      Type: normalized,
      MetadataFetchers: [],
      MetadataFetcherOrder: [],
      ImageFetchers: [],
      ImageFetcherOrder: [],
      ImageOptions: []
    };
    options.TypeOptions.push(result);
  }

  result.MetadataFetchers ??= [];
  result.MetadataFetcherOrder ??= [];
  result.ImageFetchers ??= [];
  result.ImageFetcherOrder ??= [];
  result.ImageOptions ??= [];
  return result;
}

function normalizeTypeOptions(
  options: SettingsLibraryItemTypeOptions[] | undefined,
  available: SettingsLibraryAvailableTypeOptions[] | undefined
): SettingsLibraryItemTypeOptions[] {
  const existing = [...(options ?? [])].map(option => ({
    ...option,
    MetadataFetchers: [...(option.MetadataFetchers ?? [])],
    MetadataFetcherOrder: [...(option.MetadataFetcherOrder ?? [])],
    ImageFetchers: [...(option.ImageFetchers ?? [])],
    ImageFetcherOrder: [...(option.ImageFetcherOrder ?? [])],
    ImageOptions: [...(option.ImageOptions ?? [])]
  }));

  for (const availableOption of available ?? []) {
    const type = availableOption.Type || 'default';
    const target = existing.find(option => (option.Type || 'default') === type) ?? (() => {
      const created: SettingsLibraryItemTypeOptions = {
        Type: type,
        MetadataFetchers: [],
        MetadataFetcherOrder: [],
        ImageFetchers: [],
        ImageFetcherOrder: [],
        ImageOptions: []
      };
      existing.push(created);
      return created;
    })();

    const metadataNames = typeOptionNames(availableOption.MetadataFetchers);
    const imageNames = typeOptionNames(availableOption.ImageFetchers);

    if (!target.MetadataFetcherOrder?.length) {
      target.MetadataFetcherOrder = metadataNames;
    }
    if (!target.ImageFetcherOrder?.length) {
      target.ImageFetcherOrder = imageNames;
    }
    if (!target.MetadataFetchers?.length) {
      target.MetadataFetchers = metadataNames.filter(name =>
        availableOption.MetadataFetchers?.find(option => option.Name === name)?.DefaultEnabled ?? true
      );
    }
    if (!target.ImageFetchers?.length) {
      target.ImageFetchers = imageNames.filter(name =>
        availableOption.ImageFetchers?.find(option => option.Name === name)?.DefaultEnabled ?? true
      );
    }
  }

  return existing;
}

function appendCreatePath(pathInfo: MediaPathInfoDto): void {
  const path = pathInfo.Path.trim();

  if (!path) {
    return;
  }

  if (createForm.value.pathInfos.some(item => item.Path.toLowerCase() === path.toLowerCase())) {
    return;
  }

  createForm.value.pathInfos.push({
    Path: path,
    NetworkPath: pathInfo.NetworkPath || null
  });
}

function removeCreatePath(path: string): void {
  createForm.value.pathInfos = createForm.value.pathInfos.filter(item => item.Path !== path);
}

function commitCreatePath(): void {
  appendCreatePath({
    Path: createForm.value.path,
    NetworkPath: createForm.value.networkPath || null
  });
  createForm.value.path = '';
  createForm.value.networkPath = '';
  createPathDialog.value = false;
}

function chooseSuggestedCreatePath(pathInfo: MediaPathInfoDto): void {
  appendCreatePath(pathInfo);
  createPathDialog.value = false;
}

function openEditDialog(library: VirtualFolderInfoDto): void {
  selectedLibrary.value = library;
  editAdvanced.value = false;
  editDialog.value = true;
}

function closeEditDialog(): void {
  editDialog.value = false;
  selectedLibrary.value = undefined;
}

function syncSelectedLibrary(itemId?: string): void {
  if (!itemId || selectedLibrary.value?.ItemId !== itemId) {
    return;
  }

  selectedLibrary.value = libraries.value.find(library => library.ItemId === itemId);
}

function collectionTypeLabel(collectionType?: string): string {
  return collectionTypes.value.find(item => item.value === (collectionType || 'mixed'))?.title ?? t('mixedContent');
}

function libraryPathCount(library: VirtualFolderInfoDto): number {
  return library.LibraryOptions.PathInfos.length || library.Locations.length;
}

function libraryLocationSummary(library: VirtualFolderInfoDto): string {
  const paths = library.LibraryOptions.PathInfos.map(pathInfo => pathInfo.Path).filter(Boolean);

  if (!paths.length) {
    return t('noPathsConfigured');
  }

  if (paths.length === 1) {
    return paths[0];
  }

  return `${paths.length} ${t('paths')}`;
}

function libraryIconClass(collectionType?: string): string {
  const icons: Record<string, string> = {
    movies: 'i-mdi:movie-open',
    music: 'i-mdi:music-box-multiple',
    photos: 'i-mdi:image-multiple',
    livetv: 'i-mdi:television-classic',
    tvshows: 'i-mdi:television-classic',
    games: 'i-mdi:gamepad-variant',
    trailers: 'i-mdi:movie-open',
    homevideos: 'i-mdi:video-vintage',
    musicvideos: 'i-mdi:video',
    books: 'i-mdi:book-open-page-variant',
    channels: 'i-mdi:folder',
    playlists: 'i-mdi:playlist-play',
    mixed: 'i-mdi:folder'
  };

  return icons[collectionType || 'mixed'] ?? 'i-mdi:folder';
}

function libraryPrimaryImageUrl(library: VirtualFolderInfoDto): string | undefined {
  return library.PrimaryImageItemId
    ? getItemImageUrl(library.PrimaryImageItemId, ImageType.Primary, {
        maxWidth: 600,
        quality: 90
      })
    : undefined;
}

function showChangeTypeNotice(library: VirtualFolderInfoDto): void {
  useSnackbar(`${library.Name}: ${t('contentType')}`, 'info');
}

function editLibraryImages(library: VirtualFolderInfoDto): void {
  if (!library.ItemId) {
    return;
  }

  window.location.hash = `#/metadata?itemId=${encodeURIComponent(library.ItemId)}`;
}

async function confirmRemoveLibrary(library: VirtualFolderInfoDto): Promise<void> {
  await useConfirmDialog(
    async () => {
      await removeLibrary(library);
    },
    {
      title: t('deleteConfirm'),
      text: `${t('remove')} ${library.Name}?`,
      confirmText: t('delete'),
      confirmColor: 'error'
    }
  );
}

async function saveSelectedLibrary(): Promise<void> {
  if (!selectedLibrary.value) {
    return;
  }

  await saveLibrary(selectedLibrary.value);
  closeEditDialog();
}

function resetCreateForm(): void {
  createForm.value = {
    name: '',
    collectionType: 'movies',
    pathInfos: [],
    path: '',
    networkPath: '',
    libraryOptions: defaultLibraryOptions(availableLibraryOptions.value.DefaultLibraryOptions)
  };
  createAdvanced.value = false;
  lastAutoName.value = '';
}

async function refreshCreateAvailableOptions(): Promise<void> {
  availableLibraryOptions.value = await librariesApi.getLibrariesAvailableoptions({
    LibraryContentType: createForm.value.collectionType === 'mixed' ? '' : createForm.value.collectionType,
    IsNewLibrary: true
  });

  createForm.value.libraryOptions = normalizeOptions(createForm.value.libraryOptions, createForm.value.pathInfos.map(path => path.Path));
}

async function openCreateDialog(): Promise<void> {
  resetCreateForm();
  createDialog.value = true;
  await refreshCreateAvailableOptions();
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
    SubtitleFetcherOrder: [],
    SubtitleDownloadLanguages: [],
    SkipSubtitlesIfEmbeddedSubtitlesPresent: false,
    SkipSubtitlesIfAudioTrackMatches: false,
    RequirePerfectSubtitleMatch: false,
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
    TypeOptions: normalizeTypeOptions(options?.TypeOptions, availableLibraryOptions.value.TypeOptions),
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

watch(() => createForm.value.collectionType, async (value) => {
  const title = collectionTypes.value.find(item => item.value === value)?.title ?? '';

  if (!createForm.value.name || createForm.value.name === lastAutoName.value) {
    createForm.value.name = title;
    lastAutoName.value = title;
  }

  if (createDialog.value) {
    await refreshCreateAvailableOptions();
  }
});

async function loadLibraries(): Promise<void> {
  loading.value = true;
  errorMessage.value = '';

  try {
    const [virtualFolders, mediaFolders, availableOptions, cultures, countries] = await Promise.all([
      librariesApi.getLibraryVirtualfoldersQuery(),
      librariesApi.getLibrarySelectablemediafolders(),
      librariesApi.getLibrariesAvailableoptions(),
      localizationApi.getCultures(),
      localizationApi.getCountries()
    ]);

    availableLibraryOptions.value = availableOptions;
    cultureOptions.value = cultures;
    countryOptions.value = countries;
    if (!createForm.value.name && !createForm.value.pathInfos.length) {
      createForm.value.libraryOptions = defaultLibraryOptions(availableOptions.DefaultLibraryOptions);
    }
    libraries.value = virtualFolders.map(normalizeLibrary);
    selectableFolders.value = mediaFolders;
  } catch (error) {
    console.error(error);
    libraries.value = [];
    selectableFolders.value = [];
    availableLibraryOptions.value = {};
    cultureOptions.value = [];
    countryOptions.value = [];
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
    syncSelectedLibrary(library.ItemId);
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
    syncSelectedLibrary(library.ItemId);
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
    syncSelectedLibrary(library.ItemId);
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
    if (selectedLibrary.value?.ItemId === library.ItemId) {
      closeEditDialog();
    }
  } catch (error) {
    console.error(error);
    useSnackbar(t('failedToRemoveLibrary'), 'error');
  }
}

async function createLibrary(): Promise<void> {
  const pathInfos = createForm.value.pathInfos;

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
    resetCreateForm();
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

<style scoped>
.library-card-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 1.5rem;
  align-items: start;
}

.library-card {
  display: block;
  min-width: 0;
  color: inherit;
  text-align: start;
  background: transparent;
  border: 0;
}

.library-card-visual {
  position: relative;
  display: flex;
  width: 100%;
  aspect-ratio: 16 / 9;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  color: rgb(var(--v-theme-on-surface));
  cursor: pointer;
  background: rgba(var(--v-theme-surface-variant), 0.72);
  border: 1px solid rgba(var(--v-border-color), var(--v-border-opacity));
  border-radius: 6px;
}

.library-card-visual:hover {
  background: rgba(var(--v-theme-surface-variant), 0.92);
}

.library-card-image {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.library-card-icon {
  width: 4.8rem;
  height: 4.8rem;
  color: rgba(var(--v-theme-on-surface), 0.66);
}

.library-card-footer {
  position: relative;
  min-height: 4.6rem;
  padding-top: 0.55rem;
  padding-inline-end: 2.6rem;
}

.library-card-menu {
  position: absolute;
  top: 0.15rem;
  right: 0;
}

.library-card-text {
  display: block;
  width: 100%;
  min-height: 1.35rem;
  overflow: hidden;
  font-size: 0.875rem;
  line-height: 1.35rem;
  color: rgba(var(--v-theme-on-surface), 0.72);
  text-overflow: ellipsis;
  white-space: nowrap;
  background: transparent;
  border: 0;
}

.library-card-name {
  padding: 0;
  font-weight: 500;
  color: rgb(var(--v-theme-on-surface));
  cursor: pointer;
}

.library-card-warning {
  color: rgb(var(--v-theme-error));
}

.library-card-add {
  cursor: pointer;
}

.library-card-add-icon {
  width: 4.8rem;
  height: 4.8rem;
  color: rgba(var(--v-theme-on-surface), 0.54);
}
</style>
