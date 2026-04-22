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
          label="Enable remote access" />
        <VCheckbox
          v-model="configuration.EnableUPnP"
          label="Enable automatic port mapping" />
        <VTextField
          v-model="configuration.PublicUrl"
          label="Public URL" />
        <VTextField
          v-model.number="configuration.PublicPort"
          label="HTTP port"
          type="number" />
        <VTextField
          v-model.number="configuration.HttpsPortNumber"
          label="HTTPS port"
          type="number" />
        <VTextarea
          v-model="configuration.LocalNetworkSubnetsText"
          label="Local network subnets"
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
              <td>Server name</td>
              <td>{{ systemInfo.ServerName }}</td>
            </tr>
            <tr>
              <td>Local address</td>
              <td>{{ systemInfo.LocalAddress }}</td>
            </tr>
            <tr>
              <td>Version</td>
              <td>{{ systemInfo.Version }}</td>
            </tr>
            <tr>
              <td>In network</td>
              <td>{{ endpointInfo.IsInNetwork ? 'Yes' : 'No' }}</td>
            </tr>
            <tr>
              <td>Local only</td>
              <td>{{ endpointInfo.IsLocal ? 'Yes' : 'No' }}</td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>Endpoint</th>
              <th>Type</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="domain in domains"
              :key="domain.url">
              <td>{{ domain.url }}</td>
              <td>{{ domain.isLocal ? 'Local' : 'Remote' }}</td>
            </tr>
            <tr v-if="!domains.length">
              <td
                colspan="2"
                class="uno-opacity-70">
                No endpoints available
              </td>
            </tr>
          </tbody>
        </VTable>

        <VTable class="uno-mt-4">
          <thead>
            <tr>
              <th>Wake on LAN</th>
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
                No Wake on LAN targets configured
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
import RemotePluginAxiosInstance from '#/plugins/remote/axios.ts';
import { useServerConfiguration } from '#/composables/server-configuration.ts';

type SystemInfo = {
  ServerName?: string;
  LocalAddress?: string;
  Version?: string;
};

type EndpointInfo = {
  IsLocal?: boolean;
  IsInNetwork?: boolean;
};

type DomainInfo = {
  name?: string;
  url: string;
  isLocal?: boolean;
  isRemote?: boolean;
};

const { configuration, saving } = await useServerConfiguration({
  EnableRemoteAccess: true,
  EnableUPnP: false,
  PublicUrl: '',
  PublicPort: 8096,
  HttpsPortNumber: 8920,
  LocalNetworkSubnetsText: ''
});

const [
  systemInfoResponse,
  endpointInfoResponse,
  domainsResponse,
  wakeOnLanInfoResponse
] = await Promise.all([
  RemotePluginAxiosInstance.instance.get<SystemInfo>('/System/Info'),
  RemotePluginAxiosInstance.instance.get<EndpointInfo>('/System/Endpoint'),
  RemotePluginAxiosInstance.instance.get<{ data?: DomainInfo[] }>('/System/Ext/ServerDomains'),
  RemotePluginAxiosInstance.instance.get<unknown[]>('/System/WakeOnLanInfo')
]);

const systemInfo = systemInfoResponse.data;
const endpointInfo = endpointInfoResponse.data;
const domains = computed(() => domainsResponse.data.data ?? []);
const wakeOnLanInfo = computed(() => wakeOnLanInfoResponse.data ?? []);

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
