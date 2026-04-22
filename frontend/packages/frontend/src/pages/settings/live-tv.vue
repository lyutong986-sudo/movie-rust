<template>
  <SettingsPage>
    <template #title>
      {{ $t('liveTv') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.EnableLiveTv"
          :label="$t('enableLiveTv')" />
        <VTextarea
          v-model="configuration.LiveTvTunerHostsText"
          :label="$t('tunerHosts')"
          rows="4" />
        <VTextarea
          v-model="configuration.LiveTvListingProvidersText"
          :label="$t('listingProviders')"
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
              <td>{{ $t('liveTv') }}</td>
              <td>{{ configuration.EnableLiveTv ? $t('enabled') : $t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ $t('tunerCount') }}</td>
              <td>{{ tunerHosts.length }}</td>
            </tr>
            <tr>
              <td>{{ $t('listingProviderCount') }}</td>
              <td>{{ listingProviders.length }}</td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>{{ $t('tunerHost') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="host in tunerHosts"
              :key="host">
              <td>{{ host }}</td>
            </tr>
            <tr v-if="!tunerHosts.length">
              <td class="uno-opacity-70">
                {{ $t('noTunerHostsConfigured') }}
              </td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>{{ $t('listingProvider') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="provider in listingProviders"
              :key="provider">
              <td>{{ provider }}</td>
            </tr>
            <tr v-if="!listingProviders.length">
              <td class="uno-opacity-70">
                {{ $t('noListingProvidersConfigured') }}
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
  EnableLiveTv: false,
  LiveTvTunerHostsText: '',
  LiveTvListingProvidersText: ''
});

const tunerHosts = computed(() => splitLines(configuration.value.LiveTvTunerHostsText));
const listingProviders = computed(() => splitLines(configuration.value.LiveTvListingProvidersText));

function splitLines(value?: string): string[] {
  return (value ?? '')
    .split(/\r?\n/u)
    .map(item => item.trim())
    .filter(Boolean);
}
</script>
