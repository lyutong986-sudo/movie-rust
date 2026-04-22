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
          label="Enable transcoding" />
        <VTextField
          v-model="configuration.TranscodingTempPath"
          label="Transcoding temp path" />
        <VTextField
          v-model.number="configuration.MaxStreamingBitrate"
          label="Max streaming bitrate"
          type="number" />
        <VSelect
          v-model="configuration.HardwareAccelerationType"
          label="Hardware acceleration"
          :items="hardwareAcceleration" />
        <VCheckbox
          v-model="configuration.EnableTranscodingThrottle"
          label="Enable transcoding throttle" />
        <VProgressLinear
          v-if="saving"
          indeterminate />
      </VCol>

      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <thead>
            <tr>
              <th>Play session</th>
              <th>Item</th>
              <th>State</th>
              <th>Progress</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="encoding in activeEncodings"
              :key="encoding.Id">
              <td>{{ encoding.PlaySessionId }}</td>
              <td>{{ encoding.ItemId }}</td>
              <td>{{ encoding.State }}</td>
              <td>{{ formatProgress(encoding.Progress) }}</td>
              <td class="uno-text-right">
                <VBtn
                  size="small"
                  variant="tonal"
                  :loading="busyId === encoding.Id"
                  @click="stopEncoding(encoding.Id)">
                  Stop
                </VBtn>
              </td>
            </tr>
            <tr v-if="!activeEncodings.length">
              <td
                colspan="5"
                class="uno-opacity-70">
                No active transcoding sessions
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
import { ref } from 'vue';
import type { SettingsActiveEncoding } from '#/composables/use-settings-sdk.ts';
import { useServerConfiguration } from '#/composables/server-configuration.ts';
import { useSettingsSdk } from '#/composables/use-settings-sdk.ts';

const hardwareAcceleration = ['none', 'vaapi', 'qsv', 'nvenc', 'amf', 'videotoolbox'];
const { configuration, saving } = await useServerConfiguration({
  EnableTranscoding: true,
  TranscodingTempPath: '',
  MaxStreamingBitrate: 120000000,
  HardwareAccelerationType: 'none',
  EnableTranscodingThrottle: true
});
const { transcodingApi } = useSettingsSdk();

const activeEncodings = ref<SettingsActiveEncoding[]>(await transcodingApi.getVideosActiveEncodings());
const busyId = ref<string>();

async function reloadActiveEncodings(): Promise<void> {
  activeEncodings.value = await transcodingApi.getVideosActiveEncodings();
}

async function stopEncoding(id: string): Promise<void> {
  busyId.value = id;
  try {
    await transcodingApi.deleteVideosActiveEncodings(id);
    await reloadActiveEncodings();
  } finally {
    busyId.value = undefined;
  }
}

function formatProgress(progress?: number): string {
  if (typeof progress !== 'number' || Number.isNaN(progress)) {
    return '-';
  }

  return `${Math.round(progress * 100)}%`;
}
</script>
