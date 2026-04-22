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
          label="继续观看" />
        <VCheckbox
          v-model="configuration.showNextUp"
          label="下一集" />
        <VCheckbox
          v-model="configuration.showLatest"
          label="最新媒体" />
        <VTextField
          v-model.number="configuration.latestLimit"
          label="最新媒体数量"
          type="number" />
        <VSelect
          v-model="configuration.sections"
          chips
          multiple
          label="主页栏目"
          item-title="title"
          item-value="value"
          :items="sections" />
      </VCol>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { homeSettings } from '#/store/settings/home.ts';

const configuration = homeSettings.state;
const sections = [
  { title: '媒体库', value: 'libraries' },
  { title: '继续观看', value: 'resumevideo' },
  { title: '下一集', value: 'nextup' },
  { title: '最新媒体', value: 'latestmedia' }
];
</script>
