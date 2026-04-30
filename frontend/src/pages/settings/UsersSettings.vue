<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import { api, adminUsers, isAdmin, libraries, loadAdminData } from '../../store/app';
import type { AccessSchedule, UserConfiguration, UserDto, UserPolicy } from '../../api/emby';

const router = useRouter();

const selectedUserId = ref('');
const newUserName = ref('');
const newPassword = ref('');
const copyFromUserId = ref('');
const resetPassword = ref('');
const resetPasswordConfirm = ref('');
const error = ref('');
const saving = ref(false);
const authProviderOptions = ref<string[]>(['Default']);

const userSearch = ref('');
const userPage = ref(1);
const usersPerPage = 20;

const policyForm = reactive<UserPolicy>({
  IsAdministrator: false,
  IsHidden: false,
  IsHiddenRemotely: false,
  IsDisabled: false,
  EnableRemoteAccess: true,
  EnableRemoteControlOfOtherUsers: false,
  EnableSharedDeviceControl: false,
  EnablePublicSharing: true,
  EnableMediaPlayback: true,
  EnableContentDownloading: true,
  EnableContentDeletion: false,
  EnableSyncTranscoding: true,
  EnableMediaConversion: false,
  EnableCollectionManagement: false,
  EnableSubtitleManagement: true,
  EnableSubtitleDownloading: true,
  EnableLyricManagement: false,
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
  SimultaneousStreamLimit: 0,
  InvalidLoginAttemptCount: 0,
  LoginAttemptsBeforeLockout: -1,
  RemoteClientBitrateLimit: 0,
  BlockedTags: [],
  AllowedTags: [],
  BlockUnratedItems: [],
  AccessSchedules: [],
  SyncPlayAccess: 'CreateAndJoinGroups',
  AuthenticationProviderId: 'Default',
  PasswordResetProviderId: 'Default'
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

const filteredUsers = computed(() => {
  const q = userSearch.value.trim().toLowerCase();
  if (!q) return adminUsers.value;
  return adminUsers.value.filter(
    (u) => u.Name?.toLowerCase().includes(q) || u.Id?.toLowerCase().includes(q)
  );
});
const totalUserPages = computed(() => Math.max(1, Math.ceil(filteredUsers.value.length / usersPerPage)));
const pagedUsers = computed(() => {
  const start = (userPage.value - 1) * usersPerPage;
  return filteredUsers.value.slice(start, start + usersPerPage);
});
const selectedUser = computed(() =>
  adminUsers.value.find((u) => u.Id === selectedUserId.value)
);
const mediaLibraries = computed(() =>
  libraries.value.filter((library) =>
    ['movies', 'tvshows'].includes((library.CollectionType || '').toLowerCase())
  )
);
const subtitleModeOptions = ['Default', 'Always', 'OnlyForced', 'None', 'Smart'].map((v) => ({
  label: v,
  value: v
}));
const syncPlayOptions = ['CreateAndJoinGroups', 'JoinGroups', 'None'].map((v) => ({
  label: v,
  value: v
}));
const dayOfWeekOptions = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'].map(
  (v) => ({ label: v, value: v })
);
const authProviderItems = computed(() =>
  authProviderOptions.value.map((v) => ({ label: v, value: v }))
);
const copyFromOptions = computed(() => [
  { label: '从默认结构创建', value: '' },
  ...adminUsers.value.map((a) => ({ label: `复制 ${a.Name}`, value: a.Id }))
]);

watch(userSearch, () => {
  userPage.value = 1;
});

watch(
  selectedUser,
  (u) => {
    if (u) {
      Object.assign(policyForm, normalizePolicy(u.Policy));
      Object.assign(configurationForm, normalizeConfiguration(u.Configuration));
      tagText.value = (policyForm.BlockedTags || []).join(', ');
      deviceText.value = (policyForm.EnabledDevices || []).join(', ');
      enabledChannelText.value = (policyForm.EnabledChannels || []).join(', ');
      blockedChannelText.value = (policyForm.BlockedChannels || []).join(', ');
      resetPassword.value = '';
      resetPasswordConfirm.value = '';
    }
  },
  { immediate: true }
);

onMounted(async () => {
  if (isAdmin.value) {
    await loadAdminData();
    try {
      const providers = await api.authProviders();
      const values = providers.map((p) => p.Id || p.Name).filter(Boolean);
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
    IsHiddenRemotely: Boolean(policy?.IsHiddenRemotely),
    IsDisabled: Boolean(policy?.IsDisabled),
    EnableRemoteAccess: policy?.EnableRemoteAccess ?? true,
    EnableRemoteControlOfOtherUsers: policy?.EnableRemoteControlOfOtherUsers ?? false,
    EnableSharedDeviceControl: policy?.EnableSharedDeviceControl ?? false,
    EnablePublicSharing: policy?.EnablePublicSharing ?? true,
    EnableMediaPlayback: policy?.EnableMediaPlayback ?? true,
    EnableContentDownloading: policy?.EnableContentDownloading ?? true,
    EnableContentDeletion: policy?.EnableContentDeletion ?? false,
    EnableSyncTranscoding: policy?.EnableSyncTranscoding ?? true,
    EnableMediaConversion: policy?.EnableMediaConversion ?? false,
    EnableCollectionManagement: policy?.EnableCollectionManagement ?? false,
    EnableSubtitleManagement: policy?.EnableSubtitleManagement ?? true,
    EnableSubtitleDownloading: policy?.EnableSubtitleDownloading ?? true,
    EnableLyricManagement: policy?.EnableLyricManagement ?? false,
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
    SimultaneousStreamLimit: policy?.SimultaneousStreamLimit ?? 0,
    InvalidLoginAttemptCount: policy?.InvalidLoginAttemptCount ?? 0,
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
    const u = await api.createUser(name, {
      password: newPassword.value.trim() || undefined,
      copyFromUserId: copyFromUserId.value || undefined
    });
    newUserName.value = '';
    newPassword.value = '';
    copyFromUserId.value = '';
    await loadAdminData();
    selectedUserId.value = u.Id;
  } catch (err) {
    error.value = err instanceof Error ? err.message : '创建用户失败';
  } finally {
    saving.value = false;
  }
}

async function saveUser() {
  const u = selectedUser.value;
  if (!u) return;
  if (resetPassword.value || resetPasswordConfirm.value) {
    if (resetPassword.value.length < 4) {
      error.value = '新密码至少需要 4 个字符';
      return;
    }
    if (resetPassword.value !== resetPasswordConfirm.value) {
      error.value = '两次输入的新密码不一致';
      return;
    }
  }
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
    await api.updateUserPolicy(u.Id, next);
    await api.updateUserSettings(u.Id, {
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
      await api.changePassword(u.Id, {
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
  const u = selectedUser.value;
  if (!u || !window.confirm(`删除用户 ${u.Name}？`)) return;
  saving.value = true;
  error.value = '';
  try {
    await api.deleteUser(u.Id);
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

function userStatus(account: UserDto) {
  if (account.Policy?.IsAdministrator) return '管理员';
  if (account.Policy?.IsDisabled) return '已禁用';
  return '用户';
}
</script>

<template>
  <SettingsLayout>
    <div
      v-if="!isAdmin"
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
    >
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能查看全部用户列表。</p>
    </div>

    <div v-else class="space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">Users</p>
          <h2 class="text-highlighted text-xl font-semibold">用户管理</h2>
        </div>
        <div class="flex flex-wrap items-center gap-2">
          <UButton
            color="neutral"
            variant="soft"
            icon="i-lucide-database"
            @click="router.push('/settings/users/import-emby')"
          >
            从 Emby 导入
          </UButton>
          <UButton icon="i-lucide-save" :loading="saving" @click="saveUser">保存更改</UButton>
        </div>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">新建用户</h3>
        </template>
        <div class="grid gap-3 sm:grid-cols-4">
          <UFormField label="用户名">
            <UInput v-model="newUserName" class="w-full" />
          </UFormField>
          <UFormField label="初始密码" hint="可留空">
            <UInput v-model="newPassword" type="password" class="w-full" />
          </UFormField>
          <UFormField label="复制模板">
            <USelect v-model="copyFromUserId" :items="copyFromOptions" class="w-full" />
          </UFormField>
          <div class="flex items-end">
            <UButton icon="i-lucide-user-plus" :loading="saving" class="w-full justify-center" @click="createUser">
              新增用户
            </UButton>
          </div>
        </div>
      </UCard>

      <div class="grid gap-4 lg:grid-cols-[280px_1fr]">
        <aside class="space-y-2">
          <UInput
            v-model="userSearch"
            icon="i-lucide-search"
            placeholder="搜索用户…"
            class="w-full"
          />
          <p class="text-muted text-xs">
            共 {{ filteredUsers.length }} 个用户
            <template v-if="totalUserPages > 1">
              · 第 {{ userPage }}/{{ totalUserPages }} 页
            </template>
          </p>
          <div
            v-for="account in pagedUsers"
            :key="account.Id"
            class="flex w-full items-center gap-3 rounded-lg border border-default p-3 transition"
            :class="
              account.Id === selectedUserId
                ? 'bg-primary/10 ring-1 ring-primary/50'
                : 'hover:bg-elevated/50'
            "
          >
            <button
              type="button"
              class="flex min-w-0 flex-1 items-center gap-3 text-start"
              @click="selectedUserId = account.Id"
            >
              <UAvatar :alt="account.Name" :text="account.Name.slice(0, 1).toUpperCase()" size="sm" />
              <div class="min-w-0 flex-1">
                <p class="text-highlighted truncate text-sm font-medium">{{ account.Name }}</p>
                <p class="text-muted truncate text-xs">{{ userStatus(account) }}</p>
              </div>
            </button>
            <UButton
              variant="ghost"
              icon="i-lucide-external-link"
              size="xs"
              title="查看详情"
              @click="router.push(`/settings/users/${account.Id}`)"
            />
          </div>
          <div v-if="totalUserPages > 1" class="flex items-center justify-center gap-1 pt-1">
            <UButton
              size="xs"
              variant="ghost"
              icon="i-lucide-chevron-left"
              :disabled="userPage <= 1"
              @click="userPage = Math.max(1, userPage - 1)"
            />
            <span class="text-muted text-xs tabular-nums">{{ userPage }} / {{ totalUserPages }}</span>
            <UButton
              size="xs"
              variant="ghost"
              icon="i-lucide-chevron-right"
              :disabled="userPage >= totalUserPages"
              @click="userPage = Math.min(totalUserPages, userPage + 1)"
            />
          </div>
        </aside>

        <div v-if="selectedUser" class="space-y-4">
          <UCard>
            <template #header>
              <div class="flex items-center justify-between">
                <h3 class="text-highlighted text-sm font-semibold">{{ selectedUser.Name }}</h3>
                <UBadge v-if="policyForm.IsAdministrator" color="primary" variant="subtle">管理员</UBadge>
                <UBadge v-else-if="policyForm.IsDisabled" color="error" variant="subtle">已禁用</UBadge>
              </div>
            </template>
            <div class="grid gap-3 sm:grid-cols-2">
              <USwitch v-model="policyForm.IsAdministrator" label="管理员" />
              <USwitch v-model="policyForm.IsHidden" label="登录页隐藏" />
              <USwitch v-model="policyForm.IsDisabled" label="禁用用户" />
              <USwitch v-model="policyForm.EnableRemoteAccess" label="允许远程访问" />
              <USwitch v-model="policyForm.EnableUserPreferenceAccess" label="允许修改个人偏好" />
            </div>
            <div class="mt-4 grid gap-3 sm:grid-cols-2">
              <UFormField label="认证提供器">
                <USelect v-model="policyForm.AuthenticationProviderId" :items="authProviderItems" class="w-full" />
              </UFormField>
              <UFormField label="密码重置提供器">
                <USelect v-model="policyForm.PasswordResetProviderId" :items="authProviderItems" class="w-full" />
              </UFormField>
            </div>
          </UCard>

          <UCard>
            <template #header>
              <h3 class="text-highlighted text-sm font-semibold">密码与登录方式</h3>
            </template>
            <div class="grid gap-3 sm:grid-cols-2">
              <USwitch v-model="configurationForm.EnableLocalPassword" label="允许本地密码登录" />
              <USwitch v-model="policyForm.EnablePublicSharing" label="允许公共分享" />
            </div>
            <div class="mt-4 grid gap-3 sm:grid-cols-2">
              <UFormField label="重置密码" hint="留空表示不修改">
                <UInput v-model="resetPassword" type="password" class="w-full" />
              </UFormField>
              <UFormField label="确认新密码">
                <UInput v-model="resetPasswordConfirm" type="password" class="w-full" />
              </UFormField>
            </div>
          </UCard>

          <UCard>
            <template #header>
              <div class="flex items-center justify-between">
                <h3 class="text-highlighted text-sm font-semibold">媒体库访问</h3>
                <USwitch v-model="policyForm.EnableAllFolders" label="允许访问所有媒体库" />
              </div>
            </template>
            <div v-if="!policyForm.EnableAllFolders" class="grid gap-2 sm:grid-cols-2">
              <label
                v-for="library in mediaLibraries"
                :key="library.Id"
                class="flex items-center gap-2 rounded-md border border-default p-2 text-sm"
              >
                <UCheckbox
                  :model-value="policyForm.EnabledFolders?.includes(library.Id)"
                  @update:model-value="(v: boolean) => toggleFolder(library.Id, v)"
                />
                {{ library.Name }}
              </label>
            </div>
            <div class="mt-3">
              <p class="text-muted mb-2 text-xs">屏蔽媒体库</p>
              <div class="grid gap-2 sm:grid-cols-2">
                <label
                  v-for="library in mediaLibraries"
                  :key="`blocked-${library.Id}`"
                  class="flex items-center gap-2 rounded-md border border-default p-2 text-sm"
                >
                  <UCheckbox
                    :model-value="policyForm.BlockedMediaFolders?.includes(library.Id)"
                    @update:model-value="(v: boolean) => toggleBlockedFolder(library.Id, v)"
                  />
                  屏蔽 {{ library.Name }}
                </label>
              </div>
            </div>
          </UCard>

          <UCard>
            <template #header>
              <h3 class="text-highlighted text-sm font-semibold">播放与下载</h3>
            </template>
            <div class="grid gap-3 sm:grid-cols-2">
              <USwitch v-model="policyForm.EnableMediaPlayback" label="允许播放媒体" />
              <USwitch v-model="policyForm.EnableContentDownloading" label="允许下载媒体" />
              <USwitch v-model="policyForm.EnableVideoPlaybackTranscoding" label="允许视频转码" />
              <USwitch v-model="policyForm.EnableAudioPlaybackTranscoding" label="允许音频转码" />
              <USwitch v-model="policyForm.EnablePlaybackRemuxing" label="允许封装转换" />
              <USwitch v-model="policyForm.EnableSyncTranscoding" label="允许同步转码" />
              <USwitch v-model="policyForm.EnableMediaConversion" label="允许媒体转换" />
              <USwitch v-model="policyForm.EnableContentDeletion" label="允许删除媒体" />
              <USwitch v-model="policyForm.EnableCollectionManagement" label="允许管理合集" />
              <USwitch v-model="policyForm.EnableSubtitleManagement" label="允许管理字幕" />
              <USwitch v-model="policyForm.EnableSubtitleDownloading" label="允许下载字幕" />
              <USwitch v-model="policyForm.EnableLyricManagement" label="允许管理歌词" />
              <USwitch v-model="policyForm.ForceRemoteSourceTranscoding" label="强制远程源转码" />
            </div>
            <div class="mt-4 grid gap-3 sm:grid-cols-3">
              <UFormField label="远程码率上限 (bps)">
                <UInput v-model.number="policyForm.RemoteClientBitrateLimit" type="number" :min="0" class="w-full" />
              </UFormField>
              <UFormField label="最大活跃会话">
                <UInput v-model.number="policyForm.SimultaneousStreamLimit" type="number" :min="0" class="w-full" />
              </UFormField>
              <UFormField label="SyncPlay 权限">
                <USelect v-model="policyForm.SyncPlayAccess" :items="syncPlayOptions" class="w-full" />
              </UFormField>
            </div>
          </UCard>

          <UCard>
            <template #header>
              <h3 class="text-highlighted text-sm font-semibold">家长控制</h3>
            </template>
            <div class="grid gap-3 sm:grid-cols-3">
              <UFormField label="最高分级值">
                <UInput v-model.number="policyForm.MaxParentalRating" type="number" :min="0" class="w-full" />
              </UFormField>
              <UFormField label="子分级上限">
                <UInput v-model.number="policyForm.MaxParentalSubRating" type="number" :min="0" class="w-full" />
              </UFormField>
              <UFormField label="屏蔽标签" hint="多个标签用逗号分隔">
                <UInput v-model="tagText" class="w-full" />
              </UFormField>
            </div>
          </UCard>

          <UCard>
            <template #header>
              <h3 class="text-highlighted text-sm font-semibold">设备与时间</h3>
            </template>
            <div class="grid gap-3 sm:grid-cols-2">
              <USwitch v-model="policyForm.EnableRemoteControlOfOtherUsers" label="允许控制其他用户" />
              <USwitch v-model="policyForm.EnableSharedDeviceControl" label="允许共享设备控制" />
              <USwitch v-model="policyForm.EnableAllDevices" label="允许所有设备" />
              <USwitch v-model="policyForm.EnableAllChannels" label="允许所有频道" />
            </div>
            <div class="mt-4 space-y-3">
              <UFormField v-if="!policyForm.EnableAllDevices" label="允许的 DeviceId" hint="逗号分隔">
                <UInput v-model="deviceText" class="w-full" />
              </UFormField>
              <UFormField v-if="!policyForm.EnableAllChannels" label="允许的 ChannelId" hint="逗号分隔">
                <UInput v-model="enabledChannelText" class="w-full" />
              </UFormField>
              <UFormField label="屏蔽的 ChannelId" hint="逗号分隔">
                <UInput v-model="blockedChannelText" class="w-full" />
              </UFormField>
            </div>

            <div class="mt-4">
              <p class="text-muted mb-2 text-xs">访问时间段</p>
              <div class="grid gap-2 sm:grid-cols-[1fr_1fr_1fr_auto]">
                <USelect v-model="scheduleForm.DayOfWeek" :items="dayOfWeekOptions" class="w-full" />
                <UInput v-model.number="scheduleForm.StartHour" type="number" :min="0" :max="24" :step="0.5" placeholder="开始" />
                <UInput v-model.number="scheduleForm.EndHour" type="number" :min="0" :max="24" :step="0.5" placeholder="结束" />
                <UButton color="neutral" variant="subtle" icon="i-lucide-plus" @click="addSchedule">添加</UButton>
              </div>
              <div v-if="(policyForm.AccessSchedules || []).length" class="mt-3 flex flex-wrap gap-2">
                <UButton
                  v-for="(schedule, index) in policyForm.AccessSchedules"
                  :key="`${schedule.DayOfWeek}-${index}`"
                  color="neutral"
                  variant="soft"
                  size="xs"
                  trailing-icon="i-lucide-x"
                  @click="removeSchedule(index)"
                >
                  {{ schedule.DayOfWeek }} {{ schedule.StartHour }}-{{ schedule.EndHour }}
                </UButton>
              </div>
            </div>
          </UCard>

          <UCard>
            <template #header>
              <h3 class="text-highlighted text-sm font-semibold">用户偏好</h3>
            </template>
            <div class="grid gap-3 sm:grid-cols-2">
              <USwitch v-model="configurationForm.PlayDefaultAudioTrack" label="默认选择音轨" />
              <USwitch v-model="configurationForm.PlayDefaultSubtitleTrack" label="默认选择字幕" />
              <USwitch v-model="configurationForm.DisplayMissingEpisodes" label="显示缺失剧集" />
              <USwitch v-model="configurationForm.HidePlayedInLatest" label="最新内容中隐藏已播放" />
              <USwitch v-model="configurationForm.RememberAudioSelections" label="记住音轨选择" />
              <USwitch v-model="configurationForm.RememberSubtitleSelections" label="记住字幕选择" />
            </div>
            <div class="mt-4 grid gap-3 sm:grid-cols-3">
              <UFormField label="字幕模式">
                <USelect v-model="configurationForm.SubtitleMode" :items="subtitleModeOptions" class="w-full" />
              </UFormField>
              <UFormField label="音频语言偏好">
                <UInput v-model="configurationForm.AudioLanguagePreference" placeholder="如 zh, en, ja" class="w-full" />
              </UFormField>
              <UFormField label="字幕语言偏好">
                <UInput v-model="configurationForm.SubtitleLanguagePreference" placeholder="如 zh, en" class="w-full" />
              </UFormField>
            </div>
            <template #footer>
              <div class="flex justify-between">
                <UButton color="error" variant="soft" icon="i-lucide-trash-2" :disabled="saving" @click="removeUser">
                  删除用户
                </UButton>
                <UButton icon="i-lucide-save" :loading="saving" @click="saveUser">保存更改</UButton>
              </div>
            </template>
          </UCard>
        </div>

        <div v-else class="text-muted rounded-xl border border-dashed border-default p-8 text-center text-sm">
          请在左侧选择一个用户
        </div>
      </div>
    </div>
  </SettingsLayout>
</template>
