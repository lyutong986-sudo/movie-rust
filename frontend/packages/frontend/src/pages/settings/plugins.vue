<template>
  <SettingsPage>
    <template #title>
      {{ $t('plugins') }}
    </template>

    <template #content>
      <VCol
        md="8"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.EnablePlugins"
          :label="t('enablePluginSystem')" />
        <VTextarea
          v-model="configuration.PluginRepositoriesText"
          :label="t('pluginRepositories')"
          rows="4" />
        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>{{ t('plugins') }}</th>
              <th>{{ t('version') }}</th>
              <th>{{ t('status') }}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="plugin in plugins"
              :key="plugin.Id">
              <td>
                <div class="uno-font-medium">
                  {{ plugin.Name }}
                </div>
                <div class="uno-text-sm uno-opacity-70">
                  {{ plugin.Description }}
                </div>
              </td>
              <td>{{ plugin.Version }}</td>
              <td>{{ plugin.Enabled ? t('enabled') : t('disabled') }}</td>
              <td class="uno-text-right">
                <VBtn
                  size="small"
                  variant="tonal"
                  :loading="busyId === plugin.Id"
                  @click="togglePlugin(plugin)">
                  {{ plugin.Enabled ? t('disable') : t('enable') }}
                </VBtn>
              </td>
            </tr>
          </tbody>
        </VTable>
        <VProgressLinear
          v-if="saving"
          indeterminate />
      </VCol>
    </template>
  </SettingsPage>
</template>

<route lang="yaml">
meta:
  admin: true
</route>

<script setup lang="ts">
import { ref } from 'vue';
import { useTranslation } from 'i18next-vue';
import type { SettingsPluginInfo } from '#/composables/use-settings-sdk.ts';
import { useServerConfiguration } from '#/composables/server-configuration.ts';
import { useSettingsSdk } from '#/composables/use-settings-sdk.ts';

const { t } = useTranslation();
const { configuration, saving } = await useServerConfiguration({
  EnablePlugins: false,
  PluginRepositoriesText: '',
  DisabledPluginsText: ''
});
const { pluginsApi } = useSettingsSdk();

const plugins = ref<SettingsPluginInfo[]>(await pluginsApi.getPlugins());
const busyId = ref<string>();

async function reloadPlugins(): Promise<void> {
  plugins.value = await pluginsApi.getPlugins();
}

async function togglePlugin(plugin: SettingsPluginInfo): Promise<void> {
  busyId.value = plugin.Id;
  try {
    await pluginsApi.postPluginsByIdConfiguration(plugin.Id, {
      Enabled: !plugin.Enabled
    });
    await reloadPlugins();
  } finally {
    busyId.value = undefined;
  }
}
</script>
