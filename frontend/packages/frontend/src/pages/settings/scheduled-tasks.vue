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
          label="启用计划任务" />
        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>任务</th>
              <th>分类</th>
              <th>状态</th>
              <th>上次运行</th>
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
                  立即运行
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
import RemotePluginAxiosInstance from '#/plugins/remote/axios.ts';
import { useServerConfiguration } from '#/composables/server-configuration.ts';

type TaskInfo = {
  Id: string;
  Name: string;
  Description?: string;
  Category?: string;
  State?: string;
  LastExecutionResult?: {
    EndTimeUtc?: string;
  };
};

const { configuration, saving } = await useServerConfiguration({
  EnableScheduledTasks: true,
  LibraryScanIntervalHours: 24,
  MetadataRefreshIntervalHours: 72
});

const tasks = ref(
  (await RemotePluginAxiosInstance.instance.get<TaskInfo[]>('/ScheduledTasks')).data
);
const busyId = ref<string>();

async function reloadTasks(): Promise<void> {
  tasks.value = (await RemotePluginAxiosInstance.instance.get<TaskInfo[]>('/ScheduledTasks')).data;
}

async function runTask(id: string): Promise<void> {
  busyId.value = id;
  try {
    await RemotePluginAxiosInstance.instance.post(`/ScheduledTasks/Running/${id}`);
    await reloadTasks();
  } finally {
    busyId.value = undefined;
  }
}
</script>
