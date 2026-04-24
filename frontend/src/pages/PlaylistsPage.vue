<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import type { PlaylistInfo } from '../api/emby';
import { api } from '../store/app';
import EmptyState from '../components/EmptyState.vue';

const router = useRouter();
const loading = ref(true);
const error = ref('');
const saved = ref('');
const playlists = ref<PlaylistInfo[]>([]);
const creating = ref(false);
const newName = ref('');
const newOverview = ref('');

async function load() {
  loading.value = true;
  error.value = '';
  try {
    const result = await api.listPlaylists();
    playlists.value = result.Items;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

function formatTime(value?: string) {
  if (!value) return '-';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '-';
  return date.toLocaleString();
}

async function createPlaylist() {
  if (!newName.value.trim()) {
    error.value = '请输入播放列表名称';
    return;
  }
  creating.value = true;
  error.value = '';
  saved.value = '';
  try {
    const result = await api.createPlaylist({
      Name: newName.value.trim(),
      MediaType: 'Video',
      Overview: newOverview.value.trim() || undefined
    });
    saved.value = `已创建播放列表：${result.Name}`;
    newName.value = '';
    newOverview.value = '';
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    creating.value = false;
  }
}

async function removePlaylist(playlist: PlaylistInfo) {
  if (!window.confirm(`删除播放列表「${playlist.Name}」？条目不会被删除。`)) {
    return;
  }
  error.value = '';
  saved.value = '';
  try {
    await api.deletePlaylist(playlist.Id);
    saved.value = `已删除播放列表：${playlist.Name}`;
    await load();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  }
}

function openPlaylist(playlist: PlaylistInfo) {
  router.push(`/playlist/${playlist.Id}`);
}

onMounted(load);
</script>

<template>
  <div class="min-h-0 w-full min-w-0 flex-1 space-y-4">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div>
        <p class="text-muted text-xs uppercase tracking-wider">Playlists</p>
        <h2 class="text-highlighted text-2xl font-semibold">播放列表</h2>
      </div>
      <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-cw" :loading="loading" @click="load">
        刷新
      </UButton>
    </div>

    <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
    <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

    <UCard>
      <template #header>
        <h3 class="text-highlighted text-sm font-semibold">新建播放列表</h3>
      </template>
      <div class="grid gap-3 sm:grid-cols-3">
        <UFormField label="名称">
          <UInput v-model="newName" class="w-full" />
        </UFormField>
        <UFormField label="简介（可选）" class="sm:col-span-2">
          <UInput v-model="newOverview" class="w-full" />
        </UFormField>
      </div>
      <template #footer>
        <div class="flex justify-end">
          <UButton icon="i-lucide-list-plus" :loading="creating" @click="createPlaylist">
            创建播放列表
          </UButton>
        </div>
      </template>
    </UCard>

    <div v-if="playlists.length" class="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
      <UCard v-for="playlist in playlists" :key="playlist.Id">
        <template #header>
          <div class="flex items-start justify-between gap-2">
            <div class="min-w-0">
              <button
                type="button"
                class="text-highlighted truncate text-base font-semibold hover:text-primary"
                @click="openPlaylist(playlist)"
              >
                {{ playlist.Name }}
              </button>
              <p class="text-muted mt-1 text-xs">
                {{ playlist.ChildCount }} 个条目 · 更新 {{ formatTime(playlist.DateModified) }}
              </p>
            </div>
            <UButton
              color="error"
              variant="soft"
              size="xs"
              icon="i-lucide-trash-2"
              @click="removePlaylist(playlist)"
            />
          </div>
        </template>
        <p v-if="playlist.Overview" class="text-muted line-clamp-3 text-xs">{{ playlist.Overview }}</p>
        <p v-else class="text-dimmed text-xs">未设置描述</p>
        <template #footer>
          <div class="flex items-center justify-between">
            <span class="text-muted text-xs">创建于 {{ formatTime(playlist.DateCreated) }}</span>
            <UButton
              variant="subtle"
              color="neutral"
              size="xs"
              trailing-icon="i-lucide-chevron-right"
              @click="openPlaylist(playlist)"
            >
              打开
            </UButton>
          </div>
        </template>
      </UCard>
    </div>

    <EmptyState
      v-else-if="!loading"
      icon="i-lucide-list-music"
      title="还没有播放列表"
      description="使用上方表单创建一个，然后在条目详情页添加视频/剧集到列表中。"
    />
  </div>
</template>
