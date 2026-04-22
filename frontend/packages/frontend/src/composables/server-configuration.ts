import { ref, shallowRef, watch } from 'vue';
import { watchDeep } from '@vueuse/core';
import { useSettingsSdk } from '#/composables/use-settings-sdk.ts';
import { taskManager } from '#/store/task-manager.ts';

type ServerConfiguration = Record<string, unknown>;

export async function useServerConfiguration<T extends ServerConfiguration>(defaults: T) {
  const { serverApi } = useSettingsSdk();
  const response = await serverApi.getConfiguration();
  const configuration = ref<T & ServerConfiguration>({
    ...defaults,
    ...response
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
      await serverApi.updateConfiguration(configuration.value);
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
