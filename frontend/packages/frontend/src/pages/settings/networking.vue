<template>
  <SettingsPage>
    <template #title>
      {{ $t('networking') }}
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VCheckbox
          v-model="configuration.EnableRemoteAccess"
          :label="$t('enableRemoteAccess')" />
        <VCheckbox
          v-model="configuration.EnableUPnP"
          :label="$t('enableAutomaticPortMapping')" />
        <VTextField
          v-model="configuration.PublicUrl"
          :label="$t('publicUrl')" />
        <VTextField
          v-model.number="configuration.PublicPort"
          :label="$t('httpPort')"
          type="number" />
        <VTextField
          v-model.number="configuration.HttpsPortNumber"
          :label="$t('httpsPort')"
          type="number" />
        <VTextarea
          v-model="configuration.LocalNetworkSubnetsText"
          :label="$t('localNetworkSubnets')"
          rows="3" />
        <VProgressLinear
          v-if="saving"
          indeterminate />
      </VCol>

      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <tbody>
            <tr>
              <td>{{ $t('serverName') }}</td>
              <td>{{ systemInfo.ServerName }}</td>
            </tr>
            <tr>
              <td>{{ $t('localAddress') }}</td>
              <td>{{ systemInfo.LocalAddress }}</td>
            </tr>
            <tr>
              <td>{{ $t('version') }}</td>
              <td>{{ systemInfo.Version }}</td>
            </tr>
            <tr>
              <td>{{ $t('inNetwork') }}</td>
              <td>{{ endpointInfo.IsInNetwork ? $t('yes') : $t('no') }}</td>
            </tr>
            <tr>
              <td>{{ $t('localOnly') }}</td>
              <td>{{ endpointInfo.IsLocal ? $t('yes') : $t('no') }}</td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>{{ $t('endpoint') }}</th>
              <th>{{ $t('type') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="domain in domains"
              :key="domain.url">
              <td>{{ domain.url }}</td>
              <td>{{ domain.isLocal ? $t('local') : $t('remote') }}</td>
            </tr>
            <tr v-if="!domains.length">
              <td
                colspan="2"
                class="uno-opacity-70">
                {{ $t('noEndpointsAvailable') }}
              </td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>{{ $t('wakeOnLan') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="(item, index) in wakeOnLanInfo"
              :key="index">
              <td>{{ formatWakeOnLan(item) }}</td>
            </tr>
            <tr v-if="!wakeOnLanInfo.length">
              <td class="uno-opacity-70">
                {{ $t('noWakeOnLanTargetsConfigured') }}
              </td>
            </tr>
          </tbody>
        </VTable>
      </VCol>
    </template>
  </SettingsPage>
</template>

<route lang="yaml">
meta:
  admin: true
</route>

<script setup lang="ts">
import { computed } from 'vue';
import type { SystemInfo } from '@jellyfin/sdk/lib/generated-client';
import type {
  SettingsNetEndPointInfo,
  SettingsServerDomain,
  SettingsWakeOnLanInfo
} from '#/composables/use-settings-sdk.ts';
import { useServerConfiguration } from '#/composables/server-configuration.ts';
import { useSettingsSdk } from '#/composables/use-settings-sdk.ts';

const { configuration, saving } = await useServerConfiguration({
  EnableRemoteAccess: true,
  EnableUPnP: false,
  PublicUrl: '',
  PublicPort: 8096,
  HttpsPortNumber: 8920,
  LocalNetworkSubnetsText: ''
});
const { serverApi } = useSettingsSdk();

const [
  systemInfo,
  endpointInfo,
  domainsValue,
  wakeOnLanInfoValue
] = await Promise.all([
  serverApi.getSystemInfo() as Promise<SystemInfo>,
  serverApi.getSystemEndpoint() as Promise<SettingsNetEndPointInfo>,
  serverApi.getServerDomains() as Promise<SettingsServerDomain[]>,
  serverApi.getSystemWakeonlaninfo() as Promise<SettingsWakeOnLanInfo[]>
]);

const domains = computed(() => domainsValue);
const wakeOnLanInfo = computed(() => wakeOnLanInfoValue);

function formatWakeOnLan(item: unknown): string {
  if (typeof item === 'string') {
    return item;
  }

  if (item && typeof item === 'object') {
    return Object.entries(item as Record<string, unknown>)
      .map(([key, value]) => `${key}: ${String(value)}`)
      .join(' | ');
  }

  return String(item ?? '');
}
</script>
