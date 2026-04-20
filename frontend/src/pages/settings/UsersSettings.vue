<script setup lang="ts">
import { onMounted } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';
import { adminUsers, isAdmin, loadAdminData } from '../../store/app';

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
        <p>用户</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能查看全部用户列表。</p>
      </div>

      <div v-else class="settings-page">
        <div class="admin-table">
          <div class="admin-row head">
            <span>用户名</span>
            <span>服务器</span>
            <span>角色</span>
          </div>
          <div v-for="account in adminUsers" :key="account.Id" class="admin-row">
            <span>{{ account.Name }}</span>
            <span>{{ account.ServerId }}</span>
            <span>{{ account.Policy?.IsAdministrator ? '管理员' : '用户' }}</span>
          </div>
        </div>

        <div class="user-admin-grid">
          <article v-for="account in adminUsers" :key="`${account.Id}-card`">
            <span>{{ account.Name.slice(0, 1).toUpperCase() }}</span>
            <div>
              <strong>{{ account.Name }}</strong>
              <p>{{ account.Policy?.IsAdministrator ? '拥有控制台权限' : '标准播放账户' }}</p>
            </div>
          </article>
        </div>
      </div>
    </div>
  </section>
</template>
