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
          :label="$t('enableNotifications')" />
        <VCheckbox
          v-model="configuration.NotifyOnPlaybackStart"
          :label="$t('notifyOnPlaybackStart')" />
        <VCheckbox
          v-model="configuration.NotifyOnLibraryScan"
          :label="$t('notifyOnLibraryScan')" />
        <VTextarea
          v-model="configuration.NotificationTargetsText"
          :label="$t('notificationTargets')"
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
              <td>{{ $t('notifications') }}</td>
              <td>{{ configuration.EnableNotifications ? $t('enabled') : $t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ $t('playbackEvent') }}</td>
              <td>{{ configuration.NotifyOnPlaybackStart ? $t('subscribed') : $t('muted') }}</td>
            </tr>
            <tr>
              <td>{{ $t('libraryScanEvent') }}</td>
              <td>{{ configuration.NotifyOnLibraryScan ? $t('subscribed') : $t('muted') }}</td>
            </tr>
            <tr>
              <td>{{ $t('targetCount') }}</td>
              <td>{{ notificationTargets.length }}</td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>{{ $t('notificationTarget') }}</th>
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
                {{ $t('noNotificationTargetsConfigured') }}
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
