<template>
  <SettingsPage>
    <template #title>
      {{ $t('mediaPlayers') }}
    </template>

    <template #actions>
      <VBtn
        variant="elevated"
        :loading="reloading"
        @click="reloadSessions">
        Reload
      </VBtn>
    </template>

    <template #content>
      <VCol
        md="8"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <thead>
            <tr>
              <th>Device</th>
              <th>Client</th>
              <th>User</th>
              <th>Playback</th>
              <th>Capabilities</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="session in sessions"
              :key="session.Id">
              <td>
                <div class="uno-font-medium">
                  {{ session.DeviceName || session.DeviceId }}
                </div>
                <div class="uno-text-sm uno-opacity-70">
                  {{ session.DeviceId }}
                </div>
              </td>
              <td>
                <div>{{ session.Client }} {{ session.ApplicationVersion }}</div>
                <div class="uno-text-sm uno-opacity-70">
                  {{ session.RemoteEndPoint || 'Local' }}
                </div>
              </td>
              <td>{{ session.UserName }}</td>
              <td>
                <div>{{ session.NowPlayingItem?.Name ?? 'Idle' }}</div>
                <div class="uno-text-sm uno-opacity-70">
                  {{ session.PlayState?.IsPaused ? 'Paused' : session.NowPlayingItem ? 'Playing' : '-' }}
                </div>
              </td>
              <td>
                <div>{{ session.SupportsRemoteControl ? 'Remote control' : 'Read only' }}</div>
                <div class="uno-text-sm uno-opacity-70">
                  {{ (session.SupportedCommands ?? []).slice(0, 3).join(', ') || '-' }}
                </div>
              </td>
              <td class="uno-text-right">
                <VBtn
                  size="small"
                  variant="tonal"
                  class="uno-mr-2"
                  :disabled="busyId === session.Id"
                  @click="openMessageDialog(session)">
                  Message
                </VBtn>
                <VBtn
                  size="small"
                  variant="tonal"
                  class="uno-mr-2"
                  :loading="busyId === session.Id && busyAction === 'pause'"
                  :disabled="!session.SupportsRemoteControl"
                  @click="sendCommand(session.Id, 'Pause', 'pause')">
                  Pause
                </VBtn>
                <VBtn
                  size="small"
                  variant="tonal"
                  color="error"
                  :loading="busyId === session.Id && busyAction === 'stop'"
                  :disabled="!session.SupportsRemoteControl"
                  @click="sendCommand(session.Id, 'Stop', 'stop')">
                  Stop
                </VBtn>
              </td>
            </tr>
            <tr v-if="!sessions.length">
              <td
                colspan="6"
                class="uno-opacity-70">
                No active media players
              </td>
            </tr>
          </tbody>
        </VTable>
      </VCol>

      <VDialog
        width="520"
        :model-value="!!messageTarget"
        @update:model-value="messageTarget = undefined">
        <VCard>
          <VCardTitle>Send Message</VCardTitle>
          <VCardText>
            <VTextField
              v-model="messageTitle"
              label="Title" />
            <VTextarea
              v-model="messageBody"
              class="uno-mt-3"
              rows="4"
              label="Message" />
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn @click="messageTarget = undefined">
              Cancel
            </VBtn>
            <VBtn
              color="primary"
              :loading="busyId === messageTarget?.Id && busyAction === 'message'"
              @click="sendMessage">
              Send
            </VBtn>
          </VCardActions>
        </VCard>
      </VDialog>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import RemotePluginAxiosInstance from '#/plugins/remote/axios.ts';

type SessionInfo = {
  Id: string;
  UserName?: string;
  Client?: string;
  DeviceId?: string;
  DeviceName?: string;
  ApplicationVersion?: string;
  RemoteEndPoint?: string;
  SupportsRemoteControl?: boolean;
  SupportedCommands?: string[];
  NowPlayingItem?: {
    Name?: string;
  };
  PlayState?: {
    IsPaused?: boolean;
  };
};

const sessions = ref<SessionInfo[]>([]);
const reloading = ref(false);
const busyId = ref<string>();
const busyAction = ref<string>();
const messageTarget = ref<SessionInfo>();
const messageTitle = ref('');
const messageBody = ref('');

async function reloadSessions(): Promise<void> {
  reloading.value = true;
  try {
    sessions.value = (await RemotePluginAxiosInstance.instance.get<SessionInfo[]>('/Sessions')).data;
  } finally {
    reloading.value = false;
  }
}

async function sendCommand(id: string, command: string, action: string): Promise<void> {
  busyId.value = id;
  busyAction.value = action;
  try {
    await RemotePluginAxiosInstance.instance.post(`/Sessions/${id}/Playing/${command}`, {
      Name: command,
      Command: command
    });
    await reloadSessions();
  } finally {
    busyId.value = undefined;
    busyAction.value = undefined;
  }
}

function openMessageDialog(session: SessionInfo): void {
  messageTarget.value = session;
  messageTitle.value = '';
  messageBody.value = '';
}

async function sendMessage(): Promise<void> {
  if (!messageTarget.value?.Id) {
    return;
  }

  busyId.value = messageTarget.value.Id;
  busyAction.value = 'message';
  try {
    await RemotePluginAxiosInstance.instance.post(`/Sessions/${messageTarget.value.Id}/Message`, {
      Header: messageTitle.value,
      Text: messageBody.value,
      TimeoutMs: 5000
    });
    messageTarget.value = undefined;
  } finally {
    busyId.value = undefined;
    busyAction.value = undefined;
  }
}

await reloadSessions();
</script>
