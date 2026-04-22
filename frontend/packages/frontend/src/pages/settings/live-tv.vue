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
          label="启用 Live TV" />
        <VTextarea
          v-model="configuration.LiveTvTunerHostsText"
          label="调谐器地址"
          rows="4" />
        <VTextarea
          v-model="configuration.LiveTvListingProvidersText"
          label="节目单来源"
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
  EnableLiveTv: false,
  LiveTvTunerHostsText: '',
  LiveTvListingProvidersText: ''
});
</script>
