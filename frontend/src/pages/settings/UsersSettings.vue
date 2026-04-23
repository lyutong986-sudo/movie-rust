<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';
import { api, adminUsers, isAdmin, libraries, loadAdminData } from '../../store/app';
import type { AccessSchedule, UserDto, UserPolicy } from '../../api/emby';

const selectedUserId = ref('');
const newUserName = ref('');
const newPassword = ref('');
const copyFromUserId = ref('');
const error = ref('');
const saving = ref(false);

const policyForm = reactive<UserPolicy>({
  IsAdministrator: false,
  IsHidden: false,
  IsDisabled: false,
  EnableRemoteAccess: true,
  EnableMediaPlayback: true,
  EnableContentDownloading: true,
  EnableContentDeletion: false,
  EnableAudioPlaybackTranscoding: true,
  EnableVideoPlaybackTranscoding: true,
  EnablePlaybackRemuxing: true,
  EnableUserPreferenceAccess: true,
  EnableAllFolders: true,
  EnabledFolders: [],
  EnableAllDevices: true,
  EnabledDevices: [],
  MaxParentalRating: null,
  MaxActiveSessions: 0,
  LoginAttemptsBeforeLockout: -1,
  RemoteClientBitrateLimit: 0,
  BlockedTags: [],
  BlockUnratedItems: [],
  AccessSchedules: []
});

const tagText = ref('');
const deviceText = ref('');
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
    tagText.value = (policyForm.BlockedTags || []).join(', ');
    deviceText.value = (policyForm.EnabledDevices || []).join(', ');
  }
}, { immediate: true });

onMounted(async () => {
  if (isAdmin.value) {
    await loadAdminData();
    selectedUserId.value = adminUsers.value[0]?.Id || '';
  }
});

function normalizePolicy(policy: UserDto['Policy']): UserPolicy {
  return {
    IsAdministrator: Boolean(policy?.IsAdministrator),
    IsHidden: Boolean(policy?.IsHidden),
    IsDisabled: Boolean(policy?.IsDisabled),
    EnableRemoteAccess: policy?.EnableRemoteAccess ?? true,
    EnableMediaPlayback: policy?.EnableMediaPlayback ?? true,
    EnableContentDownloading: policy?.EnableContentDownloading ?? true,
    EnableContentDeletion: policy?.EnableContentDeletion ?? false,
    EnableAudioPlaybackTranscoding: policy?.EnableAudioPlaybackTranscoding ?? true,
    EnableVideoPlaybackTranscoding: policy?.EnableVideoPlaybackTranscoding ?? true,
    EnablePlaybackRemuxing: policy?.EnablePlaybackRemuxing ?? true,
    EnableUserPreferenceAccess: policy?.EnableUserPreferenceAccess ?? true,
    EnableAllFolders: policy?.EnableAllFolders ?? true,
    EnabledFolders: [...(policy?.EnabledFolders || [])],
    EnableAllDevices: policy?.EnableAllDevices ?? true,
    EnabledDevices: [...(policy?.EnabledDevices || [])],
    MaxParentalRating: policy?.MaxParentalRating ?? null,
    MaxActiveSessions: policy?.MaxActiveSessions ?? 0,
    LoginAttemptsBeforeLockout: policy?.LoginAttemptsBeforeLockout ?? -1,
    RemoteClientBitrateLimit: policy?.RemoteClientBitrateLimit ?? 0,
    BlockedTags: [...(policy?.BlockedTags || [])],
    AllowedTags: [...(policy?.AllowedTags || [])],
    BlockUnratedItems: [...(policy?.BlockUnratedItems || [])],
    AccessSchedules: [...(policy?.AccessSchedules || [])]
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

async function savePolicy() {
  const user = selectedUser.value;
  if (!user) return;
  saving.value = true;
  error.value = '';
  try {
    const next: UserPolicy = {
      ...policyForm,
      BlockedTags: listFromText(tagText.value),
      EnabledDevices: listFromText(deviceText.value),
      EnabledFolders: policyForm.EnableAllFolders ? [] : [...(policyForm.EnabledFolders || [])],
      AccessSchedules: [...(policyForm.AccessSchedules || [])]
    };
    await api.updateUserPolicy(user.Id, next);
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
          <button type="button" :disabled="saving" @click="savePolicy">保存策略</button>
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
              </div>
              <div class="form-grid">
                <label>远程码率上限 bps<input v-model.number="policyForm.RemoteClientBitrateLimit" type="number" min="0" /></label>
                <label>最大活跃会话<input v-model.number="policyForm.MaxActiveSessions" type="number" min="0" /></label>
              </div>
            </section>

            <section class="settings-band">
              <h3>家长控制</h3>
              <div class="form-grid">
                <label>最高分级值<input v-model.number="policyForm.MaxParentalRating" type="number" min="0" /></label>
                <label>屏蔽标签<input v-model="tagText" placeholder="多个标签用逗号分隔" /></label>
              </div>
            </section>

            <section class="settings-band">
              <h3>设备与时间</h3>
              <label><input v-model="policyForm.EnableAllDevices" type="checkbox" /> 允许所有设备</label>
              <input v-if="!policyForm.EnableAllDevices" v-model="deviceText" placeholder="允许的 DeviceId，逗号分隔" />
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
