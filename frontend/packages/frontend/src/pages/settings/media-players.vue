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
        {{ t('reload') }}
      </VBtn>
    </template>

    <template #content>
      <VCol
        md="8"
        class="uno-pb-4 uno-pt-0">
        <VTable>
          <thead>
            <tr>
              <th>{{ t('device') }}</th>
              <th>{{ t('client') }}</th>
              <th>{{ t('user') }}</th>
              <th>{{ t('playback') }}</th>
              <th>{{ t('capabilities') }}</th>
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
                  {{ session.RemoteEndPoint || t('local') }}
                </div>
              </td>
              <td>{{ session.UserName }}</td>
              <td>
                <div>{{ session.NowPlayingItem?.Name ?? t('idle') }}</div>
                <div class="uno-text-sm uno-opacity-70">
                  {{ session.PlayState?.IsPaused ? t('paused') : session.NowPlayingItem ? t('playing') : '-' }}
                </div>
              </td>
              <td>
                <div>{{ session.SupportsRemoteControl ? t('remoteControl') : t('readOnly') }}</div>
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
                  {{ t('message') }}
                </VBtn>
                <VBtn
                  size="small"
                  variant="tonal"
                  class="uno-mr-2"
                  :loading="busyId === session.Id && busyAction === 'pause'"
                  :disabled="!session.SupportsRemoteControl"
                  @click="sendCommand(session.Id, 'Pause', 'pause')">
                  {{ t('pause') }}
                </VBtn>
                <VBtn
                  size="small"
                  variant="tonal"
                  color="error"
                  :loading="busyId === session.Id && busyAction === 'stop'"
                  :disabled="!session.SupportsRemoteControl"
                  @click="sendCommand(session.Id, 'Stop', 'stop')">
                  {{ t('stop') }}
                </VBtn>
              </td>
            </tr>
            <tr v-if="!sessions.length">
              <td
                colspan="6"
                class="uno-opacity-70">
                {{ t('noActiveMediaPlayers') }}
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
          <VCardTitle>{{ t('sendMessage') }}</VCardTitle>
          <VCardText>
            <VTextField
              v-model="messageTitle"
              :label="t('title')" />
            <VTextarea
              v-model="messageBody"
              class="uno-mt-3"
              rows="4"
              :label="t('message')" />
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn @click="messageTarget = undefined">
              {{ t('cancel') }}
            </VBtn>
            <VBtn
              color="primary"
              :loading="busyId === messageTarget?.Id && busyAction === 'message'"
              @click="sendMessage">
              {{ t('send') }}
            </VBtn>
          </VCardActions>
        </VCard>
      </VDialog>
    </template>
  </SettingsPage>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { useTranslation } from 'i18next-vue';
import { useSettingsSdk, type SettingsSessionInfo } from '#/composables/use-settings-sdk.ts';

const { t } = useTranslation();
const { sessionsApi } = useSettingsSdk();
const sessions = ref<SettingsSessionInfo[]>([]);
const reloading = ref(false);
const busyId = ref<string>();
const busyAction = ref<string>();
const messageTarget = ref<SettingsSessionInfo>();
const messageTitle = ref('');
const messageBody = ref('');

async function reloadSessions(): Promise<void> {
  reloading.value = true;
  try {
    sessions.value = await sessionsApi.getSessions();
  } finally {
    reloading.value = false;
  }
}

async function sendCommand(id: string, command: string, action: string): Promise<void> {
  busyId.value = id;
  busyAction.value = action;
  try {
    await sessionsApi.sendPlayingCommand(id, command);
    await reloadSessions();
  } finally {
    busyId.value = undefined;
    busyAction.value = undefined;
  }
}

function openMessageDialog(session: SettingsSessionInfo): void {
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
    await sessionsApi.sendMessage(messageTarget.value.Id, {
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
