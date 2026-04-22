<template>
  <SettingsPage>
    <template #title>
      {{ $t('plugins') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.EnablePlugins"
          label="启用插件系统" />
        <VTextarea
          v-model="configuration.PluginRepositoriesText"
          label="插件仓库"
          rows="4" />
        <VTextarea
          v-model="configuration.DisabledPluginsText"
          label="禁用插件"
          rows="4" />
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
import { useServerConfiguration } from '#/composables/server-configuration.ts';

const { configuration, saving } = await useServerConfiguration({
  EnablePlugins: false,
  PluginRepositoriesText: '',
  DisabledPluginsText: ''
});
</script>
