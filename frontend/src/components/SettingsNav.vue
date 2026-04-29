<script setup lang="ts">
import { computed } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { isAdmin } from '../store/app';

const route = useRoute();
const router = useRouter();

const NAV = [
  { label: '仪表盘', icon: 'i-lucide-layout-dashboard', to: '/settings/dashboard', admin: true },
  { label: '设置首页', icon: 'i-lucide-layout-grid', to: '/settings', admin: false },
  { label: '账户', icon: 'i-lucide-user', to: '/settings/account', admin: false },
  { label: '主页布局', icon: 'i-lucide-layout-list', to: '/settings/home-layout', admin: false },
  { label: '播放', icon: 'i-lucide-circle-play', to: '/settings/playback', admin: false },
  { label: '字幕', icon: 'i-lucide-subtitles', to: '/settings/subtitles', admin: false },
  { label: '服务器', icon: 'i-lucide-server', to: '/settings/server', admin: true },
  { label: '转码', icon: 'i-lucide-cpu', to: '/settings/transcoding', admin: true },
  { label: '媒体库', icon: 'i-lucide-library', to: '/settings/libraries', admin: true },
  { label: '远端 Emby 中转', icon: 'i-lucide-waypoints', to: '/settings/remote-emby', admin: true },
  { label: '媒体库显示', icon: 'i-lucide-layout-grid', to: '/settings/library-display', admin: true },
  { label: '计划任务', icon: 'i-lucide-timer', to: '/settings/scheduled-tasks', admin: true },
  { label: '字幕下载', icon: 'i-lucide-download-cloud', to: '/settings/subtitle-download', admin: true },
  { label: '品牌化', icon: 'i-lucide-palette', to: '/settings/branding', admin: true },
  { label: '活动报表', icon: 'i-lucide-bar-chart-3', to: '/settings/reports', admin: true },
  { label: '用户', icon: 'i-lucide-users', to: '/settings/users', admin: true },
  { label: '设备', icon: 'i-lucide-monitor', to: '/settings/devices', admin: true },
  { label: 'API Key', icon: 'i-lucide-key-round', to: '/settings/apikeys', admin: true },
  { label: '日志与活动', icon: 'i-lucide-activity', to: '/settings/logs-and-activity', admin: true },
  { label: '网络', icon: 'i-lucide-network', to: '/settings/network', admin: true }
];

const items = computed(() =>
  NAV.filter((entry) => !entry.admin || isAdmin.value).map((entry) => ({
    label: entry.label,
    icon: entry.icon,
    to: entry.to,
    active: route.path === entry.to,
    onSelect: () => router.push(entry.to)
  }))
);
</script>

<template>
  <UNavigationMenu :items="items" orientation="vertical" class="w-full" />
</template>
