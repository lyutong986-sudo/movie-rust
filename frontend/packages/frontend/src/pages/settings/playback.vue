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
          label="默认播放速度"
          :items="playbackSpeeds" />
        <VCheckbox
          v-model="playerElement.state.value.isStretched"
          label="拉伸视频画面" />
        <VCheckbox
          v-model="subtitleSettings.state.value.enabled"
          label="启用自定义字幕样式" />
        <VSlider
          v-model="subtitleSettings.state.value.fontSize"
          label="字幕字号"
          min="0.8"
          max="4"
          step="0.1" />
        <VSlider
          v-model="subtitleSettings.state.value.positionFromBottom"
          label="字幕底部位置"
          min="0"
          max="40"
          step="1" />
      </VCol>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { playbackManager } from '#/store/playback-manager.ts';
import { playerElement } from '#/store/player-element.ts';
import { subtitleSettings } from '#/store/settings/subtitle.ts';

const playbackSpeeds = [0.5, 0.75, 1, 1.25, 1.5, 2];
</script>
