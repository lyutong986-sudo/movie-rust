<template>
  <SettingsPage>
    <template #title>
      {{ $t('notifications') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.EnableNotifications"
          label="启用通知" />
        <VCheckbox
          v-model="configuration.NotifyOnPlaybackStart"
          label="播放开始通知" />
        <VCheckbox
          v-model="configuration.NotifyOnLibraryScan"
          label="媒体库扫描通知" />
        <VTextarea
          v-model="configuration.NotificationTargetsText"
          label="通知目标"
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
  EnableNotifications: false,
  NotifyOnPlaybackStart: false,
  NotifyOnLibraryScan: true,
  NotificationTargetsText: ''
});
</script>
