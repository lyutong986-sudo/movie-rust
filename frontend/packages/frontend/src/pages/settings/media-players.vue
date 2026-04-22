<template>
  <SettingsPage>
    <template #title>
      {{ $t('mediaPlayers') }}
    </template>

    <template #content>
      <VCol
        md="8"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <thead>
            <tr>
              <th>名称</th>
              <th>客户端</th>
              <th>用户</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="session in sessions"
              :key="session.Id">
              <td>{{ session.DeviceName || session.DeviceId }}</td>
              <td>{{ session.Client }} {{ session.ApplicationVersion }}</td>
              <td>{{ session.UserName }}</td>
              <td>{{ session.NowPlayingItem ? '正在播放' : '空闲' }}</td>
            </tr>
          </tbody>
        </VTable>
      </VCol>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import RemotePluginAxiosInstance from '#/plugins/remote/axios.ts';

type SessionInfo = Record<string, any>;

const sessions = (await RemotePluginAxiosInstance.instance.get<SessionInfo[]>('/Sessions')).data;
</script>
