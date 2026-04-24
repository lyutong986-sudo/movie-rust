<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import {
  adminUsers,
  currentServer,
  isAdmin,
  libraries,
  loadAdminData,
  state,
  systemInfo,
  totalLibraryItems,
  user
} from '../../store/app';

const router = useRouter();

const userEntries = computed(() => [
  {
    icon: 'i-lucide-user',
    title: '账户',
    description: '查看当前登录用户并修改密码',
    to: '/settings/account'
  },
  {
    icon: 'i-lucide-circle-play',
    title: '播放',
    description: '本地播放器、直链和会话兼容状态',
    to: '/settings/playback'
  },
  {
    icon: 'i-lucide-subtitles',
    title: '字幕',
    description: '客户端字幕样式预设',
    to: '/settings/subtitles'
  }
]);

const adminEntries = computed(() => [
  {
    icon: 'i-lucide-server',
    title: '服务器',
    description: '名称、语言、元数据和引导配置',
    to: '/settings/server'
  },
  {
    icon: 'i-lucide-cpu',
    title: '转码',
    description: 'ffmpeg、硬件加速、线程和 H264 质量',
    to: '/settings/transcoding'
  },
  {
    icon: 'i-lucide-library',
    title: '媒体库',
    description: '创建媒体库、查看路径并执行扫描',
    to: '/settings/libraries'
  },
  {
    icon: 'i-lucide-users',
    title: '用户',
    description: '管理员与普通用户列表',
    to: '/settings/users'
  },
  {
    icon: 'i-lucide-monitor',
    title: '设备',
    description: '查看已建立的会话设备',
    to: '/settings/devices'
  },
  {
    icon: 'i-lucide-key-round',
    title: 'API Key',
    description: '当前版本的令牌和接口兼容说明',
    to: '/settings/apikeys'
  },
  {
    icon: 'i-lucide-activity',
    title: '日志活动',
    description: '近期播放活动和服务状态',
    to: '/settings/logs-and-activity'
  },
  {
    icon: 'i-lucide-network',
    title: '网络',
    description: '远程访问、端口和 Emby 兼容入口',
    to: '/settings/network'
  }
]);

onMounted(async () => {
  if (isAdmin.value) {
    await loadAdminData();
  }
});
</script>

<template>
  <SettingsLayout>
    <div class="space-y-6">
      <div class="grid gap-3 sm:grid-cols-3">
        <UCard variant="soft">
          <p class="text-muted text-xs">当前用户</p>
          <p class="text-highlighted mt-1 text-lg font-semibold">{{ user?.Name || '未登录' }}</p>
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

      <div>
        <h3 class="text-highlighted mb-2 text-sm font-semibold">用户设置</h3>
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <button
            v-for="entry in userEntries"
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

      <div v-if="isAdmin">
        <h3 class="text-highlighted mb-2 text-sm font-semibold">管理员设置</h3>
        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <button
            v-for="entry in adminEntries"
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
        <h3 class="text-highlighted text-sm font-semibold">管理员概览</h3>
        <p class="text-muted mt-1 text-sm">
          共有 {{ adminUsers.length }} 个用户，当前服务器版本
          {{ systemInfo?.Version || '0.1.0' }}。
        </p>
      </UCard>
    </div>
  </SettingsLayout>
</template>
