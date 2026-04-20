<script setup lang="ts">
import { onMounted, ref } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';
import type { SessionInfo } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(false);
const error = ref('');
const devices = ref<SessionInfo[]>([]);

onMounted(async () => {
  if (!isAdmin.value) {
    return;
  }

  loading.value = true;
  try {
    devices.value = await api.sessions();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : String(loadError);
  } finally {
    loading.value = false;
  }
});

function formatDate(value: string) {
  return new Date(value).toLocaleString('zh-CN');
}
</script>

<template>
  <section class="settings-shell">
    <SettingsNav />

    <div class="settings-content">
      <div v-if="!isAdmin" class="empty">
        <p>设备</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能查看已连接会话列表。</p>
      </div>

      <div v-else-if="loading" class="empty">
        <p>设备</p>
        <h2>正在加载</h2>
        <p>正在读取当前服务器上的会话设备。</p>
      </div>

      <div v-else-if="error" class="empty">
        <p>设备</p>
        <h2>加载失败</h2>
        <p>{{ error }}</p>
      </div>

      <div v-else class="settings-page">
        <div class="admin-table table-card">
          <div class="admin-row head">
            <span>用户</span>
            <span>客户端</span>
            <span>最后活动</span>
          </div>
          <div v-for="device in devices" :key="device.Id" class="admin-row">
            <span>{{ device.UserName }}</span>
            <span>{{ device.DeviceName }} / {{ device.Client }}</span>
            <span>{{ formatDate(device.LastActivityDate) }}</span>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
