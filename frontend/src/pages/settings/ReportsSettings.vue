<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { ActivityLogEntry } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(true);
const error = ref('');
const entries = ref<ActivityLogEntry[]>([]);
const rangeDays = ref(7);

async function load() {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    const result = await api.activity(500);
    entries.value = result.Items;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

const filteredEntries = computed(() => {
  const cutoff = Date.now() - rangeDays.value * 24 * 60 * 60 * 1000;
  return entries.value.filter((entry) => {
    const date = new Date(entry.Date).getTime();
    return !Number.isNaN(date) && date >= cutoff;
  });
});

const totalEvents = computed(() => filteredEntries.value.length);

const eventTypeStats = computed(() => {
  const map = new Map<string, number>();
  for (const entry of filteredEntries.value) {
    const key = entry.Type || 'Unknown';
    map.set(key, (map.get(key) || 0) + 1);
  }
  return Array.from(map.entries())
    .map(([type, count]) => ({ type, count }))
    .sort((a, b) => b.count - a.count);
});

const topUsers = computed(() => {
  const map = new Map<string, number>();
  for (const entry of filteredEntries.value) {
    const name = entry.Name.split(' · ')[0] || entry.UserId || 'Unknown';
    map.set(name, (map.get(name) || 0) + 1);
  }
  return Array.from(map.entries())
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count)
    .slice(0, 10);
});

const dailyCounts = computed(() => {
  const map = new Map<string, number>();
  const cutoff = Date.now() - rangeDays.value * 24 * 60 * 60 * 1000;
  for (let i = 0; i < rangeDays.value; i += 1) {
    const day = new Date(cutoff + i * 24 * 60 * 60 * 1000);
    const key = day.toISOString().slice(0, 10);
    map.set(key, 0);
  }
  for (const entry of filteredEntries.value) {
    const date = new Date(entry.Date);
    if (Number.isNaN(date.getTime())) continue;
    const key = date.toISOString().slice(0, 10);
    map.set(key, (map.get(key) || 0) + 1);
  }
  const entriesArr = Array.from(map.entries()).sort((a, b) => (a[0] < b[0] ? -1 : 1));
  const peak = Math.max(1, ...entriesArr.map(([, count]) => count));
  return entriesArr.map(([date, count]) => ({ date, count, percent: (count / peak) * 100 }));
});

function rangeOptions() {
  return [
    { label: '近 1 天', value: 1 },
    { label: '近 7 天', value: 7 },
    { label: '近 30 天', value: 30 },
    { label: '近 90 天', value: 90 }
  ];
}

onMounted(load);
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能查看活动报表。</p>
    </div>

    <div v-else class="space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">Reports</p>
          <h2 class="text-highlighted text-xl font-semibold">活动报表</h2>
        </div>
        <div class="flex items-center gap-2">
          <USelect v-model.number="rangeDays" :items="rangeOptions()" value-key="value" class="w-40" />
          <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-cw" :loading="loading" @click="load">
            刷新
          </UButton>
        </div>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />

      <div class="grid gap-3 sm:grid-cols-3">
        <UCard variant="soft">
          <p class="text-muted text-xs">时间范围内事件数</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ totalEvents }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">不同用户</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ topUsers.length }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">事件类型</p>
          <p class="text-highlighted mt-1 text-2xl font-semibold">{{ eventTypeStats.length }}</p>
        </UCard>
      </div>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">按日分布</h3>
        </template>
        <div class="grid grid-cols-2 gap-2 sm:grid-cols-4 lg:grid-cols-7">
          <div v-for="day in dailyCounts" :key="day.date" class="rounded-lg border border-default bg-elevated/20 p-2">
            <p class="text-muted text-[10px]">{{ day.date }}</p>
            <p class="text-highlighted text-sm font-semibold">{{ day.count }}</p>
            <div class="mt-1 h-1 rounded bg-elevated">
              <div class="h-1 rounded bg-primary" :style="{ width: `${day.percent}%` }" />
            </div>
          </div>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">事件类型</h3>
        </template>
        <div v-if="eventTypeStats.length" class="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          <div
            v-for="stat in eventTypeStats"
            :key="stat.type"
            class="flex items-center justify-between rounded-lg border border-default bg-elevated/20 p-3"
          >
            <span class="text-highlighted text-sm font-medium">{{ stat.type }}</span>
            <UBadge variant="subtle" color="primary">{{ stat.count }}</UBadge>
          </div>
        </div>
        <p v-else class="text-muted text-sm">暂无数据</p>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">用户 Top 10</h3>
        </template>
        <div v-if="topUsers.length" class="space-y-2">
          <div
            v-for="user in topUsers"
            :key="user.name"
            class="flex items-center justify-between rounded-lg border border-default bg-elevated/20 px-3 py-2"
          >
            <span class="text-highlighted text-sm">{{ user.name }}</span>
            <UBadge variant="subtle" color="primary">{{ user.count }} 次</UBadge>
          </div>
        </div>
        <p v-else class="text-muted text-sm">暂无数据</p>
      </UCard>
    </div>
  </SettingsLayout>
</template>
