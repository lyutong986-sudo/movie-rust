<script setup lang="ts">
import { onMounted, ref } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';
import type { ActivityLogEntry, LogFileDto } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(false);
const error = ref('');
const logs = ref<LogFileDto[]>([]);
const activity = ref<ActivityLogEntry[]>([]);

onMounted(async () => {
  if (!isAdmin.value) {
    return;
  }

  loading.value = true;
  try {
    const [logFiles, activityResponse] = await Promise.all([api.serverLogs(), api.activity(50)]);
    logs.value = logFiles;
    activity.value = activityResponse.Items;
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
        <p>日志与活动</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能查看活动流和服务端日志。</p>
      </div>

      <div v-else-if="loading" class="empty">
        <p>日志与活动</p>
        <h2>正在加载</h2>
        <p>正在读取播放活动和日志列表。</p>
      </div>

      <div v-else-if="error" class="empty">
        <p>日志与活动</p>
        <h2>加载失败</h2>
        <p>{{ error }}</p>
      </div>

      <div v-else class="settings-page">
        <div class="split-grid">
          <section class="settings-panel">
            <h3>日志</h3>
            <div v-if="logs.length" class="log-list">
              <article v-for="log in logs" :key="log.Name">
                <strong>{{ log.Name }}</strong>
                <p>{{ formatDate(log.DateModified) }}</p>
              </article>
            </div>
            <p v-else>当前版本暂未输出独立日志文件列表。</p>
          </section>

          <section class="settings-panel">
            <h3>活动</h3>
            <div v-if="activity.length" class="activity-list">
              <article v-for="entry in activity" :key="entry.Id">
                <strong>{{ entry.Name }}</strong>
                <p>{{ entry.ShortOverview || entry.Type }}</p>
                <small>{{ formatDate(entry.Date) }}</small>
              </article>
            </div>
            <p v-else>暂时没有播放活动记录。</p>
          </section>
        </div>
      </div>
    </div>
  </section>
</template>
