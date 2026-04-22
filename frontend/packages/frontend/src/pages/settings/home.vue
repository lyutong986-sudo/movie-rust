<template>
  <SettingsPage>
    <template #title>
      {{ $t('homeScreen') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.showResume"
          :label="t('continueWatching')" />
        <VCheckbox
          v-model="configuration.showNextUp"
          :label="t('nextUp')" />
        <VCheckbox
          v-model="configuration.showLatest"
          :label="t('latestMedia')" />
        <VTextField
          v-model.number="configuration.latestLimit"
          :label="t('latestMediaCount')"
          type="number" />
        <VSelect
          v-model="configuration.sections"
          chips
          multiple
          :label="t('homeSections')"
          item-title="title"
          item-value="value"
          :items="sections" />
      </VCol>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useTranslation } from 'i18next-vue';
import { homeSettings } from '#/store/settings/home.ts';

const { t } = useTranslation();
const configuration = homeSettings.state;
const sections = computed(() => [
  { title: t('libraries'), value: 'libraries' },
  { title: t('continueWatching'), value: 'resumevideo' },
  { title: t('nextUp'), value: 'nextup' },
  { title: t('latestMedia'), value: 'latestmedia' }
]);
</script>
