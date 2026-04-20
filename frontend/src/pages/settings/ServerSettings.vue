<script setup lang="ts">
import { onMounted } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';
import { isAdmin, loadAdminData, saveServerSettings, state, systemInfo } from '../../store/app';

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
        <p>服务器</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能修改服务器配置。</p>
      </div>

      <form v-else class="settings-page settings-form" @submit.prevent="saveServerSettings">
        <div class="stat-grid">
          <article>
            <small>版本</small>
            <strong>{{ systemInfo?.Version || '0.1.0' }}</strong>
            <span>{{ systemInfo?.ProductName || 'Movie Rust' }}</span>
          </article>
          <article>
            <small>系统</small>
            <strong>{{ systemInfo?.OperatingSystem || 'Unknown' }}</strong>
            <span>与 Jellyfin/Emby 兼容接口共存</span>
          </article>
          <article>
            <small>引导状态</small>
            <strong>{{ state.startupWizardCompleted ? '已完成' : '未完成' }}</strong>
            <span>管理员账户通过首次启动向导创建</span>
          </article>
        </div>

        <label>
          服务器名称
          <input v-model="state.serverName" />
        </label>
        <label>
          界面语言
          <select v-model="state.uiCulture">
            <option value="zh-CN">简体中文</option>
            <option value="en-US">English</option>
          </select>
        </label>
        <label>
          元数据语言
          <select v-model="state.metadataLanguage">
            <option value="zh">中文</option>
            <option value="en">English</option>
            <option value="ja">日本語</option>
            <option value="ko">한국어</option>
          </select>
        </label>
        <label>
          元数据国家/地区
          <select v-model="state.metadataCountry">
            <option value="CN">中国</option>
            <option value="US">United States</option>
            <option value="JP">日本</option>
            <option value="KR">韩国</option>
          </select>
        </label>
        <div class="button-row">
          <button :disabled="state.busy" type="submit">保存服务器设置</button>
        </div>
      </form>
    </div>
  </section>
</template>
