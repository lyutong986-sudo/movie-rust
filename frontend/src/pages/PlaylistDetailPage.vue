<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { BaseItemDto, PlaylistInfo } from '../api/emby';
import { api } from '../store/app';
import { playbackRoute } from '../utils/navigation';
import EmptyState from '../components/EmptyState.vue';

const route = useRoute();
const router = useRouter();
const playlistId = computed(() => String(route.params.id || ''));

const loading = ref(true);
const saving = ref(false);
const error = ref('');
const saved = ref('');
const playlist = ref<PlaylistInfo | null>(null);
const items = ref<BaseItemDto[]>([]);

const form = ref({ Name: '', Overview: '' });

async function load() {
  loading.value = true;
  error.value = '';
  try {
    const detail = await api.getPlaylist(playlistId.value);
    playlist.value = detail;
    form.value = {
      Name: detail.Name,
      Overview: detail.Overview || ''
    };
    const result = await api.listPlaylistItems(playlistId.value, { Limit: 500 });
    items.value = result.Items;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

async function save() {
  if (!playlist.value) return;
  saving.value = true;
  error.value = '';
  saved.value = '';
  try {
    playlist.value = await api.updatePlaylist(playlist.value.Id, {
      Name: form.value.Name.trim(),
      Overview: form.value.Overview.trim() || null
    });
    saved.value = '播放列表已更新';
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    saving.value = false;
  }
}

async function removeItem(item: BaseItemDto) {
  if (!playlist.value || !item.PlaylistItemId) return;
  error.value = '';
  saved.value = '';
  try {
    await api.removePlaylistItems(playlist.value.Id, [item.PlaylistItemId]);
    items.value = items.value.filter((value) => value.PlaylistItemId !== item.PlaylistItemId);
    saved.value = `已移除：${item.Name}`;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

async function moveItem(item: BaseItemDto, direction: -1 | 1) {
  if (!playlist.value || !item.PlaylistItemId) return;
  const current = items.value.findIndex((value) => value.PlaylistItemId === item.PlaylistItemId);
  if (current < 0) return;
  const target = current + direction;
  if (target < 0 || target >= items.value.length) return;
  try {
    await api.movePlaylistItem(playlist.value.Id, item.PlaylistItemId, target);
    const next = [...items.value];
    next.splice(current, 1);
    next.splice(target, 0, item);
    items.value = next;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

function openItem(item: BaseItemDto) {
  router.push(`/item/${item.Id}`);
}

function playItem(item: BaseItemDto) {
  router.push(playbackRoute(item));
}

async function removePlaylistEntirely() {
  if (!playlist.value) return;
  if (!window.confirm(`删除播放列表「${playlist.value.Name}」？`)) return;
  try {
    await api.deletePlaylist(playlist.value.Id);
    router.replace('/playlists');
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

onMounted(load);
watch(playlistId, load);
</script>

<template>
  <div class="min-h-0 w-full min-w-0 flex-1 space-y-4">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div>
        <button
          type="button"
          class="text-muted hover:text-primary mb-1 flex items-center gap-1 text-xs"
          @click="router.push('/playlists')"
        >
          <UIcon name="i-lucide-arrow-left" class="size-3" /> 返回播放列表
        </button>
        <h2 class="text-highlighted text-2xl font-semibold">
          {{ playlist?.Name || '播放列表' }}
        </h2>
        <p v-if="playlist" class="text-muted text-xs">
          {{ items.length }} 个条目 · 更新 {{ new Date(playlist.DateModified).toLocaleString() }}
        </p>
      </div>
      <div class="flex gap-2">
        <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-cw" :loading="loading" @click="load">
          刷新
        </UButton>
        <UButton color="error" variant="soft" icon="i-lucide-trash-2" @click="removePlaylistEntirely">
          删除列表
        </UButton>
      </div>
    </div>

    <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
    <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

    <UCard v-if="playlist">
      <template #header>
        <h3 class="text-highlighted text-sm font-semibold">基本信息</h3>
      </template>
      <div class="grid gap-4 sm:grid-cols-2">
        <UFormField label="名称">
          <UInput v-model="form.Name" class="w-full" />
        </UFormField>
        <UFormField label="简介" class="sm:col-span-2">
          <UTextarea v-model="form.Overview" :rows="3" class="w-full" />
        </UFormField>
      </div>
      <template #footer>
        <div class="flex justify-end">
          <UButton :loading="saving" icon="i-lucide-save" @click="save">保存修改</UButton>
        </div>
      </template>
    </UCard>

    <UCard v-if="items.length" :ui="{ body: 'p-0' }">
      <template #header>
        <h3 class="text-highlighted text-sm font-semibold">条目</h3>
      </template>
      <div class="divide-default divide-y">
        <div
          v-for="(item, index) in items"
          :key="item.PlaylistItemId || item.Id"
          class="grid grid-cols-[auto_1fr_auto] items-center gap-3 p-3"
        >
          <div class="flex flex-col gap-1">
            <UButton
              color="neutral"
              variant="subtle"
              size="xs"
              icon="i-lucide-chevron-up"
              :disabled="index === 0"
              @click="moveItem(item, -1)"
            />
            <UButton
              color="neutral"
              variant="subtle"
              size="xs"
              icon="i-lucide-chevron-down"
              :disabled="index === items.length - 1"
              @click="moveItem(item, 1)"
            />
          </div>
          <div class="min-w-0 cursor-pointer" @click="openItem(item)">
            <p class="text-highlighted truncate text-sm font-medium">{{ item.Name }}</p>
            <p class="text-muted truncate text-xs">
              {{ item.Type }} · {{ item.SeriesName || item.ProductionYear || '' }}
            </p>
          </div>
          <div class="flex items-center gap-2">
            <UButton color="primary" variant="soft" size="xs" icon="i-lucide-play" @click="playItem(item)">
              播放
            </UButton>
            <UButton color="error" variant="soft" size="xs" icon="i-lucide-trash-2" @click="removeItem(item)">
              移除
            </UButton>
          </div>
        </div>
      </div>
    </UCard>

    <EmptyState
      v-else-if="!loading"
      icon="i-lucide-list"
      title="播放列表是空的"
      description="去详情页或搜索页，将视频/剧集加入该列表。"
    />
  </div>
</template>
