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
          label="Enable DLNA server" />
        <VCheckbox
          v-model="configuration.EnableDlnaPlayTo"
          label="Enable DLNA Play To" />
        <VCheckbox
          v-model="configuration.EnableBlastAliveMessages"
          label="Send alive broadcasts" />
        <VTextField
          v-model.number="configuration.BlastAliveMessageIntervalSeconds"
          label="Alive broadcast interval (seconds)"
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
              <td>DLNA server</td>
              <td>{{ configuration.EnableDlnaServer ? 'Enabled' : 'Disabled' }}</td>
            </tr>
            <tr>
              <td>Play To</td>
              <td>{{ configuration.EnableDlnaPlayTo ? 'Enabled' : 'Disabled' }}</td>
            </tr>
            <tr>
              <td>Alive broadcasts</td>
              <td>{{ configuration.EnableBlastAliveMessages ? 'Enabled' : 'Disabled' }}</td>
            </tr>
            <tr>
              <td>Broadcast interval</td>
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
