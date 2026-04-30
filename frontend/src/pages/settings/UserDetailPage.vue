<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import { api, isAdmin, user as currentUser, libraries, loadAdminData } from '../../store/app';
import type { ActivityLogEntry, UserDto, UserPolicy } from '../../api/emby';

const route = useRoute();
const router = useRouter();

const userId = computed(() => route.params.userId as string);
const isSelf = computed(() => currentUser.value?.Id === userId.value);

const loading = ref(true);
const saving = ref(false);
const error = ref('');
const success = ref('');
const userDetail = ref<UserDto | null>(null);
const activityEntries = ref<ActivityLogEntry[]>([]);
const activityLoading = ref(false);

const activeTab = ref('profile');
const tabItems = [
  { label: '个人资料', value: 'profile' },
  { label: '权限', value: 'policy' },
  { label: '活动', value: 'activity' }
];

const passwordForm = reactive({
  currentPassword: '',
  newPassword: '',
  confirmPassword: ''
});

const policyForm = reactive<UserPolicy>({
  IsAdministrator: false,
  IsHidden: false,
  IsHiddenRemotely: false,
  IsDisabled: false,
  EnableRemoteAccess: true,
  EnableMediaPlayback: true,
  EnableAudioPlaybackTranscoding: true,
  EnableVideoPlaybackTranscoding: true,
  EnableContentDeletion: false,
  EnableRemoteControlOfOtherUsers: false,
  EnableSharedDeviceControl: false,
  EnablePublicSharing: true,
  EnableContentDownloading: true,
  EnablePlaybackRemuxing: true,
  EnableSyncTranscoding: true,
  EnableMediaConversion: false,
  EnableCollectionManagement: false,
  EnableSubtitleManagement: true,
  EnableSubtitleDownloading: true,
  EnableLyricManagement: false,
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

const userName = ref('');

const mediaLibraries = computed(() =>
  libraries.value.filter((lib) =>
    ['movies', 'tvshows'].includes((lib.CollectionType || '').toLowerCase())
  )
);

const bitrateOptions = [
  { label: '不限制', value: 0 },
  { label: '480p · 1 Mbps', value: 1_000_000 },
  { label: '720p · 3 Mbps', value: 3_000_000 },
  { label: '1080p · 8 Mbps', value: 8_000_000 },
  { label: '1080p · 20 Mbps', value: 20_000_000 },
  { label: '4K · 40 Mbps', value: 40_000_000 },
  { label: '4K · 80 Mbps', value: 80_000_000 },
  { label: '原始 · 140 Mbps', value: 140_000_000 }
];

function applyUserData(u: UserDto) {
  userDetail.value = u;
  userName.value = u.Name;
  const p = u.Policy;
  Object.assign(policyForm, {
    IsAdministrator: Boolean(p?.IsAdministrator),
    IsHidden: Boolean(p?.IsHidden),
    IsHiddenRemotely: Boolean(p?.IsHiddenRemotely),
    IsDisabled: Boolean(p?.IsDisabled),
    EnableRemoteAccess: p?.EnableRemoteAccess ?? true,
    EnableMediaPlayback: p?.EnableMediaPlayback ?? true,
    EnableAudioPlaybackTranscoding: p?.EnableAudioPlaybackTranscoding ?? true,
    EnableVideoPlaybackTranscoding: p?.EnableVideoPlaybackTranscoding ?? true,
    EnableContentDeletion: p?.EnableContentDeletion ?? false,
    EnableRemoteControlOfOtherUsers: p?.EnableRemoteControlOfOtherUsers ?? false,
    EnableSharedDeviceControl: p?.EnableSharedDeviceControl ?? false,
    EnablePublicSharing: p?.EnablePublicSharing ?? true,
    EnableContentDownloading: p?.EnableContentDownloading ?? true,
    EnablePlaybackRemuxing: p?.EnablePlaybackRemuxing ?? true,
    EnableSyncTranscoding: p?.EnableSyncTranscoding ?? true,
    EnableMediaConversion: p?.EnableMediaConversion ?? false,
    EnableCollectionManagement: p?.EnableCollectionManagement ?? false,
    EnableSubtitleManagement: p?.EnableSubtitleManagement ?? true,
    EnableSubtitleDownloading: p?.EnableSubtitleDownloading ?? true,
    EnableLyricManagement: p?.EnableLyricManagement ?? false,
    ForceRemoteSourceTranscoding: p?.ForceRemoteSourceTranscoding ?? false,
    EnableUserPreferenceAccess: p?.EnableUserPreferenceAccess ?? true,
    EnableAllFolders: p?.EnableAllFolders ?? true,
    EnabledFolders: [...(p?.EnabledFolders || [])],
    BlockedMediaFolders: [...(p?.BlockedMediaFolders || [])],
    EnableAllChannels: p?.EnableAllChannels ?? true,
    EnabledChannels: [...(p?.EnabledChannels || [])],
    BlockedChannels: [...(p?.BlockedChannels || [])],
    EnableAllDevices: p?.EnableAllDevices ?? true,
    EnabledDevices: [...(p?.EnabledDevices || [])],
    MaxParentalRating: p?.MaxParentalRating ?? null,
    MaxParentalSubRating: p?.MaxParentalSubRating ?? null,
    SimultaneousStreamLimit: p?.SimultaneousStreamLimit ?? 0,
    InvalidLoginAttemptCount: p?.InvalidLoginAttemptCount ?? 0,
    LoginAttemptsBeforeLockout: p?.LoginAttemptsBeforeLockout ?? -1,
    RemoteClientBitrateLimit: p?.RemoteClientBitrateLimit ?? 0,
    BlockedTags: [...(p?.BlockedTags || [])],
    AllowedTags: [...(p?.AllowedTags || [])],
    BlockUnratedItems: [...(p?.BlockUnratedItems || [])],
    AccessSchedules: [...(p?.AccessSchedules || [])],
    SyncPlayAccess: p?.SyncPlayAccess || 'CreateAndJoinGroups',
    AuthenticationProviderId: p?.AuthenticationProviderId || 'Default',
    PasswordResetProviderId: p?.PasswordResetProviderId || 'Default'
  });
}

async function loadUser() {
  loading.value = true;
  error.value = '';
  try {
    const u = await api.getUser(userId.value);
    applyUserData(u);
  } catch (e) {
    error.value = e instanceof Error ? e.message : '加载用户失败';
  } finally {
    loading.value = false;
  }
}

async function loadActivity() {
  activityLoading.value = true;
  try {
    const res = await api.activity(100, userId.value);
    activityEntries.value = res.Items;
  } catch {
    activityEntries.value = [];
  } finally {
    activityLoading.value = false;
  }
}

watch(activeTab, (tab) => {
  if (tab === 'activity' && activityEntries.value.length === 0) {
    loadActivity();
  }
});

onMounted(async () => {
  if (isAdmin.value) {
    await loadAdminData();
  }
  await loadUser();
});

async function updatePassword() {
  error.value = '';
  success.value = '';
  if (passwordForm.newPassword.length < 4) {
    error.value = '新密码至少需要 4 个字符';
    return;
  }
  if (passwordForm.newPassword !== passwordForm.confirmPassword) {
    error.value = '两次输入的新密码不一致';
    return;
  }
  saving.value = true;
  try {
    const payload: { CurrentPw?: string; NewPw: string } = {
      NewPw: passwordForm.newPassword
    };
    if (isSelf.value) {
      payload.CurrentPw = passwordForm.currentPassword;
    }
    await api.changePassword(userId.value, payload);
    passwordForm.currentPassword = '';
    passwordForm.newPassword = '';
    passwordForm.confirmPassword = '';
    success.value = '密码已更新';
  } catch (e) {
    error.value = e instanceof Error ? e.message : '更新密码失败';
  } finally {
    saving.value = false;
  }
}

async function savePolicy() {
  error.value = '';
  success.value = '';
  saving.value = true;
  try {
    const next: UserPolicy = {
      ...policyForm,
      EnabledFolders: policyForm.EnableAllFolders ? [] : [...(policyForm.EnabledFolders || [])],
      AccessSchedules: [...(policyForm.AccessSchedules || [])]
    };
    await api.updateUserPolicy(userId.value, next);
    success.value = '权限已保存';
    const u = await api.getUser(userId.value);
    applyUserData(u);
  } catch (e) {
    error.value = e instanceof Error ? e.message : '保存权限失败';
  } finally {
    saving.value = false;
  }
}

function toggleFolder(id: string, checked: boolean) {
  const folders = new Set(policyForm.EnabledFolders || []);
  checked ? folders.add(id) : folders.delete(id);
  policyForm.EnabledFolders = [...folders];
}

function formatDate(value: string) {
  return new Date(value).toLocaleString('zh-CN');
}

function goBack() {
  router.push('/settings/users');
}
</script>

<template>
  <SettingsLayout>
    <div v-if="loading" class="flex min-h-[30vh] flex-col items-center justify-center gap-2">
      <UProgress animation="carousel" class="w-48" />
      <p class="text-muted text-sm">正在加载用户信息…</p>
    </div>

    <UAlert v-else-if="error && !userDetail" color="error" icon="i-lucide-triangle-alert" :description="error" />

    <div v-else-if="userDetail" class="space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex items-center gap-3">
          <UButton variant="ghost" icon="i-lucide-arrow-left" size="sm" @click="goBack" />
          <div>
            <p class="text-muted text-xs uppercase tracking-wider">用户详情</p>
            <h2 class="text-highlighted text-xl font-semibold">{{ userDetail.Name }}</h2>
          </div>
          <UBadge v-if="userDetail.Policy?.IsAdministrator" color="primary" variant="subtle">管理员</UBadge>
          <UBadge v-if="isSelf" color="info" variant="subtle">当前用户</UBadge>
        </div>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" class="mb-2" />
      <UAlert v-if="success" color="success" icon="i-lucide-check" :description="success" class="mb-2" />

      <UTabs v-model="activeTab" :items="tabItems" variant="link" :content="false" />

      <!-- Tab: 个人资料 -->
      <div v-if="activeTab === 'profile'" class="space-y-4">
        <UCard>
          <template #header>
            <h3 class="text-highlighted text-sm font-semibold">基本信息</h3>
          </template>
          <div class="grid gap-4 sm:grid-cols-2">
            <UFormField label="用户名">
              <UInput v-model="userName" class="w-full" disabled />
            </UFormField>
            <UFormField label="上次登录">
              <UInput
                :model-value="userDetail.LastLoginDate ? formatDate(userDetail.LastLoginDate) : '从未登录'"
                class="w-full"
                disabled
              />
            </UFormField>
          </div>
        </UCard>

        <UCard>
          <template #header>
            <h3 class="text-highlighted text-sm font-semibold">修改密码</h3>
          </template>
          <div class="grid gap-4 sm:grid-cols-2">
            <UFormField v-if="isSelf" label="当前密码">
              <UInput v-model="passwordForm.currentPassword" type="password" autocomplete="current-password" class="w-full" />
            </UFormField>
            <div v-if="isSelf" />
            <UFormField label="新密码">
              <UInput v-model="passwordForm.newPassword" type="password" autocomplete="new-password" class="w-full" />
            </UFormField>
            <UFormField label="确认新密码">
              <UInput v-model="passwordForm.confirmPassword" type="password" autocomplete="new-password" class="w-full" />
            </UFormField>
          </div>
          <template #footer>
            <div class="flex justify-end">
              <UButton :loading="saving" @click="updatePassword">更新密码</UButton>
            </div>
          </template>
        </UCard>
      </div>

      <!-- Tab: 权限 -->
      <div v-if="activeTab === 'policy'" class="space-y-4">
        <UCard>
          <template #header>
            <h3 class="text-highlighted text-sm font-semibold">基本权限</h3>
          </template>
          <div class="grid gap-3 sm:grid-cols-2">
            <USwitch v-model="policyForm.IsAdministrator" label="管理员" />
            <USwitch v-model="policyForm.EnableMediaPlayback" label="允许播放媒体" />
            <USwitch v-model="policyForm.EnableAudioPlaybackTranscoding" label="允许音频转码" />
            <USwitch v-model="policyForm.EnableVideoPlaybackTranscoding" label="允许视频转码" />
            <USwitch v-model="policyForm.EnablePlaybackRemuxing" label="允许封装转换" />
            <USwitch v-model="policyForm.EnableSyncTranscoding" label="允许同步转码" />
            <USwitch v-model="policyForm.EnableMediaConversion" label="允许媒体转换" />
            <USwitch v-model="policyForm.EnableContentDeletion" label="允许删除内容" />
            <USwitch v-model="policyForm.EnableContentDownloading" label="允许下载内容" />
            <USwitch v-model="policyForm.EnableCollectionManagement" label="允许管理合集" />
            <USwitch v-model="policyForm.EnableSubtitleManagement" label="允许管理字幕" />
            <USwitch v-model="policyForm.EnableSubtitleDownloading" label="允许下载字幕" />
            <USwitch v-model="policyForm.EnableLyricManagement" label="允许管理歌词" />
            <USwitch v-model="policyForm.ForceRemoteSourceTranscoding" label="强制远程源转码" />
            <USwitch v-model="policyForm.EnableRemoteAccess" label="允许远程访问" />
            <USwitch v-model="policyForm.IsHidden" label="登录页隐藏" />
            <USwitch v-model="policyForm.IsDisabled" label="禁用用户" />
          </div>
        </UCard>

        <UCard>
          <template #header>
            <h3 class="text-highlighted text-sm font-semibold">串流限制</h3>
          </template>
          <div class="grid gap-4 sm:grid-cols-2">
            <UFormField label="最大串流比特率">
              <USelect
                v-model.number="policyForm.RemoteClientBitrateLimit"
                :items="bitrateOptions"
                value-key="value"
                class="w-full"
              />
            </UFormField>
            <UFormField label="最大活跃会话">
              <UInput v-model.number="policyForm.SimultaneousStreamLimit" type="number" :min="0" class="w-full" />
            </UFormField>
          </div>
        </UCard>

        <UCard>
          <template #header>
            <div class="flex items-center justify-between">
              <h3 class="text-highlighted text-sm font-semibold">媒体库访问</h3>
              <USwitch v-model="policyForm.EnableAllFolders" label="允许访问所有文件夹" />
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
          <p v-if="!policyForm.EnableAllFolders && !mediaLibraries.length" class="text-muted text-sm">
            没有可用的媒体库
          </p>
        </UCard>

        <div class="flex justify-end">
          <UButton icon="i-lucide-save" :loading="saving" @click="savePolicy">保存</UButton>
        </div>
      </div>

      <!-- Tab: 活动 -->
      <div v-if="activeTab === 'activity'">
        <div v-if="activityLoading" class="flex min-h-[20vh] flex-col items-center justify-center gap-2">
          <UProgress animation="carousel" class="w-48" />
          <p class="text-muted text-sm">正在加载活动记录…</p>
        </div>

        <UCard v-else>
          <template #header>
            <div class="flex items-center justify-between">
              <h3 class="text-highlighted text-sm font-semibold">最近活动</h3>
              <UBadge variant="subtle" color="neutral">{{ activityEntries.length }}</UBadge>
            </div>
          </template>
          <div v-if="activityEntries.length" class="space-y-3">
            <div
              v-for="entry in activityEntries"
              :key="entry.Id"
              class="flex items-start gap-3 rounded-lg border border-default p-3"
            >
              <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
                <UIcon name="i-lucide-activity" class="size-4" />
              </div>
              <div class="min-w-0 flex-1">
                <p class="text-highlighted truncate text-sm font-medium">{{ entry.Name }}</p>
                <p class="text-muted truncate text-xs">{{ entry.ShortOverview || entry.Type }}</p>
                <p class="text-dimmed mt-1 text-[11px]">{{ formatDate(entry.Date) }}</p>
              </div>
            </div>
          </div>
          <p v-else class="text-muted text-sm">暂时没有活动记录。</p>
        </UCard>
      </div>
    </div>
  </SettingsLayout>
</template>
