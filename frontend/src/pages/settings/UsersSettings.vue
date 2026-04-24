<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';
import { api, adminUsers, isAdmin, libraries, loadAdminData } from '../../store/app';
import type { AccessSchedule, UserConfiguration, UserDto, UserPolicy } from '../../api/emby';

const selectedUserId = ref('');
const newUserName = ref('');
const newPassword = ref('');
const copyFromUserId = ref('');
const resetPassword = ref('');
const resetPasswordConfirm = ref('');
const error = ref('');
const saving = ref(false);
const authProviderOptions = ref<string[]>(['Default']);

const policyForm = reactive<UserPolicy>({
  IsAdministrator: false,
  IsHidden: false,
  IsDisabled: false,
  EnableRemoteAccess: true,
  EnableRemoteControlOfOtherUsers: false,
  EnableSharedDeviceControl: false,
  EnablePublicSharing: true,
  EnableMediaPlayback: true,
  EnableContentDownloading: true,
  EnableContentDeletion: false,
  EnableAudioPlaybackTranscoding: true,
  EnableVideoPlaybackTranscoding: true,
  EnablePlaybackRemuxing: true,
  ForceRemoteSourceTranscoding: false,
  EnableUserPreferenceAccess: true,
  EnableAllFolders: true,
  EnabledFolders: [],
  BlockedMediaFolders: [],
  EnableAllChannels: true,
  EnabledChannels: [],
  BlockedChannels: [],
  EnableAllDevices: true,
  EnabledDevices: [],
  MaxParentalRating: null,
  MaxParentalSubRating: null,
  MaxActiveSessions: 0,
  LoginAttemptsBeforeLockout: -1,
  RemoteClientBitrateLimit: 0,
  BlockedTags: [],
  BlockUnratedItems: [],
  AccessSchedules: [],
  SyncPlayAccess: 'CreateAndJoinGroups'
});

const configurationForm = reactive<UserConfiguration>({
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
  RememberSubtitleSelections: true,
  EnableLocalPassword: true
});

const tagText = ref('');
const deviceText = ref('');
const enabledChannelText = ref('');
const blockedChannelText = ref('');
const scheduleForm = reactive<AccessSchedule>({
  DayOfWeek: 'Monday',
  StartHour: 0,
  EndHour: 24
});

const selectedUser = computed(() => adminUsers.value.find((user) => user.Id === selectedUserId.value));
const mediaLibraries = computed(() => libraries.value.filter((library) => ['movies', 'tvshows'].includes((library.CollectionType || '').toLowerCase())));

watch(selectedUser, (user) => {
  if (user) {
    Object.assign(policyForm, normalizePolicy(user.Policy));
    Object.assign(configurationForm, normalizeConfiguration(user.Configuration));
    tagText.value = (policyForm.BlockedTags || []).join(', ');
    deviceText.value = (policyForm.EnabledDevices || []).join(', ');
    enabledChannelText.value = (policyForm.EnabledChannels || []).join(', ');
    blockedChannelText.value = (policyForm.BlockedChannels || []).join(', ');
    resetPassword.value = '';
    resetPasswordConfirm.value = '';
  }
}, { immediate: true });

onMounted(async () => {
  if (isAdmin.value) {
    await loadAdminData();
    try {
      const providers = await api.authProviders();
      const values = providers
        .map((provider) => provider.Id || provider.Name)
        .filter(Boolean);
      authProviderOptions.value = values.length ? Array.from(new Set(values)) : ['Default'];
    } catch {
      authProviderOptions.value = ['Default'];
    }
    selectedUserId.value = adminUsers.value[0]?.Id || '';
  }
});

function normalizePolicy(policy: UserDto['Policy']): UserPolicy {
  return {
    IsAdministrator: Boolean(policy?.IsAdministrator),
    IsHidden: Boolean(policy?.IsHidden),
    IsDisabled: Boolean(policy?.IsDisabled),
    EnableRemoteAccess: policy?.EnableRemoteAccess ?? true,
    EnableRemoteControlOfOtherUsers: policy?.EnableRemoteControlOfOtherUsers ?? false,
    EnableSharedDeviceControl: policy?.EnableSharedDeviceControl ?? false,
    EnablePublicSharing: policy?.EnablePublicSharing ?? true,
    EnableMediaPlayback: policy?.EnableMediaPlayback ?? true,
    EnableContentDownloading: policy?.EnableContentDownloading ?? true,
    EnableContentDeletion: policy?.EnableContentDeletion ?? false,
    EnableAudioPlaybackTranscoding: policy?.EnableAudioPlaybackTranscoding ?? true,
    EnableVideoPlaybackTranscoding: policy?.EnableVideoPlaybackTranscoding ?? true,
    EnablePlaybackRemuxing: policy?.EnablePlaybackRemuxing ?? true,
    ForceRemoteSourceTranscoding: policy?.ForceRemoteSourceTranscoding ?? false,
    EnableUserPreferenceAccess: policy?.EnableUserPreferenceAccess ?? true,
    EnableAllFolders: policy?.EnableAllFolders ?? true,
    EnabledFolders: [...(policy?.EnabledFolders || [])],
    BlockedMediaFolders: [...(policy?.BlockedMediaFolders || [])],
    EnableAllChannels: policy?.EnableAllChannels ?? true,
    EnabledChannels: [...(policy?.EnabledChannels || [])],
    BlockedChannels: [...(policy?.BlockedChannels || [])],
    EnableAllDevices: policy?.EnableAllDevices ?? true,
    EnabledDevices: [...(policy?.EnabledDevices || [])],
    AuthenticationProviderId: policy?.AuthenticationProviderId || 'Default',
    PasswordResetProviderId: policy?.PasswordResetProviderId || 'Default',
    MaxParentalRating: policy?.MaxParentalRating ?? null,
    MaxParentalSubRating: policy?.MaxParentalSubRating ?? null,
    MaxActiveSessions: policy?.MaxActiveSessions ?? 0,
    LoginAttemptsBeforeLockout: policy?.LoginAttemptsBeforeLockout ?? -1,
    RemoteClientBitrateLimit: policy?.RemoteClientBitrateLimit ?? 0,
    BlockedTags: [...(policy?.BlockedTags || [])],
    AllowedTags: [...(policy?.AllowedTags || [])],
    BlockUnratedItems: [...(policy?.BlockUnratedItems || [])],
    AccessSchedules: [...(policy?.AccessSchedules || [])],
    SyncPlayAccess: policy?.SyncPlayAccess || 'CreateAndJoinGroups'
  };
}

function normalizeConfiguration(configuration: UserDto['Configuration']): UserConfiguration {
  return {
    PlayDefaultAudioTrack: configuration?.PlayDefaultAudioTrack ?? true,
    PlayDefaultSubtitleTrack: configuration?.PlayDefaultSubtitleTrack ?? false,
    SubtitleMode: configuration?.SubtitleMode || 'Default',
    AudioLanguagePreference: configuration?.AudioLanguagePreference || '',
    SubtitleLanguagePreference: configuration?.SubtitleLanguagePreference || '',
    DisplayMissingEpisodes: configuration?.DisplayMissingEpisodes ?? false,
    GroupedFolders: [...(configuration?.GroupedFolders || [])],
    LatestItemsExcludes: [...(configuration?.LatestItemsExcludes || [])],
    MyMediaExcludes: [...(configuration?.MyMediaExcludes || [])],
    OrderedViews: [...(configuration?.OrderedViews || [])],
    HidePlayedInLatest: configuration?.HidePlayedInLatest ?? false,
    RememberAudioSelections: configuration?.RememberAudioSelections ?? true,
    RememberSubtitleSelections: configuration?.RememberSubtitleSelections ?? true,
    EnableLocalPassword: configuration?.EnableLocalPassword ?? true
  };
}

function listFromText(value: string) {
  return value.split(',').map((item) => item.trim()).filter(Boolean);
}

async function createUser() {
  const name = newUserName.value.trim();
  if (!name) {
    error.value = '请输入用户名';
    return;
  }
  saving.value = true;
  error.value = '';
  try {
    const user = await api.createUser(name, {
      password: newPassword.value.trim() || undefined,
      copyFromUserId: copyFromUserId.value || undefined
    });
    newUserName.value = '';
    newPassword.value = '';
    copyFromUserId.value = '';
    await loadAdminData();
    selectedUserId.value = user.Id;
  } catch (err) {
    error.value = err instanceof Error ? err.message : '创建用户失败';
  } finally {
    saving.value = false;
  }
}

async function saveUser() {
  const user = selectedUser.value;
  if (!user) return;
  saving.value = true;
  error.value = '';
  try {
    const next: UserPolicy = {
      ...policyForm,
      BlockedTags: listFromText(tagText.value),
      EnabledDevices: listFromText(deviceText.value),
      EnabledChannels: policyForm.EnableAllChannels ? [] : listFromText(enabledChannelText.value),
      BlockedChannels: listFromText(blockedChannelText.value),
      EnabledFolders: policyForm.EnableAllFolders ? [] : [...(policyForm.EnabledFolders || [])],
      AccessSchedules: [...(policyForm.AccessSchedules || [])]
    };
    await api.updateUserPolicy(user.Id, next);
    await api.updateUserSettings(user.Id, {
      ...configurationForm,
      GroupedFolders: [...(configurationForm.GroupedFolders || [])],
      LatestItemsExcludes: [...(configurationForm.LatestItemsExcludes || [])],
      MyMediaExcludes: [...(configurationForm.MyMediaExcludes || [])],
      OrderedViews: [...(configurationForm.OrderedViews || [])]
    });
    if (resetPassword.value || resetPasswordConfirm.value) {
      if (resetPassword.value.length < 4) {
        throw new Error('新密码至少需要 4 个字符');
      }
      if (resetPassword.value !== resetPasswordConfirm.value) {
        throw new Error('两次输入的新密码不一致');
      }
      await api.changePassword(user.Id, {
        NewPw: resetPassword.value
      });
      resetPassword.value = '';
      resetPasswordConfirm.value = '';
    }
    await loadAdminData();
  } catch (err) {
    error.value = err instanceof Error ? err.message : '保存用户策略失败';
  } finally {
    saving.value = false;
  }
}

async function removeUser() {
  const user = selectedUser.value;
  if (!user || !window.confirm(`删除用户 ${user.Name}？`)) return;
  saving.value = true;
  error.value = '';
  try {
    await api.deleteUser(user.Id);
    await loadAdminData();
    selectedUserId.value = adminUsers.value[0]?.Id || '';
  } catch (err) {
    error.value = err instanceof Error ? err.message : '删除用户失败';
  } finally {
    saving.value = false;
  }
}

function toggleFolder(id: string, checked: boolean) {
  const folders = new Set(policyForm.EnabledFolders || []);
  checked ? folders.add(id) : folders.delete(id);
  policyForm.EnabledFolders = [...folders];
}

function onFolderChange(id: string, event: Event) {
  toggleFolder(id, (event.target as HTMLInputElement).checked);
}

function toggleBlockedFolder(id: string, checked: boolean) {
  const folders = new Set(policyForm.BlockedMediaFolders || []);
  checked ? folders.add(id) : folders.delete(id);
  policyForm.BlockedMediaFolders = [...folders];
}

function addSchedule() {
  policyForm.AccessSchedules = [...(policyForm.AccessSchedules || []), { ...scheduleForm }];
}

function removeSchedule(index: number) {
  policyForm.AccessSchedules = (policyForm.AccessSchedules || []).filter((_, i) => i !== index);
}
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

      <div v-else class="settings-page users-page">
        <header class="settings-header">
          <div>
            <p>用户</p>
            <h2>用户管理</h2>
          </div>
          <button type="button" :disabled="saving" @click="saveUser">保存更改</button>
        </header>

        <p v-if="error" class="form-error">{{ error }}</p>

        <section class="settings-band user-create">
          <input v-model="newUserName" placeholder="用户名" />
          <input v-model="newPassword" placeholder="初始密码，可留空" type="password" />
          <select v-model="copyFromUserId">
            <option value="">从默认结构创建</option>
            <option v-for="account in adminUsers" :key="account.Id" :value="account.Id">
              复制 {{ account.Name }}
            </option>
          </select>
          <button type="button" :disabled="saving" @click="createUser">新增用户</button>
        </section>

        <section class="users-layout">
          <aside class="user-list">
            <button
              v-for="account in adminUsers"
              :key="account.Id"
              type="button"
              :class="{ active: account.Id === selectedUserId }"
              @click="selectedUserId = account.Id"
            >
              <span>{{ account.Name.slice(0, 1).toUpperCase() }}</span>
              <strong>{{ account.Name }}</strong>
              <small>{{ account.Policy?.IsAdministrator ? '管理员' : account.Policy?.IsDisabled ? '已禁用' : '用户' }}</small>
            </button>
          </aside>

          <div v-if="selectedUser" class="user-editor">
            <section class="settings-band">
              <h3>{{ selectedUser.Name }}</h3>
              <div class="toggle-grid">
                <label><input v-model="policyForm.IsAdministrator" type="checkbox" /> 管理员</label>
                <label><input v-model="policyForm.IsHidden" type="checkbox" /> 登录页隐藏</label>
                <label><input v-model="policyForm.IsDisabled" type="checkbox" /> 禁用用户</label>
                <label><input v-model="policyForm.EnableRemoteAccess" type="checkbox" /> 允许远程访问</label>
                <label><input v-model="policyForm.EnableUserPreferenceAccess" type="checkbox" /> 允许修改个人偏好</label>
              </div>
              <div class="form-grid">
                <label>
                  认证提供器
                  <select v-model="policyForm.AuthenticationProviderId">
                    <option v-for="provider in authProviderOptions" :key="provider" :value="provider">{{ provider }}</option>
                  </select>
                </label>
                <label>
                  密码重置提供器
                  <select v-model="policyForm.PasswordResetProviderId">
                    <option v-for="provider in authProviderOptions" :key="`reset-${provider}`" :value="provider">{{ provider }}</option>
                  </select>
                </label>
              </div>
            </section>

            <section class="settings-band">
              <h3>密码与登录方式</h3>
              <div class="toggle-grid">
                <label><input v-model="configurationForm.EnableLocalPassword" type="checkbox" /> 允许本地密码登录</label>
                <label><input v-model="policyForm.EnablePublicSharing" type="checkbox" /> 允许公共分享</label>
              </div>
              <div class="form-grid">
                <label>重置密码<input v-model="resetPassword" type="password" placeholder="留空表示不修改" /></label>
                <label>确认新密码<input v-model="resetPasswordConfirm" type="password" placeholder="再次输入新密码" /></label>
              </div>
            </section>

            <section class="settings-band">
              <h3>媒体库访问</h3>
              <label><input v-model="policyForm.EnableAllFolders" type="checkbox" /> 允许访问所有媒体库</label>
              <div v-if="!policyForm.EnableAllFolders" class="toggle-grid">
                <label v-for="library in mediaLibraries" :key="library.Id">
                  <input
                    type="checkbox"
                    :checked="policyForm.EnabledFolders?.includes(library.Id)"
                    @change="onFolderChange(library.Id, $event)"
                  />
                  {{ library.Name }}
                </label>
              </div>
              <div class="toggle-grid">
                <label v-for="library in mediaLibraries" :key="`blocked-${library.Id}`">
                  <input
                    type="checkbox"
                    :checked="policyForm.BlockedMediaFolders?.includes(library.Id)"
                    @change="toggleBlockedFolder(library.Id, ($event.target as HTMLInputElement).checked)"
                  />
                  屏蔽 {{ library.Name }}
                </label>
              </div>
            </section>

            <section class="settings-band">
              <h3>播放与下载</h3>
              <div class="toggle-grid">
                <label><input v-model="policyForm.EnableMediaPlayback" type="checkbox" /> 允许播放媒体</label>
                <label><input v-model="policyForm.EnableContentDownloading" type="checkbox" /> 允许下载媒体</label>
                <label><input v-model="policyForm.EnableVideoPlaybackTranscoding" type="checkbox" /> 允许视频转码</label>
                <label><input v-model="policyForm.EnableAudioPlaybackTranscoding" type="checkbox" /> 允许音频转码</label>
                <label><input v-model="policyForm.EnablePlaybackRemuxing" type="checkbox" /> 允许封装转换</label>
                <label><input v-model="policyForm.EnableContentDeletion" type="checkbox" /> 允许删除媒体</label>
                <label><input v-model="policyForm.ForceRemoteSourceTranscoding" type="checkbox" /> 强制远程源转码</label>
              </div>
              <div class="form-grid">
                <label>远程码率上限 bps<input v-model.number="policyForm.RemoteClientBitrateLimit" type="number" min="0" /></label>
                <label>最大活跃会话<input v-model.number="policyForm.MaxActiveSessions" type="number" min="0" /></label>
                <label>
                  SyncPlay 权限
                  <select v-model="policyForm.SyncPlayAccess">
                    <option value="CreateAndJoinGroups">CreateAndJoinGroups</option>
                    <option value="JoinGroups">JoinGroups</option>
                    <option value="None">None</option>
                  </select>
                </label>
              </div>
            </section>

            <section class="settings-band">
              <h3>家长控制</h3>
              <div class="form-grid">
                <label>最高分级值<input v-model.number="policyForm.MaxParentalRating" type="number" min="0" /></label>
                <label>子分级上限<input v-model.number="policyForm.MaxParentalSubRating" type="number" min="0" /></label>
                <label>屏蔽标签<input v-model="tagText" placeholder="多个标签用逗号分隔" /></label>
              </div>
            </section>

            <section class="settings-band">
              <h3>设备与时间</h3>
              <div class="toggle-grid">
                <label><input v-model="policyForm.EnableRemoteControlOfOtherUsers" type="checkbox" /> 允许控制其他用户</label>
                <label><input v-model="policyForm.EnableSharedDeviceControl" type="checkbox" /> 允许共享设备控制</label>
                <label><input v-model="policyForm.EnableAllDevices" type="checkbox" /> 允许所有设备</label>
                <label><input v-model="policyForm.EnableAllChannels" type="checkbox" /> 允许所有频道</label>
              </div>
              <input v-if="!policyForm.EnableAllDevices" v-model="deviceText" placeholder="允许的 DeviceId，逗号分隔" />
              <input v-if="!policyForm.EnableAllChannels" v-model="enabledChannelText" placeholder="允许的 ChannelId，逗号分隔" />
              <input v-model="blockedChannelText" placeholder="屏蔽的 ChannelId，逗号分隔" />
              <div class="form-grid">
                <select v-model="scheduleForm.DayOfWeek">
                  <option>Monday</option>
                  <option>Tuesday</option>
                  <option>Wednesday</option>
                  <option>Thursday</option>
                  <option>Friday</option>
                  <option>Saturday</option>
                  <option>Sunday</option>
                </select>
                <input v-model.number="scheduleForm.StartHour" type="number" min="0" max="24" step="0.5" />
                <input v-model.number="scheduleForm.EndHour" type="number" min="0" max="24" step="0.5" />
                <button type="button" @click="addSchedule">添加时间段</button>
              </div>
              <div class="schedule-list">
                <button v-for="(schedule, index) in policyForm.AccessSchedules" :key="`${schedule.DayOfWeek}-${index}`" type="button" @click="removeSchedule(index)">
                  {{ schedule.DayOfWeek }} {{ schedule.StartHour }}-{{ schedule.EndHour }}
                </button>
              </div>
            </section>

            <section class="settings-band">
              <h3>用户偏好</h3>
              <div class="toggle-grid">
                <label><input v-model="configurationForm.PlayDefaultAudioTrack" type="checkbox" /> 默认选择音轨</label>
                <label><input v-model="configurationForm.PlayDefaultSubtitleTrack" type="checkbox" /> 默认选择字幕</label>
                <label><input v-model="configurationForm.DisplayMissingEpisodes" type="checkbox" /> 显示缺失剧集</label>
                <label><input v-model="configurationForm.HidePlayedInLatest" type="checkbox" /> 最新内容中隐藏已播放</label>
                <label><input v-model="configurationForm.RememberAudioSelections" type="checkbox" /> 记住音轨选择</label>
                <label><input v-model="configurationForm.RememberSubtitleSelections" type="checkbox" /> 记住字幕选择</label>
              </div>
              <div class="form-grid">
                <label>
                  字幕模式
                  <select v-model="configurationForm.SubtitleMode">
                    <option value="Default">Default</option>
                    <option value="Always">Always</option>
                    <option value="OnlyForced">OnlyForced</option>
                    <option value="None">None</option>
                    <option value="Smart">Smart</option>
                  </select>
                </label>
                <label>音频语言偏好<input v-model="configurationForm.AudioLanguagePreference" placeholder="如 zh, en, ja" /></label>
                <label>字幕语言偏好<input v-model="configurationForm.SubtitleLanguagePreference" placeholder="如 zh, en" /></label>
              </div>
            </section>

            <button class="danger" type="button" :disabled="saving" @click="removeUser">删除用户</button>
          </div>
        </section>
      </div>
    </div>
  </section>
</template>

<style scoped>
.users-layout {
  display: grid;
  grid-template-columns: minmax(180px, 260px) 1fr;
  gap: 18px;
}

.user-list {
  display: grid;
  gap: 8px;
  align-content: start;
}

.user-list button {
  display: grid;
  grid-template-columns: 36px 1fr;
  gap: 4px 10px;
  text-align: left;
  align-items: center;
}

.user-list span {
  grid-row: span 2;
  width: 36px;
  height: 36px;
  display: grid;
  place-items: center;
  border-radius: 8px;
  background: var(--surface-muted);
}

.user-list .active {
  outline: 2px solid var(--accent);
}

.user-editor,
.settings-band,
.user-create {
  display: grid;
  gap: 14px;
}

.user-create,
.form-grid,
.toggle-grid {
  grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
  display: grid;
  gap: 12px;
}

.schedule-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.danger {
  justify-self: start;
  border-color: #d45b5b;
  color: #ffd6d6;
}

.form-error {
  color: #ffb4b4;
}

@media (max-width: 760px) {
  .users-layout {
    grid-template-columns: 1fr;
  }
}
</style>
