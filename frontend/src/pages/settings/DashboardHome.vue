<script setup lang="ts">
import { onMounted, ref, computed } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import { api, isAdmin, libraries, totalLibraryItems } from '../../store/app';
import type { SystemInfo, SessionInfo, ScheduledTaskInfo, ActivityLogEntry } from '../../api/emby';

const loading = ref(true);
const error = ref('');
const restarting = ref(false);
const shuttingDown = ref(false);
const confirmRestart = ref(false);
const confirmShutdown = ref(false);

const sysInfo = ref<SystemInfo | null>(null);
const sessions = ref<SessionInfo[]>([]);
const tasks = ref<ScheduledTaskInfo[]>([]);
const activityItems = ref<ActivityLogEntry[]>([]);

const activeSessions = computed(() => sessions.value.filter((s) => s.IsActive));
const runningTasks = computed(() => tasks.value.filter((t) => t.State === 'Running'));

function formatDate(dateStr?: string) {
  if (!dateStr) return '';
  const d = new Date(dateStr);
  return d.toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
}

function taskStateColor(state: string) {
  if (state === 'Running') return 'info';
  if (state === 'Idle') return 'neutral';
  if (state === 'Cancelling') return 'warning';
  return 'neutral';
}

function severityColor(severity: string) {
  if (severity === 'Error') return 'error';
  if (severity === 'Warn' || severity === 'Warning') return 'warning';
  if (severity === 'Information' || severity === 'Info') return 'info';
  return 'neutral';
}

async function doRestart() {
  restarting.value = true;
  confirmRestart.value = false;
  try {
    await api.restartServer();
  } catch { /* server will drop connection */ }
}

async function doShutdown() {
  shuttingDown.value = true;
  confirmShutdown.value = false;
  try {
    await api.shutdownServer();
  } catch { /* server will drop connection */ }
}

onMounted(async () => {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  try {
    const [info, sess, scheduledTasks, activityResult] = await Promise.all([
      api.systemInfo(),
      api.sessions(),
      api.scheduledTasks(),
      api.activity(20)
    ]);
    sysInfo.value = info;
    sessions.value = sess;
    tasks.value = scheduledTasks;
    activityItems.value = activityResult.Items || [];
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
});
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户无法查看仪表盘。</p>
    </div>

    <div v-else-if="loading" class="flex items-center justify-center py-20">
      <UIcon name="i-lucide-loader-2" class="size-8 animate-spin text-muted" />
    </div>

    <div v-else class="space-y-6">
      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />

      <!-- 服务器信息 -->
      <div class="grid gap-4 sm:grid-cols-3">
        <UCard variant="soft">
          <p class="text-muted text-xs">服务器名称</p>
          <p class="text-highlighted mt-1 text-base font-semibold">{{ sysInfo?.ServerName || '—' }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">版本</p>
          <p class="text-highlighted mt-1 text-base font-semibold">{{ sysInfo?.Version || '—' }}</p>
          <p class="text-muted text-xs">{{ sysInfo?.ProductName || '' }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">操作系统</p>
          <p class="text-highlighted mt-1 text-base font-semibold">{{ sysInfo?.OperatingSystem || '—' }}</p>
        </UCard>
      </div>

      <!-- 服务器控制 -->
      <div class="flex flex-wrap gap-2">
        <UButton
          color="warning"
          variant="soft"
          icon="i-lucide-refresh-cw"
          :loading="restarting"
          :disabled="shuttingDown"
          @click="confirmRestart = true"
        >
          重启服务器
        </UButton>
        <UButton
          color="error"
          variant="soft"
          icon="i-lucide-power"
          :loading="shuttingDown"
          :disabled="restarting"
          @click="confirmShutdown = true"
        >
          关闭服务器
        </UButton>
      </div>

      <!-- 重启确认 -->
      <UModal v-model:open="confirmRestart">
        <template #content>
          <div class="p-6 space-y-4">
            <h3 class="text-highlighted text-lg font-semibold">确认重启</h3>
            <p class="text-muted text-sm">即将重启服务器，所有活跃会话将被中断。确定继续吗？</p>
            <div class="flex justify-end gap-2">
              <UButton color="neutral" variant="outline" @click="confirmRestart = false">取消</UButton>
              <UButton color="warning" @click="doRestart">确认重启</UButton>
            </div>
          </div>
        </template>
      </UModal>

      <!-- 关机确认 -->
      <UModal v-model:open="confirmShutdown">
        <template #content>
          <div class="p-6 space-y-4">
            <h3 class="text-highlighted text-lg font-semibold">确认关机</h3>
            <p class="text-muted text-sm">即将关闭服务器，关机后需要手动重新启动。确定继续吗？</p>
            <div class="flex justify-end gap-2">
              <UButton color="neutral" variant="outline" @click="confirmShutdown = false">取消</UButton>
              <UButton color="error" @click="doShutdown">确认关机</UButton>
            </div>
          </div>
        </template>
      </UModal>

      <div class="grid gap-6 lg:grid-cols-2">
        <!-- 活跃会话 -->
        <UCard>
          <template #header>
            <div class="flex items-center justify-between">
              <h3 class="text-highlighted text-sm font-semibold">活跃会话</h3>
              <UBadge color="info" variant="subtle">{{ activeSessions.length }}</UBadge>
            </div>
          </template>
          <div v-if="activeSessions.length === 0" class="text-muted py-4 text-center text-sm">
            暂无活跃会话
          </div>
          <div v-else class="divide-y divide-default">
            <div v-for="session in activeSessions" :key="session.Id" class="flex items-center gap-3 py-3 first:pt-0 last:pb-0">
              <UIcon name="i-lucide-user" class="size-5 shrink-0 text-muted" />
              <div class="min-w-0 flex-1">
                <p class="text-highlighted text-sm font-medium truncate">{{ session.UserName || '未知用户' }}</p>
                <p class="text-muted text-xs truncate">{{ session.DeviceName }} · {{ session.Client }}</p>
              </div>
            </div>
          </div>
        </UCard>

        <!-- 媒体库统计 -->
        <UCard>
          <template #header>
            <h3 class="text-highlighted text-sm font-semibold">媒体库统计</h3>
          </template>
          <div class="grid grid-cols-2 gap-4">
            <div class="text-center">
              <p class="text-3xl font-bold text-highlighted">{{ libraries.length }}</p>
              <p class="text-muted text-xs mt-1">媒体库数量</p>
            </div>
            <div class="text-center">
              <p class="text-3xl font-bold text-highlighted">{{ totalLibraryItems }}</p>
              <p class="text-muted text-xs mt-1">总条目数</p>
            </div>
          </div>
          <div v-if="libraries.length" class="mt-4 divide-y divide-default">
            <div v-for="lib in libraries" :key="lib.Id" class="flex items-center justify-between py-2 first:pt-0 last:pb-0">
              <span class="text-sm text-highlighted truncate">{{ lib.Name }}</span>
              <UBadge color="neutral" variant="subtle" size="sm">{{ lib.ChildCount || 0 }}</UBadge>
            </div>
          </div>
        </UCard>
      </div>

      <!-- 运行中的任务 -->
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">计划任务</h3>
            <UBadge v-if="runningTasks.length" color="info" variant="subtle">{{ runningTasks.length }} 运行中</UBadge>
          </div>
        </template>
        <div v-if="tasks.length === 0" class="text-muted py-4 text-center text-sm">
          暂无计划任务
        </div>
        <div v-else class="divide-y divide-default">
          <div v-for="task in tasks" :key="task.Id" class="py-3 first:pt-0 last:pb-0">
            <div class="flex items-center justify-between gap-2">
              <div class="min-w-0 flex-1">
                <p class="text-highlighted text-sm font-medium truncate">{{ task.Name }}</p>
                <p class="text-muted text-xs">{{ task.Category }}</p>
              </div>
              <UBadge :color="taskStateColor(task.State)" variant="subtle" size="sm">{{ task.State }}</UBadge>
            </div>
            <UProgress
              v-if="task.State === 'Running' && task.CurrentProgressPercentage != null"
              :value="task.CurrentProgressPercentage"
              :max="100"
              size="xs"
              class="mt-2"
            />
          </div>
        </div>
      </UCard>

      <!-- 最近活动 -->
      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">最近活动</h3>
        </template>
        <div v-if="activityItems.length === 0" class="text-muted py-4 text-center text-sm">
          暂无活动记录
        </div>
        <div v-else class="divide-y divide-default">
          <div v-for="entry in activityItems" :key="entry.Id" class="flex items-start gap-3 py-3 first:pt-0 last:pb-0">
            <UBadge :color="severityColor(entry.Severity)" variant="subtle" size="sm" class="mt-0.5 shrink-0">
              {{ entry.Severity }}
            </UBadge>
            <div class="min-w-0 flex-1">
              <p class="text-highlighted text-sm truncate">{{ entry.Name }}</p>
              <p v-if="entry.ShortOverview" class="text-muted text-xs truncate">{{ entry.ShortOverview }}</p>
            </div>
            <span class="text-muted text-xs shrink-0 whitespace-nowrap">{{ formatDate(entry.Date) }}</span>
          </div>
        </div>
      </UCard>
    </div>
  </SettingsLayout>
</template>
