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
              <td>{{ $t('customSubtitles') }}</td>
              <td>{{ subtitleSettings.state.value.enabled ? $t('enabled') : $t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ $t('fontFamily') }}</td>
              <td>{{ subtitleSettings.state.value.fontFamily }}</td>
            </tr>
            <tr>
              <td>{{ $t('fontSize') }}</td>
              <td>{{ subtitleSettings.state.value.fontSize }}em</td>
            </tr>
            <tr>
              <td>{{ $t('bottomOffset') }}</td>
              <td>{{ subtitleSettings.state.value.positionFromBottom }}vh</td>
            </tr>
            <tr>
              <td>{{ $t('backdrop') }}</td>
              <td>{{ subtitleSettings.state.value.backdrop ? $t('enabled') : $t('disabled') }}</td>
            </tr>
            <tr>
              <td>{{ $t('stroke') }}</td>
              <td>{{ subtitleSettings.state.value.stroke ? $t('enabled') : $t('disabled') }}</td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <tbody>
            <tr>
              <td>{{ $t('currentSubtitleTrack') }}</td>
              <td>{{ currentSubtitleTrack }}</td>
            </tr>
            <tr>
              <td>{{ $t('availableSubtitleTracks') }}</td>
              <td>{{ playbackManager.currentItemSubtitleTracks.value?.length ?? 0 }}</td>
            </tr>
            <tr>
              <td>{{ $t('externalParsedTracks') }}</td>
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
import { useTranslation } from 'i18next-vue';
import { subtitleSettings } from '#/store/settings/subtitle.ts';
import { playbackManager } from '#/store/playback-manager.ts';
import { playerElement } from '#/store/player-element.ts';

const { t } = useTranslation();
const currentSubtitleTrack = computed(() =>
  playbackManager.currentSubtitleTrack.value?.DisplayTitle
  ?? playbackManager.currentSubtitleTrack.value?.Title
  ?? t('none')
);
</script>
