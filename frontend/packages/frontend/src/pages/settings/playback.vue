<template>
  <SettingsPage>
    <template #title>
      {{ $t('playback') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VSelect
          v-model="playbackManager.playbackSpeed.value"
          label="Default playback speed"
          :items="playbackSpeeds" />
        <VCheckbox
          v-model="playerElement.state.value.isStretched"
          label="Stretch video to fill" />
        <VCheckbox
          v-model="subtitleSettings.state.value.enabled"
          label="Enable custom subtitle rendering" />
        <VSlider
          v-model="subtitleSettings.state.value.fontSize"
          label="Subtitle font size"
          min="0.8"
          max="4"
          step="0.1" />
        <VSlider
          v-model="subtitleSettings.state.value.positionFromBottom"
          label="Subtitle bottom offset"
          min="0"
          max="40"
          step="1" />
        <VCheckbox
          v-model="subtitleSettings.state.value.backdrop"
          :disabled="!subtitleSettings.state.value.enabled"
          label="Subtitle backdrop" />
        <VCheckbox
          v-model="subtitleSettings.state.value.stroke"
          :disabled="!subtitleSettings.state.value.enabled"
          label="Subtitle stroke" />
      </VCol>

      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <tbody>
            <tr>
              <td>Current status</td>
              <td>{{ playbackStatusText }}</td>
            </tr>
            <tr>
              <td>Playback speed</td>
              <td>{{ playbackManager.playbackSpeed.value }}x</td>
            </tr>
            <tr>
              <td>Stretch mode</td>
              <td>{{ playerElement.state.value.isStretched ? 'Enabled' : 'Disabled' }}</td>
            </tr>
            <tr>
              <td>Custom subtitles</td>
              <td>{{ subtitleSettings.state.value.enabled ? 'Enabled' : 'Disabled' }}</td>
            </tr>
            <tr>
              <td>Subtitle font size</td>
              <td>{{ subtitleSettings.state.value.fontSize }}em</td>
            </tr>
            <tr>
              <td>Subtitle offset</td>
              <td>{{ subtitleSettings.state.value.positionFromBottom }}vh</td>
            </tr>
            <tr>
              <td>Current item</td>
              <td>{{ playbackManager.currentItem.value?.Name ?? '-' }}</td>
            </tr>
            <tr>
              <td>Active play session</td>
              <td>{{ playbackManager.playSessionId.value ?? '-' }}</td>
            </tr>
          </tbody>
        </VTable>
      </VCol>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { PlaybackStatus, playbackManager } from '#/store/playback-manager.ts';
import { playerElement } from '#/store/player-element.ts';
import { subtitleSettings } from '#/store/settings/subtitle.ts';

const playbackSpeeds = [0.5, 0.75, 1, 1.25, 1.5, 2];

const playbackStatusText = computed(() => {
  switch (playbackManager.status.value) {
    case PlaybackStatus.Playing:
      return 'Playing';
    case PlaybackStatus.Paused:
      return 'Paused';
    case PlaybackStatus.Buffering:
      return 'Buffering';
    case PlaybackStatus.Error:
      return 'Error';
    default:
      return 'Stopped';
  }
});
</script>
