<template>
  <SettingsPage>
    <template #title>
      {{ $t('scheduledTasks') }}
    </template>

    <template #content>
      <VCol
        md="8"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.EnableScheduledTasks"
          :label="$t('enableScheduledTasks')" />
        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>{{ $t('tasks') }}</th>
              <th>{{ $t('category') }}</th>
              <th>{{ $t('status') }}</th>
              <th>{{ $t('lastRun') }}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="task in tasks"
              :key="task.Id">
              <td>
                <div class="uno-font-medium">
                  {{ task.Name }}
                </div>
                <div class="uno-text-sm uno-opacity-70">
                  {{ task.Description }}
                </div>
              </td>
              <td>{{ task.Category }}</td>
              <td>{{ task.State }}</td>
              <td>{{ task.LastExecutionResult?.EndTimeUtc ?? '-' }}</td>
              <td class="uno-text-right">
                <VBtn
                  size="small"
                  variant="tonal"
                  :loading="busyId === task.Id"
                  @click="runTask(task.Id)">
                  {{ $t('runNow') }}
                </VBtn>
              </td>
            </tr>
          </tbody>
        </VTable>
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
import { ref } from 'vue';
import type { SettingsTaskInfo } from '#/composables/use-settings-sdk.ts';
import { useServerConfiguration } from '#/composables/server-configuration.ts';
import { useSettingsSdk } from '#/composables/use-settings-sdk.ts';

const { configuration, saving } = await useServerConfiguration({
  EnableScheduledTasks: true,
  LibraryScanIntervalHours: 24,
  MetadataRefreshIntervalHours: 72
});
const { scheduledTasksApi } = useSettingsSdk();

const tasks = ref<SettingsTaskInfo[]>(await scheduledTasksApi.getScheduledtasks());
const busyId = ref<string>();

async function reloadTasks(): Promise<void> {
  tasks.value = await scheduledTasksApi.getScheduledtasks();
}

async function runTask(id?: string): Promise<void> {
  if (!id) {
    return;
  }

  busyId.value = id;
  try {
    await scheduledTasksApi.postScheduledtasksRunningById(id);
    await reloadTasks();
  } finally {
    busyId.value = undefined;
  }
}
</script>
