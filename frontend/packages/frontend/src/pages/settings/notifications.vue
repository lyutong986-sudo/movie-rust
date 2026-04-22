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
          label="Enable notifications" />
        <VCheckbox
          v-model="configuration.NotifyOnPlaybackStart"
          label="Notify on playback start" />
        <VCheckbox
          v-model="configuration.NotifyOnLibraryScan"
          label="Notify on library scan" />
        <VTextarea
          v-model="configuration.NotificationTargetsText"
          label="Notification targets"
          rows="4" />
        <VProgressLinear
          v-if="saving"
          indeterminate />
      </VCol>

      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <tbody>
            <tr>
              <td>Notifications</td>
              <td>{{ configuration.EnableNotifications ? 'Enabled' : 'Disabled' }}</td>
            </tr>
            <tr>
              <td>Playback event</td>
              <td>{{ configuration.NotifyOnPlaybackStart ? 'Subscribed' : 'Muted' }}</td>
            </tr>
            <tr>
              <td>Library scan event</td>
              <td>{{ configuration.NotifyOnLibraryScan ? 'Subscribed' : 'Muted' }}</td>
            </tr>
            <tr>
              <td>Target count</td>
              <td>{{ notificationTargets.length }}</td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>Notification target</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="target in notificationTargets"
              :key="target">
              <td>{{ target }}</td>
            </tr>
            <tr v-if="!notificationTargets.length">
              <td class="uno-opacity-70">
                No notification targets configured
              </td>
            </tr>
          </tbody>
        </VTable>
      </VCol>
    </template>
  </SettingsPage>
</template>

<route lang="yaml">
meta:
  admin: true
</route>

<script setup lang="ts">
import { computed } from 'vue';
import { useServerConfiguration } from '#/composables/server-configuration.ts';

const { configuration, saving } = await useServerConfiguration({
  EnableNotifications: false,
  NotifyOnPlaybackStart: false,
  NotifyOnLibraryScan: true,
  NotificationTargetsText: ''
});

const notificationTargets = computed(() => (
  configuration.value.NotificationTargetsText ?? ''
)
  .split(/\r?\n/u)
  .map(item => item.trim())
  .filter(Boolean));
</script>
