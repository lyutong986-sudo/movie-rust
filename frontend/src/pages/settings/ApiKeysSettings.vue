<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { ApiKeyInfo } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(true);
const creating = ref(false);
const error = ref('');
const saved = ref('');
const keys = ref<ApiKeyInfo[]>([]);
const newAppName = ref('');
const newExpiresDays = ref<number | null>(null);
const revealedToken = ref<Record<string, boolean>>({});

const tokenDisplayBinding = computed(() => (token: string) => {
  if (revealedToken.value[token]) return token;
  if (!token) return '';
  return '•'.repeat(Math.min(token.length, 32));
});

function formatDate(value?: string | null) {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '-';
  return date.toLocaleString();
}

async function load() {
  if (!isAdmin.value) return;
  loading.value = true;
  error.value = '';
  try {
    const resp = await api.listAuthKeys();
    keys.value = resp.Items;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

async function createKey() {
  if (!newAppName.value.trim()) {
    error.value = '请输入 App 名称';
    return;
  }
  creating.value = true;
  error.value = '';
  saved.value = '';
  try {
    const created = await api.createAuthKey({
      app: newAppName.value.trim(),
      expiresInDays: newExpiresDays.value || undefined
    });
    saved.value = `已创建 API Key：${created.AppName}`;
    newAppName.value = '';
    newExpiresDays.value = null;
    revealedToken.value[created.AccessToken] = true;
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    creating.value = false;
  }
}

async function revoke(key: ApiKeyInfo) {
  if (!window.confirm(`确认撤销 API Key「${key.AppName}」？此操作不可恢复。`)) {
    return;
  }
  error.value = '';
  saved.value = '';
  try {
    await api.deleteAuthKey(key.AccessToken);
    saved.value = `已撤销 API Key：${key.AppName}`;
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

async function copyToken(token: string) {
  try {
    await navigator.clipboard.writeText(token);
    saved.value = '已复制 API Key 到剪贴板';
  } catch {
    error.value = '无法访问剪贴板';
  }
}

onMounted(load);
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能查看或管理 API Key。</p>
    </div>

    <div v-else class="space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">API Keys</p>
          <h2 class="text-highlighted text-xl font-semibold">API Key 管理</h2>
        </div>
        <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-cw" :loading="loading" @click="load">
          刷新
        </UButton>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">颁发新 Key</h3>
        </template>
        <div class="grid gap-3 sm:grid-cols-3">
          <UFormField label="App 名称">
            <UInput v-model="newAppName" placeholder="例如：Infuse、Mobile App" class="w-full" />
          </UFormField>
          <UFormField label="有效期（天）" hint="留空则永久有效">
            <UInput v-model.number="newExpiresDays" type="number" :min="1" class="w-full" />
          </UFormField>
          <div class="flex items-end">
            <UButton icon="i-lucide-key" :loading="creating" class="w-full justify-center" @click="createKey">
              颁发 API Key
            </UButton>
          </div>
        </div>
      </UCard>

      <UCard v-if="keys.length" :ui="{ body: 'p-0' }">
        <div class="divide-y divide-default">
          <div
            v-for="key in keys"
            :key="key.AccessToken"
            class="grid grid-cols-1 gap-3 p-4 lg:grid-cols-[1.4fr_2fr_1.2fr_auto] lg:items-center"
          >
            <div class="min-w-0">
              <p class="text-highlighted truncate text-sm font-semibold">{{ key.AppName }}</p>
              <p class="text-muted truncate text-xs">{{ key.UserName }} · {{ key.AppVersion || '-' }}</p>
              <p class="text-dimmed truncate text-[11px]">
                设备: {{ key.DeviceName || '-' }} · {{ key.DeviceId || '-' }}
              </p>
            </div>
            <div class="flex items-center gap-2">
              <code class="flex-1 truncate rounded-md border border-default bg-elevated/40 px-3 py-1.5 font-mono text-xs">
                {{ tokenDisplayBinding(key.AccessToken) }}
              </code>
              <UButton
                :icon="revealedToken[key.AccessToken] ? 'i-lucide-eye-off' : 'i-lucide-eye'"
                color="neutral"
                variant="subtle"
                size="xs"
                @click="revealedToken[key.AccessToken] = !revealedToken[key.AccessToken]"
              />
              <UButton
                icon="i-lucide-copy"
                color="neutral"
                variant="subtle"
                size="xs"
                @click="copyToken(key.AccessToken)"
              />
            </div>
            <div class="text-muted text-xs">
              <p>颁发: {{ formatDate(key.DateLastActivity) }}</p>
              <p>过期: {{ key.ExpirationDate ? formatDate(key.ExpirationDate) : '永久' }}</p>
              <UBadge :color="key.IsActive !== false ? 'success' : 'neutral'" variant="subtle" size="xs" class="mt-1">
                {{ key.IsActive !== false ? '生效中' : '已过期' }}
              </UBadge>
            </div>
            <UButton
              color="error"
              variant="soft"
              icon="i-lucide-trash-2"
              size="sm"
              @click="revoke(key)"
            >
              撤销
            </UButton>
          </div>
        </div>
      </UCard>

      <div
        v-else-if="!loading"
        class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
      >
        <UIcon name="i-lucide-key" class="size-10 text-muted" />
        <p class="text-muted text-sm">还没有任何 API Key，使用上方表单颁发一个新的。</p>
      </div>
    </div>
  </SettingsLayout>
</template>
