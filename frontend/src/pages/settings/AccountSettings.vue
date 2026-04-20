<script setup lang="ts">
import { onMounted, reactive } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';
import { api, state, user } from '../../store/app';

const form = reactive({
  currentPassword: '',
  newPassword: '',
  confirmPassword: ''
});

onMounted(async () => {
  if (user.value) {
    try {
      const me = await api.me();
      user.value = me;
    } catch {
      // no-op
    }
  }
});

async function changePassword() {
  state.error = '';
  state.message = '';

  if (!user.value) {
    state.error = '当前没有登录用户';
    return;
  }

  if (form.newPassword.length < 4) {
    state.error = '新密码至少需要 4 个字符';
    return;
  }

  if (form.newPassword !== form.confirmPassword) {
    state.error = '两次输入的新密码不一致';
    return;
  }

  state.busy = true;
  try {
    await api.changePassword(user.value.Id, {
      CurrentPw: form.currentPassword,
      NewPw: form.newPassword
    });
    form.currentPassword = '';
    form.newPassword = '';
    form.confirmPassword = '';
    state.message = '密码已更新';
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.busy = false;
  }
}
</script>

<template>
  <section class="settings-shell">
    <SettingsNav />

    <div class="settings-content">
      <div class="settings-page settings-form">
        <div class="user-admin-grid">
          <article>
            <span>{{ user?.Name?.slice(0, 1).toUpperCase() || 'U' }}</span>
            <div>
              <strong>{{ user?.Name || '未登录' }}</strong>
              <p>{{ user?.Policy?.IsAdministrator ? '管理员账户' : '标准账户' }}</p>
              <p>{{ user?.ServerId || '' }}</p>
            </div>
          </article>
        </div>

        <label>
          当前密码
          <input v-model="form.currentPassword" type="password" autocomplete="current-password" />
        </label>
        <label>
          新密码
          <input v-model="form.newPassword" type="password" autocomplete="new-password" />
        </label>
        <label>
          确认新密码
          <input v-model="form.confirmPassword" type="password" autocomplete="new-password" />
        </label>
        <div class="button-row">
          <button :disabled="state.busy" type="button" @click="changePassword">更新密码</button>
        </div>
      </div>
    </div>
  </section>
</template>
