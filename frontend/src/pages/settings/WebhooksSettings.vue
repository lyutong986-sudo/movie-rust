<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { WebhookInfo, WebhookNotificationType } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(true);
const saving = ref(false);
const error = ref('');
const saved = ref('');
const hooks = ref<WebhookInfo[]>([]);
const eventTypes = ref<WebhookNotificationType[]>([]);

const editingId = ref<string | null>(null);

const draft = reactive({
  Name: '',
  Url: '',
  Enabled: true,
  subscribeAll: false,
  events: [] as string[],
  ContentType: 'application/json',
  Secret: ''
});

const groupedEvents = computed(() => {
  const map = new Map<string, WebhookNotificationType[]>();
  for (const t of eventTypes.value) {
    const cat = t.Category || 'General';
    if (!map.has(cat)) map.set(cat, []);
    map.get(cat)!.push(t);
  }
  return Array.from(map.entries()).sort(([a], [b]) => a.localeCompare(b));
});

function formatDate(value?: string | null) {
  if (!value) return '-';
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return '-';
  return d.toLocaleString('zh-CN');
}

function resetDraft() {
  editingId.value = null;
  draft.Name = '';
  draft.Url = '';
  draft.Enabled = true;
  draft.subscribeAll = false;
  draft.events = [];
  draft.ContentType = 'application/json';
  draft.Secret = '';
}

function startEdit(h: WebhookInfo) {
  editingId.value = h.Id;
  draft.Name = h.Name;
  draft.Url = h.Url;
  draft.Enabled = h.Enabled;
  draft.ContentType = h.ContentType?.trim() || 'application/json';
  draft.Secret = '';
  const ev = h.Events || [];
  draft.subscribeAll = ev.length === 0;
  draft.events = draft.subscribeAll ? [] : [...ev];
}

function isEventChecked(type: string) {
  return draft.events.includes(type);
}

function toggleEvent(type: string, checked: boolean) {
  const set = new Set(draft.events);
  if (checked) set.add(type);
  else set.delete(type);
  draft.events = [...set];
}

function toggleEventFromCheckbox(type: string, v: unknown) {
  toggleEvent(type, Boolean(v));
}

async function load() {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    const [list, types] = await Promise.all([api.listWebhooks(), api.notificationsWebhookTypes()]);
    hooks.value = list;
    eventTypes.value = types;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

async function save() {
  if (!draft.Name.trim() || !draft.Url.trim()) {
    error.value = '请填写名称与 URL';
    return;
  }
  saving.value = true;
  error.value = '';
  saved.value = '';
  const body = {
    Name: draft.Name.trim(),
    Url: draft.Url.trim(),
    Enabled: draft.Enabled,
    Events: draft.subscribeAll ? [] : [...draft.events],
    ContentType: draft.ContentType.trim() || 'application/json',
    Headers: {} as Record<string, unknown>,
    ...(draft.Secret.trim() ? { Secret: draft.Secret.trim() } : {})
  };
  try {
    if (editingId.value) {
      await api.updateWebhook(editingId.value, body);
      saved.value = '已保存 Webhook 配置';
    } else {
      await api.createWebhook(body);
      saved.value = '已新建 Webhook';
      resetDraft();
    }
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    saving.value = false;
  }
}

async function removeHook(h: WebhookInfo) {
  if (!window.confirm(`删除 Webhook「${h.Name}」？`)) return;
  error.value = '';
  saved.value = '';
  try {
    await api.deleteWebhook(h.Id);
    saved.value = '已删除';
    if (editingId.value === h.Id) resetDraft();
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

async function testHook(h: WebhookInfo) {
  error.value = '';
  saved.value = '';
  try {
    const r = await api.testWebhook(h.Id);
    saved.value = r?.Message || '测试事件已排队派发';
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

onMounted(load);

watch(
  () => draft.subscribeAll,
  (all) => {
    if (all) draft.events = [];
  }
);
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">Webhook 仅管理员可配置。</p>
    </div>

    <div v-else class="space-y-4">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs uppercase tracking-wider">Webhooks</p>
          <h2 class="text-highlighted text-xl font-semibold">出向 Webhook</h2>
          <p class="text-muted mt-1 max-w-3xl text-sm leading-relaxed">
            配置保存在服务端 PostgreSQL 表 <code class="text-xs">webhooks</code>。播放器上报进度后，Movie Rust 会按订阅向目标 URL POST JSON（含
            <code class="text-xs">Event</code>、<code class="text-xs">Date</code>、<code class="text-xs">Server</code>、<code class="text-xs">User</code>、<code class="text-xs">Item</code>、<code class="text-xs">PlaybackInfo</code>
            等字段），与 Emby Webhooks 插件格式一致。对接 Sakura_embyboss 时，播放相关可指向其
            <code class="text-xs">POST …/webhook/client-filter</code>（需订阅 <code class="text-xs">playback.start</code> /
            <code class="text-xs">playback.progress</code> / <code class="text-xs">playback.stop</code> /
            <code class="text-xs">session.start</code>）。媒体库/收藏另有 <code class="text-xs">/webhook/medias</code>、<code class="text-xs">/webhook/favorites</code>。
          </p>
          <p class="text-muted mt-2 text-xs">
            「订阅全部」= 事件列表留空（后端 <code class="text-xs">events</code> 为空数组表示匹配所有事件）。
          </p>
        </div>
        <div class="flex gap-2">
          <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-cw" :loading="loading" @click="load">刷新</UButton>
          <UButton v-if="editingId" color="neutral" variant="outline" icon="i-lucide-plus" @click="resetDraft">新建其它</UButton>
        </div>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">
            {{ editingId ? '编辑 Webhook' : '新建 Webhook' }}
          </h3>
        </template>
        <div class="grid gap-4">
          <div class="grid gap-3 sm:grid-cols-2">
            <UFormField label="名称">
              <UInput v-model="draft.Name" placeholder="例如：Sakura 播放回调" class="w-full" />
            </UFormField>
            <UFormField label="启用">
              <USwitch v-model="draft.Enabled" />
            </UFormField>
          </div>
          <UFormField label="目标 URL（http/https）">
            <UInput v-model="draft.Url" placeholder="https://your-sakura-bot/webhook/client-filter" class="w-full font-mono text-sm" />
          </UFormField>
          <div class="grid gap-3 sm:grid-cols-2">
            <UFormField label="Content-Type">
              <USelect
                v-model="draft.ContentType"
                :items="[
                  { label: 'application/json', value: 'application/json' },
                  { label: 'application/x-www-form-urlencoded（字段 data=JSON）', value: 'application/x-www-form-urlencoded' }
                ]"
                value-key="value"
                label-key="label"
                class="w-full"
              />
            </UFormField>
            <UFormField label="密钥（可选，HMAC SHA256）" hint="编辑时留空表示保留原密钥">
              <UInput v-model="draft.Secret" type="password" autocomplete="new-password" placeholder="与下游约定的签名密钥" class="w-full" />
            </UFormField>
          </div>

          <div class="border-default rounded-lg border p-3">
            <UCheckbox v-model="draft.subscribeAll" label="订阅全部事件（等同于 Events 留空）" />
            <p v-if="draft.subscribeAll" class="text-muted mt-2 text-xs">关闭后可按类别勾选下方事件。</p>
            <div v-else class="mt-3 space-y-3">
              <div v-for="[cat, items] of groupedEvents" :key="cat">
                <p class="text-highlighted mb-2 text-xs font-semibold uppercase tracking-wide">{{ cat }}</p>
                <div class="flex flex-wrap gap-x-4 gap-y-2">
                  <UCheckbox
                    v-for="t in items"
                    :key="t.Type"
                    :model-value="isEventChecked(t.Type)"
                    :label="t.Type"
                    @update:model-value="toggleEventFromCheckbox(t.Type, $event)"
                  />
                </div>
              </div>
            </div>
          </div>

          <div class="flex flex-wrap gap-2">
            <UButton icon="i-lucide-save" :loading="saving" @click="save">{{ editingId ? '保存修改' : '创建' }}</UButton>
            <UButton v-if="editingId" color="neutral" variant="ghost" icon="i-lucide-x" @click="resetDraft">取消编辑</UButton>
          </div>
        </div>
      </UCard>

      <UCard v-if="hooks.length" :ui="{ body: 'p-0' }">
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">已配置（{{ hooks.length }}）</h3>
        </template>
        <div class="divide-y divide-default">
          <div
            v-for="h in hooks"
            :key="h.Id"
            class="flex flex-col gap-3 p-4 lg:flex-row lg:items-start lg:justify-between"
          >
            <div class="min-w-0 flex-1">
              <p class="text-highlighted font-semibold">{{ h.Name }}</p>
              <p class="text-muted truncate font-mono text-xs">{{ h.Url }}</p>
              <p class="text-dimmed mt-1 text-[11px]">
                事件: {{ !h.Events?.length ? '全部' : h.Events.join(', ') }} · Content-Type: {{ h.ContentType || 'application/json' }} · 密钥:
                {{ h.HasSecret ? '已设置' : '无' }}
              </p>
              <p class="text-dimmed text-[11px]">
                上次触发: {{ formatDate(h.LastTriggeredAt) }} · HTTP {{ h.LastStatus ?? '—' }}
                <span v-if="h.LastError" class="text-error"> — {{ h.LastError }}</span>
              </p>
            </div>
            <div class="flex shrink-0 flex-wrap gap-2">
              <UButton size="sm" variant="subtle" icon="i-lucide-pencil" @click="startEdit(h)">编辑</UButton>
              <UButton size="sm" variant="subtle" icon="i-lucide-send" :disabled="!h.Enabled" @click="testHook(h)">测试</UButton>
              <UButton size="sm" color="error" variant="ghost" icon="i-lucide-trash-2" @click="removeHook(h)">删除</UButton>
            </div>
          </div>
        </div>
      </UCard>

      <UCard v-else-if="!loading">
        <p class="text-muted text-sm">暂无 Webhook。多数 Sakura 场景至少需要一条指向 Bot 公网地址的回调。</p>
      </UCard>
    </div>
  </SettingsLayout>
</template>
