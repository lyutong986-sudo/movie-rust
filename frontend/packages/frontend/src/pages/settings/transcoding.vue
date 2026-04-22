<template>
  <SettingsPage>
    <template #title>
      {{ $t('transcodingAndStreaming') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.EnableTranscoding"
          label="启用转码" />
        <VTextField
          v-model="configuration.TranscodingTempPath"
          label="转码临时目录" />
        <VTextField
          v-model.number="configuration.MaxStreamingBitrate"
          label="最大流媒体码率"
          type="number" />
        <VSelect
          v-model="configuration.HardwareAccelerationType"
          label="硬件加速"
          :items="hardwareAcceleration" />
        <VCheckbox
          v-model="configuration.EnableTranscodingThrottle"
          label="启用转码节流" />
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

const hardwareAcceleration = ['none', 'vaapi', 'qsv', 'nvenc', 'amf', 'videotoolbox'];
const { configuration, saving } = await useServerConfiguration({
  EnableTranscoding: true,
  TranscodingTempPath: '',
  MaxStreamingBitrate: 120000000,
  HardwareAccelerationType: 'none',
  EnableTranscodingThrottle: true
});
</script>
