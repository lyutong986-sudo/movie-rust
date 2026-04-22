<template>
  <SettingsPage>
    <template #title>
      {{ t('apiKeys') }}
    </template>

    <template #actions>
      <VBtn
        variant="elevated"
        :loading="loading"
        @click="refreshApiKeys">
        Refresh
      </VBtn>
      <VBtn
        color="primary"
        variant="elevated"
        :loading="loading"
        @click="addingNewKey = true">
        {{ t('addNewKey') }}
      </VBtn>
      <VBtn
        v-if="apiKeys.length"
        color="error"
        variant="elevated"
        :loading="loading"
        @click="confirmRevoke = 'all'">
        {{ t('revokeAll') }}
      </VBtn>
    </template>

    <template #content>
      <VCol cols="12">
        <VTable>
          <thead>
            <tr>
              <th>Application</th>
              <th>Token</th>
              <th>Created</th>
              <th>Status</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="apiKey in apiKeys"
              :key="apiKey.AccessToken ?? undefined">
              <td>
                <div class="uno-font-medium">
                  {{ apiKey.AppName }}
                </div>
                <div class="uno-text-sm uno-opacity-70">
                  {{ apiKey.AppVersion ?? '-' }}
                </div>
              </td>
              <td>
                <code>{{ truncateToken(apiKey.AccessToken) }}</code>
              </td>
              <td>{{ formatCreated(apiKey.DateCreated) }}</td>
              <td>{{ apiKey.IsActive === false ? 'Expired' : 'Active' }}</td>
              <td class="uno-text-right">
                <VBtn
                  color="error"
                  :loading="loading"
                  @click="confirmRevoke = apiKey.AccessToken ?? undefined">
                  {{ t('revoke') }}
                </VBtn>
              </td>
            </tr>
            <tr v-if="!apiKeys.length">
              <td
                colspan="5"
                class="uno-opacity-70">
                No API keys created yet
              </td>
            </tr>
          </tbody>
        </VTable>
      </VCol>

      <AddApiKey
        :adding-new-key="addingNewKey"
        @close="addingNewKey = false"
        @key-added="
          async () => {
            addingNewKey = false;
            await refreshApiKeys();
          }
        " />

      <VDialog
        width="auto"
        :model-value="!isNil(confirmRevoke)"
        @update:model-value="confirmRevoke = undefined">
        <VCard>
          <VCardText>
            {{ t('revokeConfirm') }}
          </VCardText>
          <VCardActions>
            <VBtn
              color="primary"
              :loading="loading"
              @click="confirmRevocation">
              {{ t('confirm') }}
            </VBtn>
            <VBtn
              :loading="loading"
              @click="confirmRevoke = undefined">
              {{ t('cancel') }}
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
import type { AuthenticationInfo } from '@jellyfin/sdk/lib/generated-client';
import { getApiKeyApi } from '@jellyfin/sdk/lib/utils/api/api-key-api';
import { formatRelative, parseJSON } from 'date-fns';
import { ref } from 'vue';
import { useTranslation } from 'i18next-vue';
import { isNil } from '@jellyfin-vue/shared/validation';
import { remote } from '#/plugins/remote/index.ts';
import { useSnackbar } from '#/composables/use-snackbar.ts';
import { useDateFns } from '#/composables/use-datefns.ts';

const { t } = useTranslation();

const apiKeys = ref<AuthenticationInfo[]>([]);
const addingNewKey = ref(false);
const confirmRevoke = ref<string>();
const loading = ref(false);

function truncateToken(token?: string): string {
  if (!token) {
    return '-';
  }

  if (token.length <= 16) {
    return token;
  }

  return `${token.slice(0, 8)}...${token.slice(-6)}`;
}

function formatCreated(date?: string): string {
  return date
    ? useDateFns(formatRelative, parseJSON(date), new Date())
    : '-';
}

async function confirmRevocation(): Promise<void> {
  if (!confirmRevoke.value) {
    return;
  }

  await (confirmRevoke.value === 'all'
    ? revokeAllApiKeys()
    : revokeApiKey(confirmRevoke.value));

  confirmRevoke.value = undefined;
}

async function revokeApiKey(token: string): Promise<void> {
  loading.value = true;
  try {
    await remote.sdk.newUserApi(getApiKeyApi).revokeKey({ key: token });
    useSnackbar(t('revokeSuccess'), 'success');
    await refreshApiKeys();
  } catch (error) {
    console.error(error);
    useSnackbar(t('revokeFailure'), 'error');
  } finally {
    loading.value = false;
  }
}

async function revokeAllApiKeys(): Promise<void> {
  loading.value = true;
  try {
    for (const key of apiKeys.value) {
      if (key.AccessToken) {
        await remote.sdk.newUserApi(getApiKeyApi).revokeKey({ key: key.AccessToken });
      }
    }
    useSnackbar(t('revokeAllSuccess'), 'success');
    await refreshApiKeys();
  } catch (error) {
    console.error(error);
    useSnackbar(t('revokeAllFailure'), 'error');
  } finally {
    loading.value = false;
  }
}

async function refreshApiKeys(): Promise<void> {
  loading.value = true;
  try {
    apiKeys.value = (await remote.sdk.newUserApi(getApiKeyApi).getKeys()).data.Items ?? [];
  } catch (error) {
    apiKeys.value = [];
    console.error(error);
    useSnackbar(t('refreshKeysFailure'), 'error');
  } finally {
    loading.value = false;
  }
}

await refreshApiKeys();
</script>
