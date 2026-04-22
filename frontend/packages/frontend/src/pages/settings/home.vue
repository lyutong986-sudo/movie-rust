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
          v-model="configuration.HomeScreenShowResume"
          label="继续观看" />
        <VCheckbox
          v-model="configuration.HomeScreenShowNextUp"
          label="下一集" />
        <VCheckbox
          v-model="configuration.HomeScreenShowLatest"
          label="最新媒体" />
        <VTextField
          v-model.number="configuration.HomeScreenLatestLimit"
          label="最新媒体数量"
          type="number" />
        <VSelect
          v-model="configuration.HomeScreenSections"
          chips
          multiple
          label="主页栏目"
          :items="sections" />
        <VProgressLinear
          v-if="saving"
          indeterminate />
      </VCol>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { useServerConfiguration } from '#/composables/server-configuration.ts';

const sections = ['Resume', 'NextUp', 'Latest', 'Favorites', 'Genres'];
const { configuration, saving } = await useServerConfiguration({
  HomeScreenShowResume: true,
  HomeScreenShowNextUp: true,
  HomeScreenShowLatest: true,
  HomeScreenLatestLimit: 16,
  HomeScreenSections: ['Resume', 'NextUp', 'Latest']
});
</script>
