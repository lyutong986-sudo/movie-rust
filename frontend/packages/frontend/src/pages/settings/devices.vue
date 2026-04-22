<template>
  <SettingsPage>
    <template #title>
      {{ t('devices') }}
    </template>

    <template #actions>
      <VBtn
        variant="elevated"
        :loading="refreshing"
        @click="refreshDevices">
        {{ t('refresh') }}
      </VBtn>
      <VBtn
        v-if="devices.length"
        color="error"
        variant="elevated"
        class="ml-a"
        :loading="loading"
        @click="confirmDelete = 'all'">
        {{ t('deleteAll') }}
      </VBtn>
    </template>

    <template #content>
      <VCol cols="12">
        <VTable>
          <thead>
            <tr>
              <th>{{ t('user') }}</th>
              <th>{{ t('device') }}</th>
              <th>{{ t('client') }}</th>
              <th>{{ t('lastActive') }}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="device in devices"
              :key="device.Id ?? undefined">
              <td>{{ device.LastUserName }}</td>
              <td>
                <div class="uno-font-medium">
                  {{ device.Name }}
                </div>
                <div class="uno-text-sm uno-opacity-70">
                  {{ device.Id }}
                </div>
              </td>
              <td>{{ device.AppName }} {{ device.AppVersion }}</td>
              <td>{{ formatActivity(device.DateLastActivity) }}</td>
              <td class="uno-text-right">
                <VBtn
                  variant="tonal"
                  size="small"
                  class="uno-mr-2"
                  :disabled="loading"
                  @click="openDetails(device)">
                  {{ t('details') }}
                </VBtn>
                <VBtn
                  color="error"
                  size="small"
                  :disabled="loading"
                  @click="confirmDelete = device.Id ?? undefined">
                  {{ t('delete') }}
                </VBtn>
              </td>
            </tr>
            <tr v-if="!devices.length">
              <td
                colspan="5"
                class="uno-opacity-70">
                {{ t('noRememberedDevices') }}
              </td>
            </tr>
          </tbody>
        </VTable>
      </VCol>

      <VDialog
        width="720"
        :model-value="!!selectedDevice"
        @update:model-value="selectedDevice = undefined">
        <VCard v-if="selectedDevice">
          <VCardTitle>{{ t('deviceDetails') }}</VCardTitle>
          <VCardText>
            <VTextField
              v-model="deviceOptions.CustomName"
              :label="t('customName')" />
            <VTable class="uno-mt-4">
              <tbody>
                <tr>
                  <td>{{ t('deviceId') }}</td>
                  <td>{{ deviceInfo.Id }}</td>
                </tr>
                <tr>
                  <td>{{ t('reportedDeviceId') }}</td>
                  <td>{{ deviceInfo.ReportedDeviceId ?? '-' }}</td>
                </tr>
                <tr>
                  <td>{{ t('name') }}</td>
                  <td>{{ deviceInfo.Name }}</td>
                </tr>
                <tr>
                  <td>{{ t('client') }}</td>
                  <td>{{ deviceInfo.AppName }} {{ deviceInfo.AppVersion }}</td>
                </tr>
                <tr>
                  <td>{{ t('lastUser') }}</td>
                  <td>{{ deviceInfo.LastUserName }}</td>
                </tr>
                <tr>
                  <td>{{ t('lastActive') }}</td>
                  <td>{{ formatActivity(deviceInfo.DateLastActivity) }}</td>
                </tr>
              </tbody>
            </VTable>
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn @click="selectedDevice = undefined">
              {{ t('cancel') }}
            </VBtn>
            <VBtn
              color="primary"
              :loading="loading"
              @click="saveDeviceOptions">
              {{ t('save') }}
            </VBtn>
          </VCardActions>
        </VCard>
      </VDialog>

      <VDialog
        width="auto"
        :model-value="!isNil(confirmDelete)"
        @update:model-value="confirmDelete = undefined">
        <VCard>
          <VCardText>
            {{ t('deleteConfirm') }}
          </VCardText>
          <VCardActions>
            <VBtn
              color="primary"
              :loading="loading"
              @click="confirmDeletion">
              {{ t('confirm') }}
            </VBtn>
            <VBtn
              :loading="loading"
              @click="confirmDelete = undefined">
              {{ t('cancel') }}
            </VBtn>
          </VCardActions>
        </VCard>
      </VDialog>
    </template>
  </SettingsPage>
</template>

<route lang="yaml">
meta:
  admin: true
</route>

<script setup lang="ts">
import type {
  DevicesDeviceInfo,
  DevicesDeviceOptions
} from '@jellyfin/sdk/lib/generated-client';
import { formatRelative, parseJSON } from 'date-fns';
import { ref } from 'vue';
import { useTranslation } from 'i18next-vue';
import { isNil } from '@jellyfin-vue/shared/validation';
import { useSettingsSdk, type SettingsDeviceDetails } from '#/composables/use-settings-sdk.ts';
import { remote } from '#/plugins/remote/index.ts';
import { useSnackbar } from '#/composables/use-snackbar.ts';
import { useDateFns } from '#/composables/use-datefns.ts';

const { t } = useTranslation();
const { devicesApi } = useSettingsSdk();

const devices = ref<DevicesDeviceInfo[]>([]);
const selectedDevice = ref<DevicesDeviceInfo>();
const deviceInfo = ref<Partial<SettingsDeviceDetails>>({});
const deviceOptions = ref<DevicesDeviceOptions>({});
const loading = ref(false);
const refreshing = ref(false);
const confirmDelete = ref<string>();

async function refreshDevices(): Promise<void> {
  refreshing.value = true;
  try {
    devices.value = (await devicesApi.getDevices()).data.Items ?? [];
  } finally {
    refreshing.value = false;
  }
}

function formatActivity(value?: string): string {
  return value
    ? useDateFns(formatRelative, parseJSON(value), new Date())
    : '-';
}

async function openDetails(device: DevicesDeviceInfo): Promise<void> {
  if (!device.Id) {
    return;
  }

  loading.value = true;
  try {
    const [infoResponse, optionsResponse] = await Promise.all([
      devicesApi.getDevicesInfo(device.Id),
      devicesApi.getDevicesOptions(device.Id)
    ]);
    selectedDevice.value = device;
    deviceInfo.value = infoResponse.data ?? {};
    deviceOptions.value = optionsResponse.data ?? {};
  } finally {
    loading.value = false;
  }
}

async function saveDeviceOptions(): Promise<void> {
  if (!selectedDevice.value?.Id) {
    return;
  }

  loading.value = true;
  try {
    await devicesApi.postDevicesOptions(deviceOptions.value, selectedDevice.value.Id);
    useSnackbar(t('deviceOptionsSaved'), 'success');
    selectedDevice.value = undefined;
    await refreshDevices();
  } catch (error) {
    console.error(error);
    useSnackbar(t('failedToSaveDeviceOptions'), 'error');
  } finally {
    loading.value = false;
  }
}

async function deleteDevice(deviceId: string): Promise<void> {
  loading.value = true;
  try {
    await devicesApi.deleteDevice({ id: deviceId });
    useSnackbar(t('deleteDeviceSuccess'), 'success');
    await refreshDevices();
  } catch (error) {
    console.error(error);
    useSnackbar(t('deleteDeviceError'), 'error');
  } finally {
    loading.value = false;
  }
}

async function deleteAllDevices(): Promise<void> {
  loading.value = true;
  try {
    for (const device of devices.value) {
      if (device.Id && remote.sdk.deviceInfo.id !== device.Id) {
        await devicesApi.deleteDevice({ id: device.Id });
      }
    }
    useSnackbar(t('deleteAllDevicesSuccess'), 'success');
    await refreshDevices();
  } catch (error) {
    console.error(error);
    useSnackbar(t('deleteAllDevicesError'), 'error');
  } finally {
    loading.value = false;
  }
}

async function confirmDeletion(): Promise<void> {
  if (!confirmDelete.value) {
    return;
  }

  await (confirmDelete.value === 'all'
    ? deleteAllDevices()
    : deleteDevice(confirmDelete.value));

  confirmDelete.value = undefined;
}

await refreshDevices();
</script>
