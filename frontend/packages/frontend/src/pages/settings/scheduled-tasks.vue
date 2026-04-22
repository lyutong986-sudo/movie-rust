<template>
  <SettingsPage>
    <template #title>
      {{ $t('scheduledTasks') }}
    </template>

    <template #content>
      <VCol
        md="8"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.EnableScheduledTasks"
          label="启用计划任务" />
        <VTextField
          v-model.number="configuration.LibraryScanIntervalHours"
          label="媒体库扫描间隔小时"
          type="number" />
        <VTextField
          v-model.number="configuration.MetadataRefreshIntervalHours"
          label="元数据刷新间隔小时"
          type="number" />
        <VTable>
          <thead>
            <tr>
              <th>任务</th>
              <th>间隔</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>媒体库扫描</td>
              <td>{{ configuration.LibraryScanIntervalHours }}h</td>
            </tr>
            <tr>
              <td>元数据刷新</td>
              <td>{{ configuration.MetadataRefreshIntervalHours }}h</td>
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
import { useServerConfiguration } from '#/composables/server-configuration.ts';

const { configuration, saving } = await useServerConfiguration({
  EnableScheduledTasks: true,
  LibraryScanIntervalHours: 24,
  MetadataRefreshIntervalHours: 72
});
</script>
