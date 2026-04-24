<script setup lang="ts">
import { onMounted, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { SessionInfo } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(false);
const error = ref('');
const devices = ref<SessionInfo[]>([]);

onMounted(async () => {
  if (!isAdmin.value) {
    return;
  }

  loading.value = true;
  try {
    devices.value = await api.sessions();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : String(loadError);
  } finally {
    loading.value = false;
  }
});

function formatDate(value: string) {
  return new Date(value).toLocaleString('zh-CN');
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
      <p class="text-muted text-sm">当前账户不能查看已连接会话列表。</p>
    </div>

    <div v-else-if="loading" class="flex min-h-[30vh] flex-col items-center justify-center gap-2">
      <UProgress animation="carousel" class="w-48" />
      <p class="text-muted text-sm">正在读取当前服务器上的会话设备…</p>
    </div>

    <UAlert v-else-if="error" color="error" icon="i-lucide-triangle-alert" title="加载失败" :description="error" />

    <div v-else class="space-y-4">
      <div class="flex items-center justify-between">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">Sessions</p>
          <h2 class="text-highlighted text-xl font-semibold">活动会话</h2>
        </div>
        <UBadge variant="subtle" color="primary">{{ devices.length }} 个会话</UBadge>
      </div>

      <UCard v-if="devices.length" :ui="{ body: 'p-0' }">
        <div class="divide-y divide-default">
          <div
            v-for="device in devices"
            :key="device.Id"
            class="grid grid-cols-1 gap-2 p-4 sm:grid-cols-[1.2fr_2fr_1fr] sm:items-center"
          >
            <div class="flex items-center gap-3">
              <UAvatar :alt="device.UserName" :text="(device.UserName || '?').slice(0, 1).toUpperCase()" size="sm" />
              <div>
                <p class="text-highlighted text-sm font-medium">{{ device.UserName }}</p>
                <p class="text-muted text-xs">{{ device.Id.slice(0, 8) }}…</p>
              </div>
            </div>
            <div class="min-w-0">
              <p class="text-highlighted truncate text-sm">{{ device.DeviceName }}</p>
              <p class="text-muted truncate text-xs">{{ device.Client }}</p>
            </div>
            <div class="text-muted text-xs">
              <UIcon name="i-lucide-clock" class="mr-1 inline size-3" />
              {{ formatDate(device.LastActivityDate) }}
            </div>
          </div>
        </div>
      </UCard>
      <div v-else class="rounded-xl border border-dashed border-default p-10 text-center">
        <UIcon name="i-lucide-monitor-off" class="mx-auto size-10 text-muted" />
        <p class="text-muted mt-2 text-sm">当前没有已连接的会话</p>
      </div>
    </div>
  </SettingsLayout>
</template>
