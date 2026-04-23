<script setup lang="ts">
import { useRoute, useRouter } from 'vue-router';
import { isAdmin } from '../store/app';

const route = useRoute();
const router = useRouter();

const items = [
  { label: '设置首页', to: '/settings', admin: false },
  { label: '账户', to: '/settings/account', admin: false },
  { label: '播放', to: '/settings/playback', admin: false },
  { label: '字幕', to: '/settings/subtitles', admin: false },
  { label: '服务器', to: '/settings/server', admin: true },
  { label: '转码', to: '/settings/transcoding', admin: true },
  { label: '媒体库', to: '/settings/libraries', admin: true },
  { label: '用户', to: '/settings/users', admin: true },
  { label: '设备', to: '/settings/devices', admin: true },
  { label: 'API Key', to: '/settings/apikeys', admin: true },
  { label: '日志活动', to: '/settings/logs-and-activity', admin: true },
  { label: '网络', to: '/settings/network', admin: true }
];

function isActive(path: string) {
  return route.path === path;
}
</script>

<template>
  <aside class="settings-nav">
    <button
      v-for="item in items.filter((entry) => !entry.admin || isAdmin)"
      :key="item.to"
      type="button"
      :class="{ active: isActive(item.to) }"
      @click="router.push(item.to)"
    >
      {{ item.label }}
    </button>
  </aside>
</template>
