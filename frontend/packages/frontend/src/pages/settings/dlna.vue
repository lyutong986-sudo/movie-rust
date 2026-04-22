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
          :label="$t('enableDlnaServer')" />
        <VCheckbox
          v-model="configuration.EnableDlnaPlayTo"
          :label="$t('enableDlnaPlayTo')" />
        <VCheckbox
          v-model="configuration.EnableBlastAliveMessages"
          :label="$t('sendAliveBroadcasts')" />
        <VTextField
          v-model.number="configuration.BlastAliveMessageIntervalSeconds"
          :label="$t('aliveBroadcastIntervalSeconds')"
          type="number" />
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
              <td>{{ $t('dlnaServer') }}</td>
              <td>{{ configuration.EnableDlnaServer ? $t('enabled') : $t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ $t('playTo') }}</td>
              <td>{{ configuration.EnableDlnaPlayTo ? $t('enabled') : $t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ $t('aliveBroadcasts') }}</td>
              <td>{{ configuration.EnableBlastAliveMessages ? $t('enabled') : $t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ $t('broadcastInterval') }}</td>
              <td>{{ configuration.BlastAliveMessageIntervalSeconds }} s</td>
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
import { useServerConfiguration } from '#/composables/server-configuration.ts';

const { configuration, saving } = await useServerConfiguration({
  EnableDlnaServer: false,
  EnableDlnaPlayTo: false,
  EnableBlastAliveMessages: true,
  BlastAliveMessageIntervalSeconds: 1800
});
</script>
