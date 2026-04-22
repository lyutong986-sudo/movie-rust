<template>
  <SettingsPage>
    <template #title>
      {{ $t('serverSettings') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <h3 class="uno-mb-2 uno-text-lg uno-font-bold">
          {{ $t('serverSettingsGeneral') }}
        </h3>

        <VTextField
          v-model="serverSettings.ServerName"
          :label="$t('serverName')" />

        <VSelect
          v-model="serverSettings.UICulture"
          variant="outlined"
          :label="$t('preferredLanguage')"
          :rules="SomeItemSelectedRule"
          item-title="Name"
          item-value="Value"
          :items="culturesList" />

        <VCheckbox
          v-model="serverSettings.QuickConnectAvailable"
          :label="$t('enableQuickConnect')" />

        <h3 class="uno-mb-2 uno-text-lg uno-font-bold">
          {{ $t('serverSettingsPaths') }}
        </h3>

        <VTextField
          v-model="serverSettings.CachePath"
          :label="$t('cachePath')" />

        <VTextField
          v-model="serverSettings.MetadataPath"
          :label="$t('metadataPath')" />

        <h3 class="uno-mb-2 uno-text-lg uno-font-bold">
          {{ $t('serverSettingsBranding') }}
        </h3>

        <VTextField
          v-model="brandingSettings.LoginDisclaimer"
          :label="$t('loginDisclaimer')" />

        <VCheckbox
          v-model="brandingSettings.SplashscreenEnabled"
          :label="$t('enableSplashScreen')" />

        <h3 class="uno-mb-2 uno-text-lg uno-font-bold">
          {{ $t('serverSettingsPerformance') }}
        </h3>

        <VTextField
          v-model.number="serverSettings.LibraryScanFanoutConcurrency"
          :label="$t('parallelLibraryScanLimit')"
          type="number" />

        <VTextField
          v-model.number="serverSettings.ParallelImageEncodingLimit"
          :label="$t('parallelImageEncodingLimit')"
          type="number" />
      </VCol>

      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <tbody>
            <tr>
              <td>Server name</td>
              <td>{{ systemInfo.ServerName }}</td>
            </tr>
            <tr>
              <td>Version</td>
              <td>{{ systemInfo.Version }}</td>
            </tr>
            <tr>
              <td>Product</td>
              <td>{{ systemInfo.ProductName }}</td>
            </tr>
            <tr>
              <td>Operating system</td>
              <td>{{ systemInfo.OperatingSystem }}</td>
            </tr>
            <tr>
              <td>Local address</td>
              <td>{{ systemInfo.LocalAddress }}</td>
            </tr>
            <tr>
              <td>Startup wizard</td>
              <td>{{ systemInfo.StartupWizardCompleted ? 'Completed' : 'Pending' }}</td>
            </tr>
            <tr>
              <td>Can self restart</td>
              <td>{{ systemInfo.CanSelfRestart ? 'Yes' : 'No' }}</td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <tbody>
            <tr>
              <td>UI culture</td>
              <td>{{ serverSettings.UICulture || '-' }}</td>
            </tr>
            <tr>
              <td>Quick Connect</td>
              <td>{{ serverSettings.QuickConnectAvailable ? 'Enabled' : 'Disabled' }}</td>
            </tr>
            <tr>
              <td>Cache path</td>
              <td>{{ serverSettings.CachePath || '-' }}</td>
            </tr>
            <tr>
              <td>Metadata path</td>
              <td>{{ serverSettings.MetadataPath || '-' }}</td>
            </tr>
            <tr>
              <td>Library scan concurrency</td>
              <td>{{ serverSettings.LibraryScanFanoutConcurrency ?? '-' }}</td>
            </tr>
            <tr>
              <td>Image encoding concurrency</td>
              <td>{{ serverSettings.ParallelImageEncodingLimit ?? '-' }}</td>
            </tr>
          </tbody>
        </VTable>
      </VCol>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { onScopeDispose, shallowRef, watch } from 'vue';
import { getLocalizationApi } from '@jellyfin/sdk/lib/utils/api/localization-api';
import { getConfigurationApi } from '@jellyfin/sdk/lib/utils/api/configuration-api';
import { getBrandingApi } from '@jellyfin/sdk/lib/utils/api/branding-api';
import { SomeItemSelectedRule } from '@jellyfin-vue/shared/validation';
import { watchDeep } from '@vueuse/core';
import RemotePluginAxiosInstance from '#/plugins/remote/axios.ts';
import { useApi } from '#/composables/apis.ts';
import { taskManager } from '#/store/task-manager.ts';

type SystemInfo = {
  ServerName?: string;
  Version?: string;
  ProductName?: string;
  OperatingSystem?: string;
  LocalAddress?: string;
  StartupWizardCompleted?: boolean;
  CanSelfRestart?: boolean;
};

const tasks = new Map<number, string>();
const signal = shallowRef(false);
const [
  { data: culturesList },
  { data: serverSettings },
  { data: brandingSettings },
  systemInfoResponse
] = await Promise.all([
  useApi(getLocalizationApi, 'getLocalizationOptions')(),
  useApi(getConfigurationApi, 'getConfiguration')(),
  useApi(getBrandingApi, 'getBrandingOptions')(),
  RemotePluginAxiosInstance.instance.get<SystemInfo>('/System/Info')
]);

const systemInfo = systemInfoResponse.data;

const { loading: l1 } = await useApi(
  getConfigurationApi,
  () => signal.value ? 'updateConfiguration' : undefined,
  { skipCache: { request: true }, globalLoading: false }
)(() => ({
  serverConfiguration: serverSettings.value
}));
const { loading: l2 } = await useApi(
  getConfigurationApi,
  () => signal.value ? 'updateNamedConfiguration' : undefined,
  { skipCache: { request: true }, globalLoading: false }
)(() => ({
  key: 'branding',
  body: JSON.stringify(brandingSettings.value)
}));

watch([l1, l2], (newvals) => {
  for (let idx = 0; idx < newvals.length; idx++) {
    if (newvals[idx] && !tasks.has(idx)) {
      tasks.set(idx, taskManager.startConfigSync());
    } else {
      const taskId = tasks.get(idx);

      if (taskId) {
        taskManager.finishTask(taskId);
        tasks.delete(idx);
      }
    }
  }
});

watchDeep([serverSettings, brandingSettings], () => signal.value = true, { once: true });

onScopeDispose(() => {
  for (const [, id] of tasks) {
    taskManager.finishTask(id);
  }
});
</script>
