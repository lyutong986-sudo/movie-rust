<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import SettingsNav from '../../components/SettingsNav.vue';
import { adminUsers, currentServer, isAdmin, libraries, loadAdminData, state, systemInfo, totalLibraryItems, user } from '../../store/app';

const router = useRouter();

const userEntries = computed(() => [
  {
    icon: '账',
    title: '账户',
    description: '查看当前登录用户并修改密码',
    to: '/settings/account'
  },
  {
    icon: '播',
    title: '播放',
    description: '本地播放器、直链和会话兼容状态',
    to: '/settings/playback'
  },
  {
    icon: '字',
    title: '字幕',
    description: '客户端字幕样式预设',
    to: '/settings/subtitles'
  }
]);

const adminEntries = computed(() => [
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
    icon: '设',
    title: '设备',
    description: '查看已建立的会话设备',
    to: '/settings/devices'
  },
  {
    icon: '钥',
    title: 'API Key',
    description: '当前版本的令牌和接口兼容说明',
    to: '/settings/apikeys'
  },
  {
    icon: '志',
    title: '日志活动',
    description: '近期播放活动和服务状态',
    to: '/settings/logs-and-activity'
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
      <div class="settings-page">
        <div class="stat-grid">
          <article>
            <small>当前用户</small>
            <strong>{{ user?.Name || '未登录' }}</strong>
            <span>{{ isAdmin ? '管理员账户' : '标准账户' }}</span>
          </article>
          <article>
            <small>服务器</small>
            <strong>{{ state.serverName }}</strong>
            <span>{{ currentServer?.Url || systemInfo?.LocalAddress || '-' }}</span>
          </article>
          <article>
            <small>版本</small>
            <strong>{{ systemInfo?.Version || '0.1.0' }}</strong>
            <span>{{ libraries.length }} 个媒体库 / {{ totalLibraryItems }} 个条目</span>
          </article>
        </div>

        <div class="settings-list">
          <button v-for="entry in userEntries" :key="entry.to" type="button" @click="router.push(entry.to)">
            <span>{{ entry.icon }}</span>
            <div>
              <h3>{{ entry.title }}</h3>
              <p>{{ entry.description }}</p>
            </div>
          </button>
        </div>

        <div v-if="isAdmin" class="settings-list">
          <button v-for="entry in adminEntries" :key="entry.to" type="button" @click="router.push(entry.to)">
            <span>{{ entry.icon }}</span>
            <div>
              <h3>{{ entry.title }}</h3>
              <p>{{ entry.description }}</p>
            </div>
          </button>
        </div>

        <div v-if="isAdmin" class="placeholder-grid">
          <article>
            <h3>管理员概览</h3>
            <p>共有 {{ adminUsers.length }} 个用户，当前服务器版本 {{ systemInfo?.Version || '0.1.0' }}。</p>
          </article>
        </div>

      </div>
    </div>
  </section>
</template>
