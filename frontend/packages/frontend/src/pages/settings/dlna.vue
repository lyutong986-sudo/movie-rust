<template>
  <SettingsPage>
    <template #title>
      {{ $t('dlna') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.EnableDlnaServer"
          label="启用 DLNA 服务器" />
        <VCheckbox
          v-model="configuration.EnableDlnaPlayTo"
          label="启用 DLNA Play To" />
        <VCheckbox
          v-model="configuration.EnableBlastAliveMessages"
          label="发送活动广播" />
        <VTextField
          v-model.number="configuration.BlastAliveMessageIntervalSeconds"
          label="广播间隔秒数"
          type="number" />
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
  EnableDlnaServer: false,
  EnableDlnaPlayTo: false,
  EnableBlastAliveMessages: true,
  BlastAliveMessageIntervalSeconds: 1800
});
</script>
