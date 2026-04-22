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
          label="允许远程连接" />
        <VCheckbox
          v-model="configuration.EnableUPnP"
          label="启用自动端口映射" />
        <VTextField
          v-model="configuration.PublicUrl"
          label="公开访问地址" />
        <VTextField
          v-model.number="configuration.PublicPort"
          label="HTTP 端口"
          type="number" />
        <VTextField
          v-model.number="configuration.HttpsPortNumber"
          label="HTTPS 端口"
          type="number" />
        <VTextarea
          v-model="configuration.LocalNetworkSubnetsText"
          label="本地网络网段"
          rows="3" />
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
import { useServerConfiguration } from '#/composables/server-configuration.ts';

const { configuration, saving } = await useServerConfiguration({
  EnableRemoteAccess: true,
  EnableUPnP: false,
  PublicUrl: '',
  PublicPort: 8096,
  HttpsPortNumber: 8920,
  LocalNetworkSubnetsText: ''
});
</script>
