<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { ScheduledTaskInfo, ScheduledTaskTrigger } from '../../api/emby';
import { api, isAdmin } from '../../store/app';
import { useAppToast } from '../../composables/toast';

const toast = useAppToast();
const tasks = ref<ScheduledTaskInfo[]>([]);
const loading = ref(true);
const error = ref('');
let pollTimer = 0;

const runningCount = computed(
  () => tasks.value.filter((t) => t.State === 'Running').length
);
const idleCount = computed(() => tasks.value.filter((t) => t.State === 'Idle').length);
const failedCount = computed(
  () =>
    tasks.value.filter(
      (t) => t.LastExecutionResult?.Status && t.LastExecutionResult.Status !== 'Completed'
    ).length
);

const categories = computed(() => {
  const map = new Map<string, ScheduledTaskInfo[]>();
  for (const task of tasks.value) {
    const cat = task.Category || '其他';
    if (!map.has(cat)) map.set(cat, []);
    map.get(cat)!.push(task);
  }
  return Array.from(map.entries()).sort((a, b) => a[0].localeCompare(b[0]));
});

function formatTime(value?: string | null) {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '-';
  return date.toLocaleString();
}

function ticksToLabel(ticks?: number) {
  if (!ticks) return '-';
  const seconds = Math.round(ticks / 10_000_000);
  if (seconds < 60) return `${seconds} 秒`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟`;
  const hours = Math.round((minutes / 60) * 10) / 10;
  return `${hours} 小时`;
}

const CATEGORY_LABELS: Record<string, string> = {
  Library: '媒体库',
  Metadata: '元数据',
  Maintenance: '维护'
};

function categoryLabel(cat: string) {
  return CATEGORY_LABELS[cat] || cat;
}

const WEEKDAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
const WEEKDAY_LABELS: Record<string, string> = {
  Sunday: '周日',
  Monday: '周一',
  Tuesday: '周二',
  Wednesday: '周三',
  Thursday: '周四',
  Friday: '周五',
  Saturday: '周六'
};

function triggerLabel(trigger: ScheduledTaskTrigger) {
  if (trigger.Type === 'StartupTrigger') return '应用启动时';
  if (trigger.Type === 'IntervalTrigger') {
    return `每 ${ticksToLabel(trigger.IntervalTicks)}`;
  }
  if (trigger.Type === 'DailyTrigger' && trigger.TimeOfDayTicks !== undefined) {
    return `每天 ${ticksToTime(trigger.TimeOfDayTicks || 0)}`;
  }
  if (trigger.Type === 'WeeklyTrigger') {
    const day = WEEKDAY_LABELS[trigger.DayOfWeek || 'Monday'] || trigger.DayOfWeek || '周一';
    const time = ticksToTime(trigger.TimeOfDayTicks || 0);
    return `每${day} ${time}`;
  }
  return trigger.Type;
}

function ticksToTime(ticks: number) {
  const totalSeconds = Math.round(ticks / 10_000_000);
  const hh = String(Math.floor(totalSeconds / 3600)).padStart(2, '0');
  const mm = String(Math.floor((totalSeconds % 3600) / 60)).padStart(2, '0');
  return `${hh}:${mm}`;
}

function timeToTicks(time: string) {
  const [hh, mm] = time.split(':').map(Number);
  return ((hh || 0) * 3600 + (mm || 0) * 60) * 10_000_000;
}

function triggersLabel(task: ScheduledTaskInfo) {
  if (!task.Triggers?.length) return '手动';
  return task.Triggers.map(triggerLabel).join(' / ');
}

function stateColor(state: string) {
  if (state === 'Running') return 'warning';
  if (state === 'Cancelled') return 'error';
  return 'neutral';
}

function statusColor(status?: string) {
  if (status === 'Completed') return 'success';
  if (status === 'Failed') return 'error';
  if (status === 'Cancelled') return 'warning';
  return 'neutral';
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
  try {
    await api.startScheduledTask(task.Id);
    toast.success(`已触发任务：${task.Name}`);
    await load();
  } catch (err) {
    toast.error(err instanceof Error ? err.message : String(err));
  }
}

async function cancelTask(task: ScheduledTaskInfo) {
  try {
    await api.cancelScheduledTask(task.Id);
    toast.success(`已请求取消：${task.Name}`);
    await load();
  } catch (err) {
    toast.error(err instanceof Error ? err.message : String(err));
  }
}

// --- 触发器编辑 ---
const editingTaskId = ref<string | null>(null);
const editTriggers = ref<ScheduledTaskTrigger[]>([]);
const triggerSaving = ref(false);

const TRIGGER_TYPES = [
  { value: 'IntervalTrigger', label: '间隔触发' },
  { value: 'DailyTrigger', label: '每天定时' },
  { value: 'WeeklyTrigger', label: '每周定时' },
  { value: 'StartupTrigger', label: '启动时' }
];

const INTERVAL_OPTIONS = [
  { value: 15 * 60 * 10_000_000, label: '每 15 分钟' },
  { value: 30 * 60 * 10_000_000, label: '每 30 分钟' },
  { value: 45 * 60 * 10_000_000, label: '每 45 分钟' },
  { value: 1 * 3600 * 10_000_000, label: '每 1 小时' },
  { value: 2 * 3600 * 10_000_000, label: '每 2 小时' },
  { value: 3 * 3600 * 10_000_000, label: '每 3 小时' },
  { value: 4 * 3600 * 10_000_000, label: '每 4 小时' },
  { value: 6 * 3600 * 10_000_000, label: '每 6 小时' },
  { value: 8 * 3600 * 10_000_000, label: '每 8 小时' },
  { value: 12 * 3600 * 10_000_000, label: '每 12 小时' },
  { value: 24 * 3600 * 10_000_000, label: '每 24 小时' }
];

const MAX_RUNTIME_OPTIONS = [
  { value: 0, label: '无限制' },
  { value: 1 * 3600 * 10_000_000, label: '1 小时' },
  { value: 2 * 3600 * 10_000_000, label: '2 小时' },
  { value: 3 * 3600 * 10_000_000, label: '3 小时' },
  { value: 6 * 3600 * 10_000_000, label: '6 小时' },
  { value: 12 * 3600 * 10_000_000, label: '12 小时' },
  { value: 24 * 3600 * 10_000_000, label: '24 小时' }
];

// 新增触发器表单
const newTriggerType = ref('IntervalTrigger');
const newTriggerInterval = ref(INTERVAL_OPTIONS[3].value);
const newTriggerTime = ref('03:00');
const newTriggerDay = ref('Monday');
const newTriggerMaxRuntime = ref(0);

function openTriggerEditor(task: ScheduledTaskInfo) {
  editingTaskId.value = task.Id;
  editTriggers.value = JSON.parse(JSON.stringify(task.Triggers || []));
  newTriggerType.value = 'IntervalTrigger';
  newTriggerInterval.value = INTERVAL_OPTIONS[3].value;
  newTriggerTime.value = '03:00';
  newTriggerDay.value = 'Monday';
  newTriggerMaxRuntime.value = 0;
}

function closeTriggerEditor() {
  editingTaskId.value = null;
  editTriggers.value = [];
}

function addTrigger() {
  const trigger: ScheduledTaskTrigger = {
    Type: newTriggerType.value,
    MaxRuntimeTicks: newTriggerMaxRuntime.value || undefined
  };
  if (newTriggerType.value === 'IntervalTrigger') {
    trigger.IntervalTicks = newTriggerInterval.value;
  } else if (newTriggerType.value === 'DailyTrigger') {
    trigger.TimeOfDayTicks = timeToTicks(newTriggerTime.value);
  } else if (newTriggerType.value === 'WeeklyTrigger') {
    trigger.TimeOfDayTicks = timeToTicks(newTriggerTime.value);
    trigger.DayOfWeek = newTriggerDay.value;
  }
  editTriggers.value.push(trigger);
}

function removeTrigger(index: number) {
  editTriggers.value.splice(index, 1);
}

async function saveTriggers() {
  if (!editingTaskId.value) return;
  triggerSaving.value = true;
  try {
    await api.updateScheduledTaskTriggers(editingTaskId.value, editTriggers.value);
    toast.success('触发器配置已保存');
    closeTriggerEditor();
    await load();
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '保存失败');
  } finally {
    triggerSaving.value = false;
  }
}

const editingTask = computed(() =>
  editingTaskId.value ? tasks.value.find((t) => t.Id === editingTaskId.value) : null
);

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
onBeforeUnmount(() => stopPolling());
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能查看或触发计划任务。</p>
    </div>

    <div v-else class="space-y-6">
      <!-- 标题 -->
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">Scheduled Tasks</p>
          <h2 class="text-highlighted text-xl font-semibold">计划任务</h2>
        </div>
        <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-cw" :loading="loading" @click="load">
          刷新
        </UButton>
      </div>

      <!-- 统计卡片 -->
      <div class="grid gap-3 sm:grid-cols-3">
        <UCard variant="soft">
          <p class="text-muted text-xs">总任务数</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ tasks.length }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">运行中</p>
          <p class="mt-1 text-2xl font-semibold text-warning">{{ runningCount }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">最近失败</p>
          <p class="mt-1 text-2xl font-semibold" :class="failedCount ? 'text-error' : 'text-highlighted'">{{ failedCount }}</p>
          <p class="text-muted text-xs">{{ idleCount }} 个空闲</p>
        </UCard>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />

      <!-- 按分类分组 -->
      <div v-for="[category, categoryTasks] in categories" :key="category" class="space-y-3">
        <h3 class="text-highlighted flex items-center gap-2 text-base font-semibold">
          <UIcon
            :name="category === 'Library' ? 'i-lucide-library' : category === 'Metadata' ? 'i-lucide-database' : 'i-lucide-wrench'"
            class="size-4 text-muted"
          />
          {{ categoryLabel(category) }}
        </h3>

        <UCard v-for="task in categoryTasks" :key="task.Id">
          <div class="space-y-4">
            <!-- 任务标题行 -->
            <div class="flex flex-wrap items-start justify-between gap-3">
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <h4 class="text-highlighted cursor-pointer text-sm font-semibold hover:underline" @click="openTriggerEditor(task)">{{ task.Name }}</h4>
                  <UBadge variant="soft" :color="stateColor(task.State)" size="xs">
                    {{ task.State === 'Running' ? '运行中' : task.State === 'Idle' ? '空闲' : task.State }}
                  </UBadge>
                </div>
                <p class="text-muted mt-0.5 text-xs">{{ task.Description }}</p>
              </div>
              <div class="flex gap-2">
                <UButton
                  v-if="task.State === 'Running'"
                  color="error"
                  variant="soft"
                  size="xs"
                  icon="i-lucide-square"
                  @click="cancelTask(task)"
                >
                  取消
                </UButton>
                <UButton
                  v-else
                  color="primary"
                  variant="soft"
                  size="xs"
                  icon="i-lucide-play"
                  @click="runTask(task)"
                >
                  运行
                </UButton>
              </div>
            </div>

            <!-- 进度条 -->
            <UProgress
              v-if="task.State === 'Running'"
              :model-value="task.CurrentProgressPercentage || 0"
              :max="100"
              color="warning"
              size="sm"
            />

            <!-- 信息行 -->
            <div class="grid gap-3 sm:grid-cols-3">
              <!-- 触发器 -->
              <div class="rounded-lg border border-default p-3">
                <div class="flex items-center justify-between">
                  <p class="text-muted text-xs">触发方式</p>
                  <UButton
                    color="neutral"
                    variant="ghost"
                    size="xs"
                    icon="i-lucide-settings-2"
                    @click="openTriggerEditor(task)"
                  />
                </div>
                <p class="text-highlighted mt-1 text-xs font-medium">{{ triggersLabel(task) }}</p>
              </div>
              <!-- 最近结果 -->
              <div class="rounded-lg border border-default p-3">
                <p class="text-muted text-xs">最近结果</p>
                <div class="mt-1 flex items-center gap-1.5">
                  <UIcon
                    v-if="task.LastExecutionResult?.Status === 'Completed'"
                    name="i-lucide-check-circle"
                    class="size-3.5 text-success"
                  />
                  <UIcon
                    v-else-if="task.LastExecutionResult?.Status === 'Failed'"
                    name="i-lucide-x-circle"
                    class="size-3.5 text-error"
                  />
                  <UBadge
                    v-if="task.LastExecutionResult?.Status"
                    variant="subtle"
                    :color="statusColor(task.LastExecutionResult.Status)"
                    size="xs"
                  >
                    {{ task.LastExecutionResult.Status }}
                  </UBadge>
                  <span v-else class="text-muted text-xs">尚未执行</span>
                </div>
                <p class="text-muted mt-0.5 text-xs">
                  {{ formatTime(task.LastExecutionResult?.EndTimeUtc || task.LastExecutionResult?.EndTime) }}
                </p>
              </div>
              <!-- 运行时长 -->
              <div class="rounded-lg border border-default p-3">
                <p class="text-muted text-xs">上次耗时</p>
                <p class="text-highlighted mt-1 text-xs font-medium">
                  {{ task.LastExecutionResult?.DurationTicks ? ticksToLabel(task.LastExecutionResult.DurationTicks) : '-' }}
                </p>
              </div>
            </div>

            <!-- 错误信息 -->
            <UAlert
              v-if="task.LastExecutionResult?.ErrorMessage"
              color="error"
              icon="i-lucide-badge-alert"
              :description="task.LastExecutionResult.ErrorMessage"
              size="sm"
            />
          </div>
        </UCard>
      </div>

      <div
        v-if="!tasks.length && !loading"
        class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
      >
        <UIcon name="i-lucide-timer" class="size-10 text-muted" />
        <p class="text-muted text-sm">当前没有计划任务。</p>
      </div>

      <!-- 触发器编辑对话框 -->
      <UModal :open="!!editingTaskId" @update:open="(v: boolean) => { if (!v) closeTriggerEditor() }">
        <template #content>
          <div class="p-5 space-y-5">
            <div>
              <div class="flex items-center gap-2">
                <h3 class="text-highlighted text-lg font-semibold">{{ editingTask?.Name }}</h3>
                <UBadge v-if="editingTask?.State" variant="soft" :color="stateColor(editingTask.State)" size="xs">
                  {{ editingTask.State === 'Running' ? '运行中' : editingTask.State === 'Idle' ? '空闲' : editingTask.State }}
                </UBadge>
              </div>
              <p class="text-muted mt-1 text-sm">{{ editingTask?.Description }}</p>
              <p class="text-muted mt-0.5 text-xs">分类：{{ categoryLabel(editingTask?.Category || '') }}</p>
            </div>

            <div v-if="editingTask?.LastExecutionResult" class="space-y-1 rounded-lg border border-default p-3">
              <p class="text-highlighted text-sm font-medium">上次执行结果</p>
              <div class="grid gap-2 text-xs sm:grid-cols-3">
                <div>
                  <p class="text-muted">状态</p>
                  <UBadge variant="subtle" :color="statusColor(editingTask.LastExecutionResult.Status)" size="xs">
                    {{ editingTask.LastExecutionResult.Status || '-' }}
                  </UBadge>
                </div>
                <div>
                  <p class="text-muted">开始时间</p>
                  <p class="text-highlighted">{{ formatTime(editingTask.LastExecutionResult.StartTimeUtc || editingTask.LastExecutionResult.StartTime) }}</p>
                </div>
                <div>
                  <p class="text-muted">结束时间</p>
                  <p class="text-highlighted">{{ formatTime(editingTask.LastExecutionResult.EndTimeUtc || editingTask.LastExecutionResult.EndTime) }}</p>
                </div>
              </div>
              <UAlert
                v-if="editingTask.LastExecutionResult.ErrorMessage"
                color="error"
                icon="i-lucide-badge-alert"
                :description="editingTask.LastExecutionResult.ErrorMessage"
                size="sm"
                class="mt-2"
              />
            </div>

            <USeparator />

            <div class="space-y-2">
              <p class="text-highlighted text-sm font-medium">当前触发器</p>
              <div v-if="!editTriggers.length" class="text-muted text-xs rounded-lg border border-dashed border-default p-4 text-center">
                无触发器（仅手动运行）
              </div>
              <div
                v-for="(trigger, idx) in editTriggers"
                :key="idx"
                class="flex items-center justify-between rounded-lg border border-default p-3"
              >
                <div>
                  <p class="text-highlighted text-sm font-medium">{{ triggerLabel(trigger) }}</p>
                  <p v-if="trigger.MaxRuntimeTicks" class="text-muted text-xs">
                    最长运行 {{ ticksToLabel(trigger.MaxRuntimeTicks) }}
                  </p>
                </div>
                <UButton
                  color="error"
                  variant="ghost"
                  size="xs"
                  icon="i-lucide-trash-2"
                  @click="removeTrigger(idx)"
                />
              </div>
            </div>

            <USeparator />

            <!-- 添加触发器 -->
            <div class="space-y-3">
              <p class="text-highlighted text-sm font-medium">添加触发器</p>
              <div class="grid gap-3 sm:grid-cols-2">
                <div>
                  <label class="text-muted text-xs block mb-1">触发类型</label>
                  <USelect v-model="newTriggerType" :items="TRIGGER_TYPES" value-key="value" label-key="label" />
                </div>

                <div v-if="newTriggerType === 'IntervalTrigger'">
                  <label class="text-muted text-xs block mb-1">执行间隔</label>
                  <USelect v-model="newTriggerInterval" :items="INTERVAL_OPTIONS" value-key="value" label-key="label" />
                </div>

                <div v-if="newTriggerType === 'DailyTrigger' || newTriggerType === 'WeeklyTrigger'">
                  <label class="text-muted text-xs block mb-1">时间</label>
                  <UInput v-model="newTriggerTime" type="time" />
                </div>

                <div v-if="newTriggerType === 'WeeklyTrigger'">
                  <label class="text-muted text-xs block mb-1">星期</label>
                  <USelect
                    v-model="newTriggerDay"
                    :items="WEEKDAYS.map(d => ({ value: d, label: WEEKDAY_LABELS[d] || d }))"
                    value-key="value"
                    label-key="label"
                  />
                </div>

                <div>
                  <label class="text-muted text-xs block mb-1">最长运行时间</label>
                  <USelect v-model="newTriggerMaxRuntime" :items="MAX_RUNTIME_OPTIONS" value-key="value" label-key="label" />
                </div>
              </div>

              <UButton
                color="neutral"
                variant="soft"
                size="sm"
                icon="i-lucide-plus"
                @click="addTrigger"
              >
                添加
              </UButton>
            </div>

            <USeparator />

            <div class="flex justify-end gap-2">
              <UButton color="neutral" variant="ghost" @click="closeTriggerEditor">
                取消
              </UButton>
              <UButton color="primary" :loading="triggerSaving" @click="saveTriggers">
                保存
              </UButton>
            </div>
          </div>
        </template>
      </UModal>
    </div>
  </SettingsLayout>
</template>
