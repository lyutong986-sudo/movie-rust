<template>
  <SettingsPage>
    <template #title>
      {{ $t('subtitles') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VSwitch
          v-model="subtitleSettings.state.value.enabled"
          :label="$t('enableSubtitles')" />
        <FontSelector
          v-model="subtitleSettings.state.value.fontFamily"
          :label="$t('subtitleFont')"
          :disabled="!subtitleSettings.state.value.enabled" />

        <VSlider
          v-model="subtitleSettings.state.value.fontSize"
          :label="$t('fontSize')"
          :min="1"
          :max="4.5"
          :step="0.1"
          :disabled="!subtitleSettings.state.value.enabled" />

        <VSlider
          v-model="subtitleSettings.state.value.positionFromBottom"
          :label="$t('positionFromBottom')"
          :min="0"
          :max="30"
          :step="1"
          :disabled="!subtitleSettings.state.value.enabled" />

        <VCheckbox
          v-model="subtitleSettings.state.value.backdrop"
          :label="$t('backdrop')"
          :disabled="!subtitleSettings.state.value.enabled" />

        <VCheckbox
          v-model="subtitleSettings.state.value.stroke"
          :label="$t('stroke')"
          :disabled="!subtitleSettings.state.value.enabled" />

        <SubtitleTrack
          v-if="subtitleSettings.state.value.enabled"
          preview />
      </VCol>

      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <tbody>
            <tr>
              <td>Custom subtitles</td>
              <td>{{ subtitleSettings.state.value.enabled ? 'Enabled' : 'Disabled' }}</td>
            </tr>
            <tr>
              <td>Font family</td>
              <td>{{ subtitleSettings.state.value.fontFamily }}</td>
            </tr>
            <tr>
              <td>Font size</td>
              <td>{{ subtitleSettings.state.value.fontSize }}em</td>
            </tr>
            <tr>
              <td>Bottom offset</td>
              <td>{{ subtitleSettings.state.value.positionFromBottom }}vh</td>
            </tr>
            <tr>
              <td>Backdrop</td>
              <td>{{ subtitleSettings.state.value.backdrop ? 'Enabled' : 'Disabled' }}</td>
            </tr>
            <tr>
              <td>Stroke</td>
              <td>{{ subtitleSettings.state.value.stroke ? 'Enabled' : 'Disabled' }}</td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <tbody>
            <tr>
              <td>Current subtitle track</td>
              <td>{{ currentSubtitleTrack }}</td>
            </tr>
            <tr>
              <td>Available subtitle tracks</td>
              <td>{{ playbackManager.currentItemSubtitleTracks.value?.length ?? 0 }}</td>
            </tr>
            <tr>
              <td>External parsed tracks</td>
              <td>{{ playerElement.currentItemExternalParsedSubtitleTracks.value?.length ?? 0 }}</td>
            </tr>
          </tbody>
        </VTable>
      </VCol>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { subtitleSettings } from '#/store/settings/subtitle.ts';
import { playbackManager } from '#/store/playback-manager.ts';
import { playerElement } from '#/store/player-element.ts';

const currentSubtitleTrack = computed(() =>
  playbackManager.currentSubtitleTrack.value?.DisplayTitle
  ?? playbackManager.currentSubtitleTrack.value?.Title
  ?? 'None'
);
</script>
