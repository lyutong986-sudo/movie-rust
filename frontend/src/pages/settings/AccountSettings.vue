<script setup lang="ts">
import { onMounted, reactive } from 'vue';
import type { UserConfiguration } from '../../api/emby';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
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

const subtitleModeOptions = [
  { label: 'Default', value: 'Default' },
  { label: 'Always', value: 'Always' },
  { label: 'OnlyForced', value: 'OnlyForced' },
  { label: 'None', value: 'None' },
  { label: 'Smart', value: 'Smart' }
];

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
  <SettingsLayout>
    <div class="space-y-6">
      <UCard>
        <div class="flex items-center gap-4">
          <UAvatar
            :alt="user?.Name"
            size="lg"
            :text="user?.Name?.slice(0, 1).toUpperCase() || 'U'"
          />
          <div>
            <h3 class="text-highlighted text-base font-semibold">{{ user?.Name || '未登录' }}</h3>
            <p class="text-muted text-xs">
              {{ user?.Policy?.IsAdministrator ? '管理员账户' : '标准账户' }}
            </p>
            <p class="text-muted font-mono text-[11px]">{{ user?.ServerId || '' }}</p>
          </div>
        </div>
      </UCard>

      <UAlert v-if="state.error" color="error" icon="i-lucide-triangle-alert" :description="state.error" />
      <UAlert v-else-if="state.message" color="success" icon="i-lucide-check" :description="state.message" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">修改密码</h3>
        </template>
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="当前密码">
            <UInput v-model="form.currentPassword" type="password" autocomplete="current-password" class="w-full" />
          </UFormField>
          <div />
          <UFormField label="新密码">
            <UInput v-model="form.newPassword" type="password" autocomplete="new-password" class="w-full" />
          </UFormField>
          <UFormField label="确认新密码">
            <UInput v-model="form.confirmPassword" type="password" autocomplete="new-password" class="w-full" />
          </UFormField>
        </div>
        <template #footer>
          <div class="flex justify-end">
            <UButton :loading="state.busy" @click="changePassword">更新密码</UButton>
          </div>
        </template>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">个人偏好</h3>
        </template>

        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="字幕模式">
            <USelect v-model="preferences.SubtitleMode" :items="subtitleModeOptions" class="w-full" />
          </UFormField>
          <UFormField label="音频语言偏好">
            <UInput v-model="preferences.AudioLanguagePreference" placeholder="zho,chi,eng" class="w-full" />
          </UFormField>
          <UFormField label="字幕语言偏好">
            <UInput v-model="preferences.SubtitleLanguagePreference" placeholder="zho,chi,eng" class="w-full" />
          </UFormField>
        </div>

        <div class="mt-6 grid gap-3 sm:grid-cols-2">
          <USwitch v-model="preferences.PlayDefaultAudioTrack" label="默认选择音轨" />
          <USwitch v-model="preferences.PlayDefaultSubtitleTrack" label="默认选择字幕" />
          <USwitch v-model="preferences.DisplayMissingEpisodes" label="显示缺失剧集" />
          <USwitch v-model="preferences.HidePlayedInLatest" label="最新内容中隐藏已播放" />
          <USwitch v-model="preferences.RememberAudioSelections" label="记住音轨选择" />
          <USwitch v-model="preferences.RememberSubtitleSelections" label="记住字幕选择" />
        </div>

        <template #footer>
          <div class="flex justify-end">
            <UButton :loading="state.busy" @click="savePreferences">保存个人偏好</UButton>
          </div>
        </template>
      </UCard>
    </div>
  </SettingsLayout>
</template>
