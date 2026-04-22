<template>
  <SettingsPage>
    <template #title>
      {{ t('libraries') }}
    </template>
    <template #actions>
      <VBtn
        color="primary"
        variant="elevated"
        :loading="refreshing"
        @click="refreshLibraries">
        {{ t('refresh') }}
      </VBtn>
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

        <VList
          v-if="libraries.length"
          lines="three"
          class="uno-mb-4">
          <VListItem
            v-for="library in libraries"
            :key="library.ItemId"
            :to="library.ItemId ? `/library/${library.ItemId}` : undefined"
            :title="library.Name"
            :subtitle="getLibrarySubtitle(library)">
            <template #prepend>
              <VAvatar>
                <JIcon class="i-mdi:library-shelves" />
              </VAvatar>
            </template>
            <template #append>
              <VListItemAction>
                <VChip
                  size="small"
                  variant="outlined">
                  {{ library.CollectionType }}
                </VChip>
              </VListItemAction>
            </template>
          </VListItem>
        </VList>

        <VCard v-else>
          <VCardTitle>
            {{ t('libraries') }}
          </VCardTitle>
        </VCard>
      </VCol>
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

interface LibraryOptionsDto {
  PreferredMetadataLanguage?: string | null;
  MetadataCountryCode?: string | null;
}

interface VirtualFolderInfoDto {
  Name: string;
  CollectionType: string;
  ItemId: string;
  Locations: string[];
  LibraryOptions?: LibraryOptionsDto | null;
}

const { t } = useTranslation();
const libraries = ref<VirtualFolderInfoDto[]>([]);
const refreshing = ref(false);
const errorMessage = ref('');

const apiBase = computed(() => remote.sdk.api?.basePath ?? '');
const apiKey = computed(() => remote.auth.currentUserToken.value ?? '');

function buildUrl(path: string): string {
  const query = new URLSearchParams();

  if (apiKey.value) {
    query.set('api_key', apiKey.value);
  }

  return `${apiBase.value}${path}?${query.toString()}`;
}

function getLibrarySubtitle(library: VirtualFolderInfoDto): string {
  const details = [
    library.Locations?.filter(Boolean).join(' | '),
    [
      library.LibraryOptions?.PreferredMetadataLanguage,
      library.LibraryOptions?.MetadataCountryCode
    ].filter(Boolean).join(' / ')
  ].filter(Boolean);

  return details.join(' | ');
}

async function loadLibraries(): Promise<void> {
  if (!apiBase.value) {
    return;
  }

  errorMessage.value = '';

  try {
    const response = await fetch(buildUrl('/Library/VirtualFolders/Query'));

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }

    libraries.value = await response.json() as VirtualFolderInfoDto[];
  } catch (error) {
    console.error(error);
    libraries.value = [];
    errorMessage.value = t('unexpectedError');
  }
}

async function refreshLibraries(): Promise<void> {
  if (!apiBase.value) {
    return;
  }

  refreshing.value = true;

  try {
    const response = await fetch(buildUrl('/Library/Refresh'), {
      method: 'POST'
    });

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }

    useSnackbar(t('refresh'), 'success');
    await loadLibraries();
  } catch (error) {
    console.error(error);
    useSnackbar(t('unexpectedError'), 'error');
  } finally {
    refreshing.value = false;
  }
}

await loadLibraries();
</script>
