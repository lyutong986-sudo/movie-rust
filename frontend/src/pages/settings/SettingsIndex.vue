<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import SettingsNav from '../../components/SettingsNav.vue';
import { adminUsers, isAdmin, libraries, loadAdminData, state, systemInfo, totalLibraryItems } from '../../store/app';

const router = useRouter();

const entries = computed(() => [
  {
    icon: '服',
    title: '服务器',
    description: '名称、语言、元数据和引导配置',
    to: '/settings/server'
  },
  {
    icon: '库',
    title: '媒体库',
    description: '创建媒体库、查看路径并执行扫描',
    to: '/settings/libraries'
  },
  {
    icon: '人',
    title: '用户',
    description: '管理员与普通用户列表',
    to: '/settings/users'
  },
  {
    icon: '播',
    title: '播放',
    description: '本地播放器、直链和字幕兼容状态',
    to: '/settings/playback'
  },
  {
    icon: '网',
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
  <section class="settings-shell">
    <SettingsNav />

    <div class="settings-content">
      <div v-if="!isAdmin" class="empty">
        <p>控制台</p>
        <h2>需要管理员权限</h2>
        <p>只有管理员用户才能进入 Jellyfin 风格的后台控制台。</p>
      </div>

      <div v-else class="settings-page">
        <div class="stat-grid">
          <article>
            <small>服务器名称</small>
            <strong>{{ state.serverName }}</strong>
            <span>{{ systemInfo?.Version || '0.1.0' }}</span>
          </article>
          <article>
            <small>媒体库</small>
            <strong>{{ libraries.length }}</strong>
            <span>{{ totalLibraryItems }} 个条目</span>
          </article>
          <article>
            <small>用户</small>
            <strong>{{ adminUsers.length }}</strong>
            <span>管理员和普通用户</span>
          </article>
        </div>

        <div class="settings-list">
          <button v-for="entry in entries" :key="entry.to" type="button" @click="router.push(entry.to)">
            <span>{{ entry.icon }}</span>
            <div>
              <h3>{{ entry.title }}</h3>
              <p>{{ entry.description }}</p>
            </div>
          </button>
        </div>
      </div>
    </div>
  </section>
</template>
