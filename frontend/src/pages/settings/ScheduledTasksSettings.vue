<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { ScheduledTaskInfo } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const tasks = ref<ScheduledTaskInfo[]>([]);
const loading = ref(true);
const error = ref('');
const saved = ref('');
let pollTimer = 0;

const runningCount = computed(
  () => tasks.value.filter((task) => task.State === 'Running').length
);

const idleCount = computed(() => tasks.value.filter((task) => task.State === 'Idle').length);

const failedCount = computed(
  () =>
    tasks.value.filter(
      (task) => task.LastExecutionResult?.Status && task.LastExecutionResult.Status !== 'Completed'
    ).length
);

function formatTime(value?: string | null) {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '-';
  return date.toLocaleString();
}

function ticksToMinutes(ticks?: number) {
  if (!ticks) return '-';
  const seconds = Math.round(ticks / 10_000_000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟`;
  const hours = Math.round((minutes / 60) * 10) / 10;
  return `${hours} 小时`;
}

function triggerLabel(task: ScheduledTaskInfo) {
  if (!task.Triggers?.length) return '手动';
  return task.Triggers.map((trigger) => {
    if (trigger.Type === 'IntervalTrigger') {
      return `每 ${ticksToMinutes(trigger.IntervalTicks)} 执行一次`;
    }
    if (trigger.Type === 'DailyTrigger' && trigger.TimeOfDayTicks !== undefined) {
      const totalSeconds = Math.round((trigger.TimeOfDayTicks || 0) / 10_000_000);
      const hh = String(Math.floor(totalSeconds / 3600)).padStart(2, '0');
      const mm = String(Math.floor((totalSeconds % 3600) / 60)).padStart(2, '0');
      return `每天 ${hh}:${mm}`;
    }
    if (trigger.Type === 'WeeklyTrigger') {
      return `每 ${trigger.DayOfWeek || '周一'}`;
    }
    return trigger.Type;
  }).join(' / ');
}

function stateColor(state: string) {
  if (state === 'Running') return 'warning';
  if (state === 'Cancelled') return 'error';
  if (state === 'Idle') return 'neutral';
  return 'primary';
}

async function load() {
  if (!isAdmin.value) return;
  loading.value = true;
  error.value = '';
  try {
    tasks.value = await api.scheduledTasks();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

async function runTask(task: ScheduledTaskInfo) {
  saved.value = '';
  error.value = '';
  try {
    await api.startScheduledTask(task.Id);
    saved.value = `已触发任务：${task.Name}`;
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

async function cancelTask(task: ScheduledTaskInfo) {
  saved.value = '';
  error.value = '';
  try {
    await api.cancelScheduledTask(task.Id);
    saved.value = `已请求取消：${task.Name}`;
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

function startPolling() {
  stopPolling();
  pollTimer = window.setInterval(load, 5000);
}

function stopPolling() {
  if (pollTimer) {
    window.clearInterval(pollTimer);
    pollTimer = 0;
  }
}

onMounted(async () => {
  await load();
  startPolling();
});

onBeforeUnmount(() => {
  stopPolling();
});
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能查看或触发计划任务。</p>
    </div>

    <div v-else class="space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">Scheduled Tasks</p>
          <h2 class="text-highlighted text-xl font-semibold">计划任务</h2>
        </div>
        <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-cw" :loading="loading" @click="load">
          刷新
        </UButton>
      </div>

      <div class="grid gap-3 sm:grid-cols-3">
        <UCard variant="soft">
          <p class="text-muted text-xs">总任务数</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ tasks.length }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">运行中</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ runningCount }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">最近失败</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ failedCount }}</p>
          <p class="text-muted text-xs">{{ idleCount }} 个空闲</p>
        </UCard>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <div class="grid gap-3">
        <UCard v-for="task in tasks" :key="task.Id">
          <template #header>
            <div class="flex flex-wrap items-center justify-between gap-3">
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <h3 class="text-highlighted text-base font-semibold">{{ task.Name }}</h3>
                  <UBadge variant="subtle" color="primary" size="xs">{{ task.Category }}</UBadge>
                  <UBadge variant="soft" :color="stateColor(task.State)" size="xs">
                    {{ task.State }}
                  </UBadge>
                </div>
                <p class="text-muted mt-1 text-xs">{{ task.Description }}</p>
              </div>
              <div class="flex gap-2">
                <UButton
                  v-if="task.State === 'Running'"
                  color="error"
                  variant="soft"
                  size="sm"
                  icon="i-lucide-square"
                  @click="cancelTask(task)"
                >
                  取消
                </UButton>
                <UButton
                  v-else
                  color="primary"
                  variant="soft"
                  size="sm"
                  icon="i-lucide-play"
                  @click="runTask(task)"
                >
                  立即运行
                </UButton>
              </div>
            </div>
          </template>

          <div class="grid gap-3 md:grid-cols-3">
            <div class="rounded-lg border border-default p-3">
              <p class="text-muted text-xs">当前进度</p>
              <p class="text-highlighted mt-1 text-lg font-semibold">
                {{ task.State === 'Running'
                  ? `${Math.round(task.CurrentProgressPercentage || 0)}%`
                  : '-' }}
              </p>
              <UProgress
                v-if="task.State === 'Running'"
                class="mt-2"
                :model-value="task.CurrentProgressPercentage || 0"
                :max="100"
                color="warning"
              />
            </div>
            <div class="rounded-lg border border-default p-3">
              <p class="text-muted text-xs">触发方式</p>
              <p class="text-highlighted mt-1 text-sm font-medium">{{ triggerLabel(task) }}</p>
            </div>
            <div class="rounded-lg border border-default p-3">
              <p class="text-muted text-xs">最近结果</p>
              <p class="text-highlighted mt-1 text-sm font-medium">
                {{ task.LastExecutionResult?.Status || '尚未执行' }}
              </p>
              <p class="text-muted text-xs">
                {{ formatTime(task.LastExecutionResult?.EndTimeUtc || task.LastExecutionResult?.EndTime) }}
              </p>
            </div>
          </div>

          <UAlert
            v-if="task.LastExecutionResult?.ErrorMessage"
            class="mt-3"
            color="error"
            icon="i-lucide-badge-alert"
            :description="task.LastExecutionResult.ErrorMessage"
          />
        </UCard>

        <div
          v-if="!tasks.length && !loading"
          class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
        >
          <UIcon name="i-lucide-timer" class="size-10 text-muted" />
          <p class="text-muted text-sm">当前没有计划任务。</p>
        </div>
      </div>
    </div>
  </SettingsLayout>
</template>
