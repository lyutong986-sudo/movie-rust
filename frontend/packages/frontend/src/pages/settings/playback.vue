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
          :label="$t('defaultPlaybackSpeed')"
          :items="playbackSpeeds" />
        <VCheckbox
          v-model="playerElement.state.value.isStretched"
          :label="$t('stretchVideoToFill')" />
        <VCheckbox
          v-model="subtitleSettings.state.value.enabled"
          :label="$t('enableCustomSubtitleRendering')" />
        <VSlider
          v-model="subtitleSettings.state.value.fontSize"
          :label="$t('subtitleFontSize')"
          min="0.8"
          max="4"
          step="0.1" />
        <VSlider
          v-model="subtitleSettings.state.value.positionFromBottom"
          :label="$t('subtitleBottomOffset')"
          min="0"
          max="40"
          step="1" />
        <VCheckbox
          v-model="subtitleSettings.state.value.backdrop"
          :disabled="!subtitleSettings.state.value.enabled"
          :label="$t('subtitleBackdrop')" />
        <VCheckbox
          v-model="subtitleSettings.state.value.stroke"
          :disabled="!subtitleSettings.state.value.enabled"
          :label="$t('subtitleStroke')" />
      </VCol>

      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <tbody>
            <tr>
              <td>{{ $t('currentStatus') }}</td>
              <td>{{ playbackStatusText }}</td>
            </tr>
            <tr>
              <td>{{ $t('playbackSpeed') }}</td>
              <td>{{ playbackManager.playbackSpeed.value }}x</td>
            </tr>
            <tr>
              <td>{{ $t('stretchMode') }}</td>
              <td>{{ playerElement.state.value.isStretched ? $t('enabled') : $t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ $t('customSubtitles') }}</td>
              <td>{{ subtitleSettings.state.value.enabled ? $t('enabled') : $t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ $t('subtitleFontSize') }}</td>
              <td>{{ subtitleSettings.state.value.fontSize }}em</td>
            </tr>
            <tr>
              <td>{{ $t('subtitleOffset') }}</td>
              <td>{{ subtitleSettings.state.value.positionFromBottom }}vh</td>
            </tr>
            <tr>
              <td>{{ $t('currentItem') }}</td>
              <td>{{ playbackManager.currentItem.value?.Name ?? '-' }}</td>
            </tr>
            <tr>
              <td>{{ $t('activePlaySession') }}</td>
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
import { useTranslation } from 'i18next-vue';
import { PlaybackStatus, playbackManager } from '#/store/playback-manager.ts';
import { playerElement } from '#/store/player-element.ts';
import { subtitleSettings } from '#/store/settings/subtitle.ts';

const { t } = useTranslation();
const playbackSpeeds = [0.5, 0.75, 1, 1.25, 1.5, 2];

const playbackStatusText = computed(() => {
  switch (playbackManager.status.value) {
    case PlaybackStatus.Playing:
      return t('playing');
    case PlaybackStatus.Paused:
      return t('paused');
    case PlaybackStatus.Buffering:
      return t('buffering');
    case PlaybackStatus.Error:
      return t('error');
    default:
      return t('stopped');
  }
});
</script>
