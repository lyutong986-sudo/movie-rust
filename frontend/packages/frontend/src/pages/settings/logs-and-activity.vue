<template>
  <SettingsPage>
    <template #title>
      {{ t('logsAndActivity') }}
    </template>

    <template #actions>
      <VBtn
        variant="elevated"
        :loading="refreshing"
        @click="refreshData">
        {{ t('refresh') }}
      </VBtn>
    </template>

    <template #content>
      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <h2 class="text-h6 uno-mb-2">
          {{ t('logs') }}
        </h2>
        <VTable v-if="logs.length">
          <thead>
            <tr>
              <th>{{ t('name') }}</th>
              <th>{{ t('modified') }}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="file in logs"
              :key="file.Name ?? undefined">
              <td>{{ file.Name }}</td>
              <td>{{ getFormattedLogDate(file.DateModified) }}</td>
              <td class="uno-text-right">
                <VBtn
                  variant="tonal"
                  size="small"
                  class="uno-mr-2"
                  @click="previewLog(file.Name ?? '')">
                  {{ t('preview') }}
                </VBtn>
                <VBtn
                  variant="tonal"
                  size="small"
                  :href="getLogFileLink(file.Name ?? '')"
                  target="_blank"
                  rel="noopener">
                  {{ t('open') }}
                </VBtn>
              </td>
            </tr>
          </tbody>
        </VTable>
        <VCard v-else>
          <VCardTitle>
            {{ t('noLogsFound') }}
          </VCardTitle>
        </VCard>
      </VCol>

      <VCol
        md="6"
        class="uno-pb-4 uno-pt-0">
        <h2 class="text-h6 uno-mb-2">
          {{ t('activity') }}
        </h2>
        <VList
          v-if="activityList.length"
          lines="two"
          class="uno-mb-2">
          <VListItem
            v-for="activity in activityList"
            :key="activity.Id"
            :title="activity.Name"
            :subtitle="activity.ShortOverview ?? undefined">
            <template #prepend>
              <VAvatar :color="getColorFromSeverity(activity.Severity)">
                <JIcon :class="getIconFromActivityType(activity.Type)" />
              </VAvatar>
            </template>
            <template #append>
              <VListItemSubtitle class="text-capitalize-first-letter">
                {{ getFormattedActivityDate(activity.Date) }}
              </VListItemSubtitle>
            </template>
          </VListItem>
        </VList>
        <VCard v-else>
          <VCardTitle>
            {{ t('noActivityFound') }}
          </VCardTitle>
        </VCard>
      </VCol>

      <VDialog
        width="900"
        :model-value="!!previewName"
        @update:model-value="previewName = undefined">
        <VCard>
          <VCardTitle>{{ previewName }}</VCardTitle>
          <VCardText>
            <pre class="uno-max-h-120 uno-overflow-auto uno-whitespace-pre-wrap">{{ previewContent }}</pre>
          </VCardText>
          <VCardActions>
            <VSpacer />
            <VBtn @click="previewName = undefined">
              {{ t('close') }}
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
  ActivityLogEntry,
  LogFile,
  QueryResultString
} from '@jellyfin/sdk/lib/generated-client';
import { LogLevel } from '@jellyfin/sdk/lib/generated-client';
import { format, formatRelative, parseJSON } from 'date-fns';
import { ref } from 'vue';
import { useTranslation } from 'i18next-vue';
import { useTheme } from 'vuetify';
import { useSettingsSdk } from '#/composables/use-settings-sdk.ts';
import { useDateFns } from '#/composables/use-datefns.ts';

const { t } = useTranslation();
const theme = useTheme();
const { logsApi } = useSettingsSdk();

const refreshing = ref(false);
const previewName = ref<string>();
const previewContent = ref('');
const logs = ref<LogFile[]>([]);
const activityList = ref<ActivityLogEntry[]>([]);

function getColorFromSeverity(severity: LogLevel | undefined): string {
  switch (severity) {
    case LogLevel.Trace:
      return theme.current.value.colors.success;
    case LogLevel.Debug:
      return theme.current.value.colors.accent;
    case LogLevel.Information:
      return theme.current.value.colors.info;
    case LogLevel.Warning:
      return theme.current.value.colors.warning;
    case LogLevel.Error:
      return theme.current.value.colors.error;
    case LogLevel.Critical:
      return theme.current.value.colors.secondary;
    default:
      return theme.current.value.colors.primary;
  }
}

function getIconFromActivityType(type: string | undefined | null): string {
  switch (type) {
    case 'SessionStarted':
      return 'i-mdi:login';
    case 'SessionEnded':
      return 'i-mdi:logout';
    case 'UserPasswordChanged':
      return 'i-mdi:lock';
    case 'VideoPlayback':
      return 'i-mdi:play';
    case 'VideoPlaybackStopped':
      return 'i-mdi:stop';
    default:
      return 'i-mdi:help';
  }
}

function getFormattedActivityDate(date: string | undefined): string | undefined {
  return date
    ? useDateFns(formatRelative, parseJSON(date), new Date())
    : undefined;
}

function getFormattedLogDate(date: string | undefined): string | undefined {
  return date ? useDateFns(format, parseJSON(date), 'Ppp') : undefined;
}

function getLogFileLink(name: string): string | undefined {
  return logsApi.getLogFileUrl(name);
}

async function refreshData(): Promise<void> {
  refreshing.value = true;
  try {
    const [logsResponse, activityResponse] = await Promise.all([
      logsApi.getLogs(),
      logsApi.getActivityLogEntries()
    ]);
    logs.value = logsResponse;
    activityList.value = activityResponse.Items ?? [];
  } finally {
    refreshing.value = false;
  }
}

async function previewLog(name: string): Promise<void> {
  previewName.value = name;
  previewContent.value = t('loading');
  try {
    const response: QueryResultString = await logsApi.getLogLines(name);
    previewContent.value = response.Items?.join('\n') ?? '';
  } catch {
    previewContent.value = t('failedToLoadLogPreview');
  }
}

await refreshData();
</script>
