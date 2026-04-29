<script setup lang="ts">
import { onMounted, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { ActivityLogEntry, LogFileDto } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(false);
const error = ref('');
const logs = ref<LogFileDto[]>([]);
const activity = ref<ActivityLogEntry[]>([]);

const logViewerOpen = ref(false);
const logViewerTitle = ref('');
const logViewerContent = ref('');
const logLoading = ref(false);

onMounted(async () => {
  if (!isAdmin.value) {
    return;
  }
  loading.value = true;
  try {
    const [logFiles, activityResponse] = await Promise.all([api.serverLogs(), api.activity(50)]);
    logs.value = logFiles;
    activity.value = activityResponse.Items;
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : String(loadError);
  } finally {
    loading.value = false;
  }
});

function formatDate(value: string) {
  return new Date(value).toLocaleString('zh-CN');
}

async function viewLogFile(filename: string) {
  logViewerTitle.value = filename;
  logLoading.value = true;
  logViewerOpen.value = true;
  try {
    logViewerContent.value = await api.getLogFile(filename);
  } catch {
    logViewerContent.value = '无法加载日志文件';
  } finally {
    logLoading.value = false;
  }
}

function downloadLogFile() {
  const blob = new Blob([logViewerContent.value], { type: 'text/plain;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = logViewerTitle.value;
  a.click();
  URL.revokeObjectURL(url);
}
</script>

<template>
  <SettingsLayout>
    <div
      v-if="!isAdmin"
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
    >
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能查看活动流和服务端日志。</p>
    </div>

    <div v-else-if="loading" class="flex min-h-[30vh] flex-col items-center justify-center gap-2">
      <UProgress animation="carousel" class="w-48" />
      <p class="text-muted text-sm">正在读取播放活动和日志列表…</p>
    </div>

    <UAlert v-else-if="error" color="error" icon="i-lucide-triangle-alert" title="加载失败" :description="error" />

    <div v-else class="grid gap-4 lg:grid-cols-2">
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">日志</h3>
            <UBadge variant="subtle" color="neutral">{{ logs.length }}</UBadge>
          </div>
        </template>
        <div v-if="logs.length" class="space-y-2">
          <button
            v-for="log in logs"
            :key="log.Name"
            type="button"
            class="flex w-full items-center gap-3 rounded-lg border border-default p-3 text-start transition hover:bg-elevated/70 hover:ring-1 hover:ring-primary/40"
            @click="viewLogFile(log.Name)"
          >
            <UIcon name="i-lucide-file-text" class="text-primary size-4" />
            <div class="min-w-0 flex-1">
              <p class="text-highlighted truncate text-sm font-medium">{{ log.Name }}</p>
              <p class="text-muted text-xs">{{ formatDate(log.DateModified) }}</p>
            </div>
            <UIcon name="i-lucide-eye" class="size-4 shrink-0 text-dimmed" />
          </button>
        </div>
        <p v-else class="text-muted text-sm">当前版本暂未输出独立日志文件列表。</p>
      </UCard>

      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">活动</h3>
            <UBadge variant="subtle" color="neutral">{{ activity.length }}</UBadge>
          </div>
        </template>
        <div v-if="activity.length" class="space-y-3">
          <div
            v-for="entry in activity"
            :key="entry.Id"
            class="flex items-start gap-3 rounded-lg border border-default p-3"
          >
            <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <UIcon name="i-lucide-activity" class="size-4" />
            </div>
            <div class="min-w-0 flex-1">
              <p class="text-highlighted truncate text-sm font-medium">{{ entry.Name }}</p>
              <p class="text-muted truncate text-xs">{{ entry.ShortOverview || entry.Type }}</p>
              <p class="text-dimmed mt-1 text-[11px]">{{ formatDate(entry.Date) }}</p>
            </div>
          </div>
        </div>
        <p v-else class="text-muted text-sm">暂时没有播放活动记录。</p>
      </UCard>
    </div>

    <UModal v-model:open="logViewerOpen">
      <template #content>
        <div class="flex flex-col" style="max-height: 85vh;">
          <div class="flex items-center justify-between border-b border-default px-5 py-4">
            <h3 class="text-highlighted truncate text-lg font-semibold">{{ logViewerTitle }}</h3>
            <div class="flex items-center gap-2">
              <UButton
                variant="soft"
                color="neutral"
                size="sm"
                icon="i-lucide-download"
                :disabled="logLoading || !logViewerContent"
                @click="downloadLogFile"
              >
                下载
              </UButton>
              <UButton variant="ghost" color="neutral" size="sm" icon="i-lucide-x" @click="logViewerOpen = false" />
            </div>
          </div>
          <div class="flex-1 overflow-auto p-4">
            <div v-if="logLoading" class="flex min-h-[20vh] items-center justify-center">
              <UProgress animation="carousel" class="w-48" />
            </div>
            <pre
              v-else
              class="whitespace-pre overflow-x-auto rounded-lg bg-gray-950 p-4 text-xs leading-relaxed text-gray-300"
              style="font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', 'Consolas', monospace; tab-size: 4;"
            >{{ logViewerContent }}</pre>
          </div>
          <div class="flex justify-end border-t border-default px-5 py-3">
            <UButton variant="soft" color="neutral" @click="logViewerOpen = false">关闭</UButton>
          </div>
        </div>
      </template>
    </UModal>
  </SettingsLayout>
</template>
