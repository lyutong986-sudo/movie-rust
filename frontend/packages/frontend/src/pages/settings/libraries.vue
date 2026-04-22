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
              <VCardTitle>Paths</VCardTitle>
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
                    {{ formatLocations(library.Locations) || 'No paths configured' }}
                  </div>
                </div>
                <VChip
                  size="small"
                  variant="outlined">
                  {{ library.CollectionType }}
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
                    label="Library Name"
                    density="comfortable"
                    variant="outlined" />
                </VCol>
                <VCol
                  cols="12"
                  md="6">
                  <VTextField
                    :model-value="library.CollectionType"
                    label="Type"
                    density="comfortable"
                    variant="outlined"
                    readonly />
                </VCol>
                <VCol
                  cols="12"
                  md="6">
                  <VTextField
                    v-model="library.LibraryOptions.PreferredMetadataLanguage"
                    label="Metadata Language"
                    density="comfortable"
                    variant="outlined" />
                </VCol>
                <VCol
                  cols="12"
                  md="6">
                  <VTextField
                    v-model="library.LibraryOptions.MetadataCountryCode"
                    label="Country Code"
                    density="comfortable"
                    variant="outlined" />
                </VCol>
                <VCol
                  cols="12"
                  md="4">
                  <VSwitch
                    v-model="library.LibraryOptions.Enabled"
                    label="Enabled"
                    color="primary"
                    inset />
                </VCol>
                <VCol
                  cols="12"
                  md="4">
                  <VSwitch
                    v-model="library.LibraryOptions.EnablePhotos"
                    label="Enable Photos"
                    color="primary"
                    inset />
                </VCol>
                <VCol
                  cols="12"
                  md="4">
                  <VSwitch
                    v-model="library.LibraryOptions.EnableRealtimeMonitor"
                    label="Realtime Monitor"
                    color="primary"
                    inset />
                </VCol>
              </VRow>

              <div class="uno-mt-4">
                <div class="uno-mb-2 uno-text-sm uno-font-medium">
                  Paths
                </div>
                <VList
                  density="comfortable"
                  class="uno-border uno-rounded">
                  <VListItem
                    v-for="path in library.Locations"
                    :key="`${library.ItemId}-${path}`"
                    :title="path">
                    <template #append>
                      <VBtn
                        variant="text"
                        color="error"
                        @click="removeLibraryPath(library, path)">
                        {{ t('remove') }}
                      </VBtn>
                    </template>
                  </VListItem>
                  <VListItem v-if="!library.Locations.length">
                    <VListItemTitle>No paths</VListItemTitle>
                  </VListItem>
                </VList>
                <div class="uno-mt-3 uno-flex uno-gap-2">
                  <VTextField
                    v-model="library._newPath"
                    label="New Path"
                    density="comfortable"
                    variant="outlined"
                    hide-details />
                  <VBtn
                    variant="outlined"
                    @click="addLibraryPath(library)">
                    {{ t('add') }}
                  </VBtn>
                </div>
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
        max-width="720">
        <VCard>
          <VCardTitle>Create Library</VCardTitle>
          <VCardText>
            <VRow>
              <VCol
                cols="12"
                md="6">
                <VTextField
                  v-model="createForm.name"
                  label="Library Name"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
              <VCol
                cols="12"
                md="6">
                <VSelect
                  v-model="createForm.collectionType"
                  label="Type"
                  density="comfortable"
                  variant="outlined"
                  :items="collectionTypes" />
              </VCol>
              <VCol cols="12">
                <VTextarea
                  v-model="createForm.pathsText"
                  label="Paths (one per line)"
                  rows="4"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
              <VCol
                cols="12"
                md="6">
                <VTextField
                  v-model="createForm.preferredMetadataLanguage"
                  label="Metadata Language"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
              <VCol
                cols="12"
                md="6">
                <VTextField
                  v-model="createForm.metadataCountryCode"
                  label="Country Code"
                  density="comfortable"
                  variant="outlined" />
              </VCol>
            </VRow>
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
import { computed, ref } from 'vue';
import { useTranslation } from 'i18next-vue';
import { useSnackbar } from '#/composables/use-snackbar.ts';
import { remote } from '#/plugins/remote/index.ts';

interface MediaPathInfoDto {
  Path: string;
}

interface LibraryOptionsDto {
  Enabled: boolean;
  EnablePhotos: boolean;
  EnableRealtimeMonitor: boolean;
  PreferredMetadataLanguage?: string | null;
  MetadataCountryCode?: string | null;
  PathInfos?: MediaPathInfoDto[];
}

interface VirtualFolderInfoDto {
  Name: string;
  CollectionType: string;
  ItemId: string;
  Locations: string[];
  LibraryOptions: LibraryOptionsDto;
  _draftName: string;
  _newPath: string;
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
const collectionTypes = ['movies', 'tvshows', 'music', 'musicvideos', 'homevideos', 'books'];

const createForm = ref({
  name: '',
  collectionType: 'movies',
  pathsText: '',
  preferredMetadataLanguage: '',
  metadataCountryCode: ''
});

const apiBase = computed(() => remote.sdk.api?.basePath ?? '');
const apiKey = computed(() => remote.auth.currentUserToken.value ?? '');

function buildUrl(path: string): string {
  const url = new URL(`${apiBase.value}${path}`);

  if (apiKey.value) {
    url.searchParams.set('api_key', apiKey.value);
  }

  return url.toString();
}

function normalizeLibrary(library: Omit<VirtualFolderInfoDto, '_draftName' | '_newPath'>): VirtualFolderInfoDto {
  return {
    ...library,
    _draftName: library.Name,
    _newPath: '',
    LibraryOptions: {
      Enabled: library.LibraryOptions?.Enabled ?? true,
      EnablePhotos: library.LibraryOptions?.EnablePhotos ?? true,
      EnableRealtimeMonitor: library.LibraryOptions?.EnableRealtimeMonitor ?? false,
      PreferredMetadataLanguage: library.LibraryOptions?.PreferredMetadataLanguage ?? '',
      MetadataCountryCode: library.LibraryOptions?.MetadataCountryCode ?? '',
      PathInfos: library.LibraryOptions?.PathInfos ?? library.Locations.map(path => ({ Path: path }))
    }
  };
}

function formatLocations(locations: string[]): string {
  return locations.filter(Boolean).join(' | ');
}

function parsePaths(text: string): string[] {
  return text
    .split(/\r?\n/g)
    .map(line => line.trim())
    .filter(Boolean);
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
      requestJson<Omit<VirtualFolderInfoDto, '_draftName' | '_newPath'>[]>('/Library/VirtualFolders/Query'),
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
        LibraryOptions: {
          ...library.LibraryOptions,
          PathInfos: library.Locations.map(path => ({ Path: path }))
        }
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

  try {
    await fetch(buildUrl('/Library/VirtualFolders/Paths'), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        Name: library.Name,
        Path: path
      })
    }).then(async (response) => {
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
    });

    library._newPath = '';
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
  const paths = parsePaths(createForm.value.pathsText);

  if (!createForm.value.name.trim() || !paths.length) {
    useSnackbar('Provide a name and at least one path', 'error');
    return;
  }

  try {
    const response = await fetch(buildUrl(`/Library/VirtualFolders?Name=${encodeURIComponent(createForm.value.name.trim())}&CollectionType=${encodeURIComponent(createForm.value.collectionType)}`), {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        LibraryOptions: {
          Enabled: true,
          EnablePhotos: true,
          EnableRealtimeMonitor: false,
          PreferredMetadataLanguage: createForm.value.preferredMetadataLanguage || null,
          MetadataCountryCode: createForm.value.metadataCountryCode || null,
          PathInfos: paths.map(path => ({ Path: path }))
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
      preferredMetadataLanguage: '',
      metadataCountryCode: ''
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
