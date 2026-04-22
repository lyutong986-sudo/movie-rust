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
          label="启用插件系统" />
        <VTextarea
          v-model="configuration.PluginRepositoriesText"
          label="插件仓库"
          rows="4" />
        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>插件</th>
              <th>版本</th>
              <th>状态</th>
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
              <td>{{ plugin.Enabled ? '已启用' : '已禁用' }}</td>
              <td class="uno-text-right">
                <VBtn
                  size="small"
                  variant="tonal"
                  :loading="busyId === plugin.Id"
                  @click="togglePlugin(plugin)">
                  {{ plugin.Enabled ? '禁用' : '启用' }}
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
import RemotePluginAxiosInstance from '#/plugins/remote/axios.ts';
import { useServerConfiguration } from '#/composables/server-configuration.ts';

type PluginInfo = {
  Id: string;
  Name: string;
  Version?: string;
  Description?: string;
  Enabled?: boolean;
};

const { configuration, saving } = await useServerConfiguration({
  EnablePlugins: false,
  PluginRepositoriesText: '',
  DisabledPluginsText: ''
});

const plugins = ref(
  (await RemotePluginAxiosInstance.instance.get<PluginInfo[]>('/Plugins')).data
);
const busyId = ref<string>();

async function reloadPlugins(): Promise<void> {
  plugins.value = (await RemotePluginAxiosInstance.instance.get<PluginInfo[]>('/Plugins')).data;
}

async function togglePlugin(plugin: PluginInfo): Promise<void> {
  busyId.value = plugin.Id;
  try {
    await RemotePluginAxiosInstance.instance.post(`/Plugins/${plugin.Id}/Configuration`, {
      Enabled: !plugin.Enabled
    });
    await reloadPlugins();
  } finally {
    busyId.value = undefined;
  }
}
</script>
