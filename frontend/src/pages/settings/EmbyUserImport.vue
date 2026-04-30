<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import initSqlJs, { type Database } from 'sql.js';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import { api, adminUsers, isAdmin, libraries, loadAdminData } from '../../store/app';
import type {
  ImportEmbyUserItem,
  ImportEmbyUsersResponse,
  UserPolicy,
} from '../../api/emby';

interface ParsedUser {
  embyId: number;
  guid: string;
  name: string;
  passwordHex: string;
  hasPassword: boolean;
  isAdmin: boolean;
  selected: boolean;
  conflict: 'none' | 'name';
}

const router = useRouter();

const fileInput = ref<HTMLInputElement | null>(null);
const file = ref<File | null>(null);
const loading = ref(false);
const parsing = ref(false);
const importing = ref(false);
const error = ref('');
const search = ref('');
const conflictPolicy = ref<'skip' | 'overwrite'>('skip');
const importResult = ref<ImportEmbyUsersResponse | null>(null);

const parsedUsers = ref<ParsedUser[]>([]);

const defaultPolicy = reactive<Partial<UserPolicy>>({
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
  EnableAllFolders: true,
  EnabledFolders: [],
  EnableUserPreferenceAccess: true,
  EnableSharedDeviceControl: false,
  EnableRemoteControlOfOtherUsers: false,
  RemoteClientBitrateLimit: 0,
  SimultaneousStreamLimit: 0,
});

const folderOptions = computed(() =>
  libraries.value.map(lib => ({ label: lib.Name || lib.CollectionType || lib.Id, value: lib.Id }))
);

const filteredUsers = computed(() => {
  const q = search.value.trim().toLowerCase();
  if (!q) return parsedUsers.value;
  return parsedUsers.value.filter(u => u.name.toLowerCase().includes(q));
});

const selectedCount = computed(() => parsedUsers.value.filter(u => u.selected).length);
const conflictCount = computed(() =>
  parsedUsers.value.filter(u => u.selected && u.conflict !== 'none').length
);

watch(
  () => defaultPolicy.EnableAllFolders,
  v => {
    if (v) defaultPolicy.EnabledFolders = [];
  }
);

async function ensureAdmin() {
  if (!adminUsers.value.length) await loadAdminData();
}

function pickFile() {
  fileInput.value?.click();
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

function bytesToGuid(bytes: Uint8Array): string {
  if (bytes.length !== 16) return bytesToHex(bytes);
  const hex = bytesToHex(bytes);
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20),
  ].join('-');
}

async function loadDatabase(buffer: ArrayBuffer): Promise<Database> {
  const SQL = await initSqlJs({
    locateFile: (filename: string) => `/sql-wasm/${filename}`,
  });
  return new SQL.Database(new Uint8Array(buffer));
}

async function parseFile(target: File) {
  parsing.value = true;
  error.value = '';
  parsedUsers.value = [];
  importResult.value = null;
  try {
    const buf = await target.arrayBuffer();
    const db = await loadDatabase(buf);
    let res;
    try {
      res = db.exec('SELECT Id, guid, data FROM LocalUsersv2');
    } catch (e) {
      throw new Error('未找到 LocalUsersv2 表，请确认这是 Emby users.db');
    }
    if (!res.length) {
      throw new Error('LocalUsersv2 表为空');
    }
    const rows = res[0].values;
    const existing = new Set(adminUsers.value.map(u => (u.Name || '').toLowerCase()));
    const items: ParsedUser[] = [];
    for (const row of rows) {
      const id = Number(row[0]);
      const guidRaw = row[1] as Uint8Array | null;
      const text = String(row[2] || '');
      if (!text) continue;
      let parsed: Record<string, unknown>;
      try {
        parsed = JSON.parse(text);
      } catch {
        continue;
      }
      const name = String(parsed['Name'] || '').trim();
      if (!name) continue;
      const password = String(parsed['Password'] || '').trim();
      const isAdminFlag = Boolean((parsed['Policy'] as Record<string, unknown> | undefined)?.IsAdministrator);
      items.push({
        embyId: id,
        guid: guidRaw ? bytesToGuid(guidRaw) : '',
        name,
        passwordHex: password,
        hasPassword: password.length > 0,
        isAdmin: isAdminFlag,
        selected: password.length > 0,
        conflict: existing.has(name.toLowerCase()) ? 'name' : 'none',
      });
    }
    items.sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'));
    parsedUsers.value = items;
    db.close();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    parsing.value = false;
  }
}

async function handleFileChange(ev: Event) {
  const target = ev.target as HTMLInputElement;
  const f = target.files?.[0];
  if (!f) return;
  file.value = f;
  await parseFile(f);
}

function selectAll(value: boolean) {
  for (const u of filteredUsers.value) u.selected = value;
}

function selectOnlyNew() {
  for (const u of parsedUsers.value) {
    u.selected = u.hasPassword && u.conflict === 'none';
  }
}

function selectOnlyHasPassword() {
  for (const u of parsedUsers.value) {
    u.selected = u.hasPassword;
  }
}

function buildItem(user: ParsedUser): ImportEmbyUserItem {
  return {
    Name: user.name,
    LegacyPasswordHash: user.passwordHex,
    LegacyPasswordFormat: 'emby_sha1',
    ExternalId: user.guid || String(user.embyId),
  };
}

async function doImport() {
  await ensureAdmin();
  importResult.value = null;
  error.value = '';
  const targets = parsedUsers.value.filter(u => u.selected);
  if (!targets.length) {
    error.value = '请先勾选至少一个要导入的用户';
    return;
  }
  importing.value = true;
  try {
    const policySnapshot: Partial<UserPolicy> = JSON.parse(JSON.stringify(defaultPolicy));
    if (policySnapshot.EnableAllFolders) {
      policySnapshot.EnabledFolders = [];
    }
    const payload = {
      Users: targets.map(buildItem),
      ConflictPolicy: conflictPolicy.value,
      DefaultPolicy: policySnapshot,
      DefaultLegacyFormat: 'emby_sha1',
    };
    importResult.value = await api.importEmbyUsers(payload);
    await loadAdminData();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    importing.value = false;
  }
}

function reset() {
  file.value = null;
  parsedUsers.value = [];
  importResult.value = null;
  error.value = '';
  if (fileInput.value) fileInput.value.value = '';
}

ensureAdmin();
</script>

<template>
  <SettingsLayout>
    <div
      v-if="!isAdmin"
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
    >
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
    </div>

    <div v-else class="space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">Users · Import</p>
          <h2 class="text-highlighted text-xl font-semibold">从 Emby 导入用户</h2>
          <p class="text-muted mt-1 max-w-2xl text-sm leading-relaxed">
            上传 Emby 的 <code class="px-1">users.db</code>，浏览器内用 sql.js 解析
            <code class="px-1">LocalUsersv2</code> 表的用户名 + 密码哈希（SHA1 hex），
            勾选后批量灌入本项目；用户用原 Emby 密码即可登录，登录成功后会自动升级为
            Argon2，旧 SHA1 字段被清空。
          </p>
        </div>
        <UButton color="neutral" variant="ghost" icon="i-lucide-arrow-left" @click="router.push('/settings/users')">
          返回用户管理
        </UButton>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">1. 选择 users.db 文件</h3>
        </template>
        <div class="flex flex-wrap items-center gap-3">
          <input ref="fileInput" type="file" accept=".db,.sqlite" class="hidden" @change="handleFileChange" />
          <UButton icon="i-lucide-upload" :loading="parsing" @click="pickFile">选择文件</UButton>
          <span v-if="file" class="text-sm text-muted">
            {{ file.name }} · {{ (file.size / 1024).toFixed(1) }} KB
          </span>
          <UButton v-if="file" color="neutral" variant="ghost" icon="i-lucide-x" @click="reset">移除</UButton>
        </div>
      </UCard>

      <UCard v-if="parsedUsers.length">
        <template #header>
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h3 class="text-highlighted text-sm font-semibold">2. 默认权限模板（Default Policy）</h3>
            <span class="text-muted text-xs">所有勾选用户都会套用，可在导入后单独调整。</span>
          </div>
        </template>
        <div class="grid gap-3 sm:grid-cols-2 md:grid-cols-3">
          <UCheckbox v-model="defaultPolicy.IsAdministrator" label="管理员" />
          <UCheckbox v-model="defaultPolicy.IsDisabled" label="禁用账户" />
          <UCheckbox v-model="defaultPolicy.IsHidden" label="登录页隐藏" />
          <UCheckbox v-model="defaultPolicy.EnableRemoteAccess" label="允许远程访问" />
          <UCheckbox v-model="defaultPolicy.EnableMediaPlayback" label="允许播放" />
          <UCheckbox v-model="defaultPolicy.EnableContentDownloading" label="允许下载" />
          <UCheckbox v-model="defaultPolicy.EnableContentDeletion" label="允许删除内容" />
          <UCheckbox v-model="defaultPolicy.EnableAudioPlaybackTranscoding" label="允许音频转码" />
          <UCheckbox v-model="defaultPolicy.EnableVideoPlaybackTranscoding" label="允许视频转码" />
          <UCheckbox v-model="defaultPolicy.EnablePlaybackRemuxing" label="允许 Remux" />
          <UCheckbox v-model="defaultPolicy.EnableUserPreferenceAccess" label="允许修改自己设置" />
          <UCheckbox v-model="defaultPolicy.EnableAllFolders" label="访问全部媒体库" />
        </div>
        <div v-if="!defaultPolicy.EnableAllFolders" class="mt-3">
          <UFormField label="允许的媒体库（多选）">
            <USelectMenu
              v-model="defaultPolicy.EnabledFolders"
              :items="folderOptions"
              multiple
              value-key="value"
              class="w-full"
            />
          </UFormField>
        </div>
        <div class="mt-4 grid gap-3 sm:grid-cols-2">
          <UFormField label="远程码率上限 (bps)" hint="0 = 不限制">
            <UInput
              v-model.number="defaultPolicy.RemoteClientBitrateLimit"
              type="number"
              :min="0"
              :step="1000000"
              placeholder="0"
            />
            <template #description>
              <span class="text-xs text-muted">
                常用值：8Mbps = 8000000 · 20Mbps = 20000000 · 100Mbps = 100000000
              </span>
            </template>
          </UFormField>
          <UFormField label="最大活跃会话数" hint="0 = 不限制">
            <UInput
              v-model.number="defaultPolicy.SimultaneousStreamLimit"
              type="number"
              :min="0"
              :step="1"
              placeholder="0"
            />
            <template #description>
              <span class="text-xs text-muted">
                限制用户同时播放的设备数量
              </span>
            </template>
          </UFormField>
        </div>
      </UCard>

      <UCard v-if="parsedUsers.length">
        <template #header>
          <div class="flex flex-wrap items-center justify-between gap-2">
            <h3 class="text-highlighted text-sm font-semibold">
              3. 选择要导入的用户（{{ selectedCount }} / {{ parsedUsers.length }}）
            </h3>
            <div class="flex flex-wrap items-center gap-2">
              <UInput v-model="search" icon="i-lucide-search" placeholder="按用户名筛选" class="w-48" />
              <UButton size="xs" variant="soft" @click="selectAll(true)">全选</UButton>
              <UButton size="xs" variant="soft" @click="selectAll(false)">全不选</UButton>
              <UButton size="xs" variant="soft" @click="selectOnlyHasPassword">仅有密码</UButton>
              <UButton size="xs" variant="soft" @click="selectOnlyNew">仅未导入</UButton>
            </div>
          </div>
        </template>
        <div class="max-h-[480px] overflow-auto">
          <table class="w-full text-sm">
            <thead class="text-muted">
              <tr class="border-b border-default">
                <th class="w-10 py-2"></th>
                <th class="py-2 text-left">用户名</th>
                <th class="py-2 text-left">Emby Id</th>
                <th class="py-2 text-left">SHA1（前 12 位）</th>
                <th class="py-2 text-left">本地冲突</th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="user in filteredUsers"
                :key="user.guid || user.embyId"
                class="border-b border-default/40 hover:bg-elevated/30"
              >
                <td class="py-1.5 text-center">
                  <UCheckbox v-model="user.selected" />
                </td>
                <td class="py-1.5">
                  <span class="text-highlighted">{{ user.name }}</span>
                  <UBadge v-if="user.isAdmin" color="primary" variant="soft" class="ml-2" size="xs">原 admin</UBadge>
                </td>
                <td class="py-1.5 font-mono text-xs text-muted">{{ user.embyId }}</td>
                <td class="py-1.5 font-mono text-xs">
                  <span v-if="user.hasPassword">{{ user.passwordHex.slice(0, 12) }}…</span>
                  <span v-else class="text-muted italic">空</span>
                </td>
                <td class="py-1.5">
                  <UBadge
                    v-if="user.conflict === 'name'"
                    color="warning"
                    variant="soft"
                    size="xs"
                  >
                    本地已有同名
                  </UBadge>
                  <span v-else class="text-muted text-xs">-</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </UCard>

      <UCard v-if="parsedUsers.length">
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">4. 重名策略 + 执行导入</h3>
        </template>
        <div class="flex flex-wrap items-end gap-3">
          <UFormField label="本地存在同名用户时" class="min-w-48">
            <USelect
              v-model="conflictPolicy"
              :items="[
                { label: '跳过（推荐）', value: 'skip' },
                { label: '覆盖 legacy 哈希', value: 'overwrite' },
              ]"
              class="w-56"
            />
          </UFormField>
          <p v-if="conflictCount" class="text-warning text-xs">
            {{ conflictCount }} 个勾选用户与本地重名，将按上述策略处理
          </p>
          <div class="ml-auto flex gap-2">
            <UButton
              icon="i-lucide-download"
              :loading="importing"
              :disabled="!selectedCount"
              @click="doImport"
            >
              导入 {{ selectedCount }} 个用户
            </UButton>
          </div>
        </div>
      </UCard>

      <UCard v-if="importResult">
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">导入结果</h3>
        </template>
        <div class="grid gap-3 text-sm sm:grid-cols-4">
          <div class="rounded-lg border border-default bg-elevated/30 p-3">
            <p class="text-muted text-xs">新建</p>
            <p class="text-2xl font-semibold text-primary">{{ importResult.Created.length }}</p>
          </div>
          <div class="rounded-lg border border-default bg-elevated/30 p-3">
            <p class="text-muted text-xs">已更新</p>
            <p class="text-2xl font-semibold">{{ importResult.Updated.length }}</p>
          </div>
          <div class="rounded-lg border border-default bg-elevated/30 p-3">
            <p class="text-muted text-xs">跳过</p>
            <p class="text-2xl font-semibold text-muted">{{ importResult.Skipped.length }}</p>
          </div>
          <div class="rounded-lg border border-default bg-elevated/30 p-3">
            <p class="text-muted text-xs">失败</p>
            <p class="text-2xl font-semibold text-error">{{ importResult.Failed.length }}</p>
          </div>
        </div>
        <div v-if="importResult.Failed.length" class="mt-3">
          <p class="text-error text-xs font-semibold">失败明细：</p>
          <ul class="text-xs">
            <li v-for="f in importResult.Failed" :key="f.Name + f.Error" class="font-mono">
              {{ f.Name }} — {{ f.Error }}
            </li>
          </ul>
        </div>
      </UCard>
    </div>
  </SettingsLayout>
</template>
