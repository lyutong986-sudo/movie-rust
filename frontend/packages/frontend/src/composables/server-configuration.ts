import { ref, shallowRef, watch } from 'vue';
import { watchDeep } from '@vueuse/core';
import RemotePluginAxiosInstance from '#/plugins/remote/axios.ts';
import { taskManager } from '#/store/task-manager.ts';

type ServerConfiguration = Record<string, unknown>;

export async function useServerConfiguration<T extends ServerConfiguration>(defaults: T) {
  const response = await RemotePluginAxiosInstance.instance.get<ServerConfiguration>('/System/Configuration');
  const configuration = ref<T & ServerConfiguration>({
    ...defaults,
    ...response.data
  });
  const loaded = shallowRef(false);
  const saving = shallowRef(false);
  let taskId: string | undefined;

  async function save(): Promise<void> {
    if (!loaded.value) {
      return;
    }

    saving.value = true;
    taskId ??= taskManager.startConfigSync();

    try {
      await RemotePluginAxiosInstance.instance.post('/System/Configuration', configuration.value);
    } finally {
      saving.value = false;

      if (taskId) {
        taskManager.finishTask(taskId);
        taskId = undefined;
      }
    }
  }

  watchDeep(configuration, save);
  watch(configuration, () => loaded.value = true, { immediate: true, once: true });

  return {
    configuration,
    saving
  };
}
