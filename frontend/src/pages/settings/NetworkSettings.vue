<script setup lang="ts">
import { computed, onMounted } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';
import { api, isAdmin, loadAdminData, saveServerSettings, state, systemInfo } from '../../store/app';

const baseUrl = computed(() => api.baseUrl || window.location.origin);

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
        <p>网络</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能修改远程访问设置。</p>
      </div>

      <form v-else class="settings-page settings-form" @submit.prevent="saveServerSettings">
        <label class="check-row">
          <input v-model="state.allowRemoteAccess" type="checkbox" />
          允许远程访问
        </label>
        <label class="check-row">
          <input v-model="state.enableUPNP" type="checkbox" />
          自动端口映射
        </label>

        <div class="stat-grid">
          <article>
            <small>主入口</small>
            <strong>{{ systemInfo?.LocalAddress || baseUrl }}</strong>
            <span>标准根路径</span>
          </article>
          <article>
            <small>Emby 兼容入口</small>
            <strong>{{ baseUrl }}/emby</strong>
            <span>适配现有 Emby 客户端和本地播放器</span>
          </article>
          <article>
            <small>MediaBrowser 兼容入口</small>
            <strong>{{ baseUrl }}/mediabrowser</strong>
            <span>兼容老式客户端路径</span>
          </article>
        </div>

        <div class="button-row">
          <button :disabled="state.busy" type="submit">保存网络设置</button>
        </div>
      </form>
    </div>
  </section>
</template>
