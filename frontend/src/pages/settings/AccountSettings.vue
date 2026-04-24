<script setup lang="ts">
import { onMounted, reactive } from 'vue';
import type { UserConfiguration } from '../../api/emby';
import SettingsNav from '../../components/SettingsNav.vue';
import { api, state, user } from '../../store/app';

const form = reactive({
  currentPassword: '',
  newPassword: '',
  confirmPassword: ''
});

const preferences = reactive<UserConfiguration>({
  PlayDefaultAudioTrack: true,
  PlayDefaultSubtitleTrack: false,
  SubtitleMode: 'Default',
  AudioLanguagePreference: '',
  SubtitleLanguagePreference: '',
  DisplayMissingEpisodes: false,
  GroupedFolders: [],
  LatestItemsExcludes: [],
  MyMediaExcludes: [],
  OrderedViews: [],
  HidePlayedInLatest: false,
  RememberAudioSelections: true,
  RememberSubtitleSelections: true
});

onMounted(async () => {
  if (!user.value) {
    return;
  }

  try {
    const me = await api.me();
    user.value = me;
    const settings = await api.userSettings(me.Id);
    Object.assign(preferences, settings.Configuration || {});
  } catch {
    // no-op
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

async function savePreferences() {
  state.error = '';
  state.message = '';

  if (!user.value) {
    state.error = '当前没有登录用户';
    return;
  }

  state.busy = true;
  try {
    await api.updateUserSettings(user.value.Id, {
      ...preferences,
      GroupedFolders: [...(preferences.GroupedFolders || [])],
      LatestItemsExcludes: [...(preferences.LatestItemsExcludes || [])],
      MyMediaExcludes: [...(preferences.MyMediaExcludes || [])],
      OrderedViews: [...(preferences.OrderedViews || [])]
    });
    state.message = '个人偏好已更新';
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

        <h3>修改密码</h3>
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

        <h3>个人偏好</h3>
        <label>
          字幕模式
          <select v-model="preferences.SubtitleMode">
            <option value="Default">Default</option>
            <option value="Always">Always</option>
            <option value="OnlyForced">OnlyForced</option>
            <option value="None">None</option>
            <option value="Smart">Smart</option>
          </select>
        </label>
        <label>
          音频语言偏好
          <input v-model="preferences.AudioLanguagePreference" type="text" />
        </label>
        <label>
          字幕语言偏好
          <input v-model="preferences.SubtitleLanguagePreference" type="text" />
        </label>
        <div class="user-options">
          <label><input v-model="preferences.PlayDefaultAudioTrack" type="checkbox" /> 默认选择音轨</label>
          <label><input v-model="preferences.PlayDefaultSubtitleTrack" type="checkbox" /> 默认选择字幕</label>
          <label><input v-model="preferences.DisplayMissingEpisodes" type="checkbox" /> 显示缺失剧集</label>
          <label><input v-model="preferences.HidePlayedInLatest" type="checkbox" /> 最新内容中隐藏已播放</label>
          <label><input v-model="preferences.RememberAudioSelections" type="checkbox" /> 记住音轨选择</label>
          <label><input v-model="preferences.RememberSubtitleSelections" type="checkbox" /> 记住字幕选择</label>
        </div>
        <div class="button-row">
          <button :disabled="state.busy" type="button" @click="savePreferences">保存个人偏好</button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.user-options {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px;
}
</style>
