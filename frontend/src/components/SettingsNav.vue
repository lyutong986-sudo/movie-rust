<script setup lang="ts">
import { useRoute, useRouter } from 'vue-router';

const route = useRoute();
const router = useRouter();

const items = [
  { label: '控制台', to: '/settings' },
  { label: '服务器', to: '/settings/server' },
  { label: '媒体库', to: '/settings/libraries' },
  { label: '用户', to: '/settings/users' },
  { label: '播放', to: '/settings/playback' },
  { label: '网络', to: '/settings/network' }
];

function isActive(path: string) {
  return route.path === path;
}
</script>

<template>
  <aside class="settings-nav">
    <button
      v-for="item in items"
      :key="item.to"
      type="button"
      :class="{ active: isActive(item.to) }"
      @click="router.push(item.to)"
    >
      {{ item.label }}
    </button>
  </aside>
</template>
