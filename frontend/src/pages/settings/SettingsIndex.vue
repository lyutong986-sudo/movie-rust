<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import {
  adminUsers,
  cancelCurrentScan,
  currentServer,
  homeItems,
  isAdmin,
  libraries,
  loadAdminData,
  playQueue,
  scanOperation,
  state,
  systemInfo,
  totalLibraryItems,
  user,
  watchLater
} from '../../store/app';

const router = useRouter();
const query = ref('');

type SettingsEntry = {
  icon: string;
  title: string;
  description: string;
  to: string;
};

const userEntries = computed<SettingsEntry[]>(() => [
  {
    icon: 'i-lucide-user',
    title: '账户',
    description: '查看当前登录用户，修改密码和个人偏好。',
    to: '/settings/account'
  },
  {
    icon: 'i-lucide-circle-play',
    title: '播放',
    description: '本地播放器、直链播放和会话兼容状态。',
    to: '/settings/playback'
  },
  {
    icon: 'i-lucide-subtitles',
    title: '字幕',
    description: '客户端字幕样式与默认字幕偏好。',
    to: '/settings/subtitles'
  }
]);

const adminEntries = computed<SettingsEntry[]>(() => [
  {
    icon: 'i-lucide-server',
    title: '服务器',
    description: '名称、语言、扫描线程和元数据配置。',
    to: '/settings/server'
  },
  {
    icon: 'i-lucide-cpu',
    title: '转码',
    description: 'FFmpeg、硬件加速、并发限制和 H.264 质量。',
    to: '/settings/transcoding'
  },
  {
    icon: 'i-lucide-library',
    title: '媒体库',
    description: '创建媒体库、管理路径并执行扫描。',
    to: '/settings/libraries'
  },
  {
    icon: 'i-lucide-waypoints',
    title: '远端 Emby 中转',
    description: '接入外部 Emby 账号，将远端媒体映射入库并进行中转播放。',
    to: '/settings/remote-emby'
  },
  {
    icon: 'i-lucide-layout-grid',
    title: '媒体库显示',
    description: '合集视图、Specials、文件夹视图、DateAdded 策略。',
    to: '/settings/library-display'
  },
  {
    icon: 'i-lucide-users',
    title: '用户',
    description: '管理账号、密码、权限、媒体库访问和偏好。',
    to: '/settings/users'
  },
  {
    icon: 'i-lucide-monitor',
    title: '设备',
    description: '查看已经建立的客户端会话设备。',
    to: '/settings/devices'
  },
  {
    icon: 'i-lucide-key-round',
    title: 'API Key',
    description: '管理长期 API Key 和外部集成令牌。',
    to: '/settings/apikeys'
  },
  {
    icon: 'i-lucide-timer',
    title: '计划任务',
    description: '媒体库扫描、元数据刷新、缓存清理等后台任务。',
    to: '/settings/scheduled-tasks'
  },
  {
    icon: 'i-lucide-subtitles',
    title: '字幕下载',
    description: 'OpenSubtitles 账号、下载语言、匹配策略。',
    to: '/settings/subtitle-download'
  },
  {
    icon: 'i-lucide-palette',
    title: '品牌化',
    description: '登录声明、自定义 CSS 与闪屏开关。',
    to: '/settings/branding'
  },
  {
    icon: 'i-lucide-bar-chart-3',
    title: '活动报表',
    description: '按时间范围聚合用户行为与事件类型。',
    to: '/settings/reports'
  },
  {
    icon: 'i-lucide-activity',
    title: '日志与活动',
    description: '查看近期播放活动和服务状态。',
    to: '/settings/logs-and-activity'
  },
  {
    icon: 'i-lucide-network',
    title: '网络',
    description: '远程访问、端口、HTTPS、DDNS 与接入地址。',
    to: '/settings/network'
  }
]);

onMounted(async () => {
  if (isAdmin.value) {
    await loadAdminData();
  }
});

function matches(entry: SettingsEntry) {
  const q = query.value.trim().toLowerCase();
  if (!q) return true;
  return entry.title.toLowerCase().includes(q) || entry.description.toLowerCase().includes(q);
}

const filteredUserEntries = computed(() => userEntries.value.filter(matches));
const filteredAdminEntries = computed(() => adminEntries.value.filter(matches));

const collectionStats = computed(() => {
  const map = new Map<string, number>();
  for (const lib of libraries.value) {
    const key = lib.CollectionType || 'other';
    map.set(key, (map.get(key) || 0) + (lib.ChildCount || 0));
  }
  const total = Array.from(map.values()).reduce((sum, v) => sum + v, 0) || 1;
  const labelMap: Record<string, string> = {
    movies: '电影',
    tvshows: '剧集',
    other: '其他'
  };
  const colorMap: Record<string, string> = {
    movies: 'bg-sky-500',
    tvshows: 'bg-indigo-500',
    other: 'bg-neutral-500'
  };
  return Array.from(map.entries()).map(([type, count]) => ({
    type,
    label: labelMap[type] || type,
    color: colorMap[type] || 'bg-neutral-500',
    count,
    percent: Math.round((count / total) * 100)
  }));
});

const scanStatusColor = computed(() => {
  const status = scanOperation.value?.Status;
  if (!status) return 'neutral';
  if (status === 'Succeeded') return 'success';
  if (status === 'Failed') return 'error';
  if (status === 'Cancelled') return 'neutral';
  return 'warning';
});

const canCancelScan = computed(() => {
  const status = scanOperation.value?.Status;
  return status === 'Queued' || status === 'Running' || status === 'Cancelling';
});
</script>

<template>
  <SettingsLayout>
    <div class="space-y-6">
      <UInput
        v-model="query"
        icon="i-lucide-search"
        placeholder="在设置中搜索：账户、字幕、转码、日志..."
        class="w-full"
      />

      <div class="grid gap-3 sm:grid-cols-3">
        <UCard variant="soft">
          <p class="text-muted text-xs">当前用户</p>
          <p class="text-highlighted mt-1 text-lg font-semibold">
            {{ user?.Name || '未登录' }}
          </p>
          <p class="text-muted text-xs">{{ isAdmin ? '管理员账户' : '标准账户' }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">服务器</p>
          <p class="text-highlighted mt-1 text-lg font-semibold">{{ state.serverName }}</p>
          <p class="text-muted truncate font-mono text-xs">
            {{ currentServer?.Url || systemInfo?.LocalAddress || '-' }}
          </p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">版本</p>
          <p class="text-highlighted mt-1 text-lg font-semibold">
            {{ systemInfo?.Version || '0.1.0' }}
          </p>
          <p class="text-muted text-xs">
            {{ libraries.length }} 个媒体库 / {{ totalLibraryItems }} 个条目
          </p>
        </UCard>
      </div>

      <div v-if="filteredUserEntries.length">
        <h3 class="text-highlighted mb-2 text-sm font-semibold">用户设置</h3>
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <button
            v-for="entry in filteredUserEntries"
            :key="entry.to"
            type="button"
            class="group flex items-start gap-3 rounded-xl border border-default bg-elevated/30 p-4 text-start transition hover:bg-elevated/70 hover:ring-1 hover:ring-primary/40"
            @click="router.push(entry.to)"
          >
            <div
              class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary transition group-hover:bg-primary group-hover:text-primary-contrast"
            >
              <UIcon :name="entry.icon" class="size-4" />
            </div>
            <div class="min-w-0 flex-1">
              <h4 class="text-highlighted text-sm font-medium">{{ entry.title }}</h4>
              <p class="text-muted mt-0.5 text-xs">{{ entry.description }}</p>
            </div>
            <UIcon name="i-lucide-chevron-right" class="size-4 shrink-0 self-center text-dimmed" />
          </button>
        </div>
      </div>

      <div v-if="isAdmin && filteredAdminEntries.length">
        <h3 class="text-highlighted mb-2 text-sm font-semibold">管理员设置</h3>
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <button
            v-for="entry in filteredAdminEntries"
            :key="entry.to"
            type="button"
            class="group flex items-start gap-3 rounded-xl border border-default bg-elevated/30 p-4 text-start transition hover:bg-elevated/70 hover:ring-1 hover:ring-primary/40"
            @click="router.push(entry.to)"
          >
            <div
              class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary transition group-hover:bg-primary group-hover:text-primary-contrast"
            >
              <UIcon :name="entry.icon" class="size-4" />
            </div>
            <div class="min-w-0 flex-1">
              <h4 class="text-highlighted text-sm font-medium">{{ entry.title }}</h4>
              <p class="text-muted mt-0.5 text-xs">{{ entry.description }}</p>
            </div>
            <UIcon name="i-lucide-chevron-right" class="size-4 shrink-0 self-center text-dimmed" />
          </button>
        </div>
      </div>

      <UCard v-if="isAdmin" variant="soft">
        <h3 class="text-highlighted mb-4 text-sm font-semibold">管理员概览</h3>
        <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <div class="rounded-lg bg-elevated/50 p-3">
            <p class="text-muted text-xs">媒体库</p>
            <p class="text-highlighted mt-1 text-2xl font-semibold">{{ libraries.length }}</p>
          </div>
          <div class="rounded-lg bg-elevated/50 p-3">
            <p class="text-muted text-xs">条目总数</p>
            <p class="text-highlighted mt-1 text-2xl font-semibold">{{ totalLibraryItems }}</p>
          </div>
          <div class="rounded-lg bg-elevated/50 p-3">
            <p class="text-muted text-xs">用户数</p>
            <p class="text-highlighted mt-1 text-2xl font-semibold">{{ adminUsers.length }}</p>
          </div>
          <div class="rounded-lg bg-elevated/50 p-3">
            <p class="text-muted text-xs">首页呈现</p>
            <p class="text-highlighted mt-1 text-2xl font-semibold">{{ homeItems.length }}</p>
          </div>
        </div>

        <div v-if="collectionStats.length" class="mt-4 space-y-2">
          <p class="text-muted text-xs">内容分布</p>
          <div class="flex h-3 overflow-hidden rounded-full bg-elevated">
            <div
              v-for="stat in collectionStats"
              :key="stat.type"
              :class="stat.color"
              :style="{ width: `${stat.percent}%` }"
              :title="`${stat.label}: ${stat.count} 项`"
            />
          </div>
          <div class="flex flex-wrap gap-3 text-xs">
            <span
              v-for="stat in collectionStats"
              :key="stat.type"
              class="text-muted flex items-center gap-1.5"
            >
              <span class="inline-block size-2 rounded-full" :class="stat.color" />
              {{ stat.label }} / {{ stat.count }}
            </span>
          </div>
        </div>

        <div class="text-muted mt-4 flex flex-wrap gap-3 text-xs">
          <span>队列 {{ playQueue.length }}</span>
          <span>稍后观看 {{ watchLater.length }}</span>
          <span>版本 {{ systemInfo?.Version || '0.1.0' }}</span>
        </div>
      </UCard>

      <UCard v-if="isAdmin" variant="soft">
        <div class="flex items-center justify-between gap-3">
          <div>
            <h3 class="text-highlighted text-sm font-semibold">扫描任务面板</h3>
            <p class="text-muted mt-1 text-xs">
              状态 {{ scanOperation?.Status || 'Idle' }} / 进度 {{ Math.round(scanOperation?.Progress || 0) }}%
            </p>
          </div>
          <div class="flex items-center gap-2">
            <UBadge :color="scanStatusColor" variant="soft" size="sm">
              {{ scanOperation?.Status || 'Idle' }}
            </UBadge>
            <UButton
              color="error"
              variant="soft"
              size="sm"
              icon="i-lucide-square"
              :disabled="!canCancelScan || state.busy"
              @click="cancelCurrentScan"
            >
              取消
            </UButton>
            <UButton color="neutral" variant="subtle" size="sm" @click="router.push('/settings/libraries')">
              进入媒体库任务详情
            </UButton>
          </div>
        </div>
      </UCard>
    </div>
  </SettingsLayout>
</template>
