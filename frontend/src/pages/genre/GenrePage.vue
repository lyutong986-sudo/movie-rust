<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import type { BaseItemDto } from '../../api/emby';
import { api } from '../../store/app';
import { itemRoute, playbackRoute } from '../../utils/navigation';

const route = useRoute();
const router = useRouter();

const loading = ref(false);
const error = ref('');
const items = ref<BaseItemDto[]>([]);

const genreName = computed(() => {
  const raw = String(route.params.id || '');
  try {
    return decodeURIComponent(raw);
  } catch {
    return raw;
  }
});

const selectedType = computed(() => {
  const value = route.query.type;
  return typeof value === 'string' && value ? value : '';
});

watch(
  () => [route.params.id, route.query.type],
  async () => {
    await loadGenre();
  },
  { immediate: true }
);

async function loadGenre() {
  if (!genreName.value) {
    items.value = [];
    return;
  }

  loading.value = true;
  error.value = '';

  try {
    const response = await api.items(undefined, '', true, {
      genres: [genreName.value],
      includeTypes: selectedType.value ? [selectedType.value] : undefined,
      sortBy: 'SortName',
      sortOrder: 'Ascending',
      limit: 240
    });
    items.value = response.Items ?? [];
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : String(loadError);
    items.value = [];
  } finally {
    loading.value = false;
  }
}

async function openItem(item: BaseItemDto) {
  await router.push(itemRoute(item));
}

async function playItem(item: BaseItemDto) {
  await router.push(playbackRoute(item));
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <nav class="flex items-center gap-2 text-sm">
      <UButton
        color="neutral"
        variant="ghost"
        size="xs"
        icon="i-lucide-arrow-left"
        @click="router.back()"
      >
        返回
      </UButton>
      <UIcon name="i-lucide-chevron-right" class="size-3 text-muted" />
      <span class="text-muted">类型</span>
      <UIcon name="i-lucide-chevron-right" class="size-3 text-muted" />
      <span class="text-highlighted font-medium">{{ genreName }}</span>
    </nav>

    <div class="flex items-center gap-3">
      <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10 text-primary">
        <UIcon name="i-lucide-tag" class="size-5" />
      </div>
      <div>
        <h2 class="text-highlighted text-lg font-semibold">{{ genreName }}</h2>
        <p class="text-muted text-xs">{{ selectedType || '全部类型' }} · {{ items.length }} 项</p>
      </div>
    </div>

    <div v-if="loading" class="flex min-h-[40vh] flex-col items-center justify-center gap-2">
      <UProgress animation="carousel" class="w-48" />
      <p class="text-muted text-sm">正在按类型筛选媒体项目…</p>
    </div>
    <UAlert
      v-else-if="error"
      color="error"
      variant="subtle"
      icon="i-lucide-triangle-alert"
      title="加载失败"
      :description="error"
    />
    <div
      v-else-if="items.length"
      class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-7"
    >
      <MediaCard
        v-for="item in items"
        :key="item.Id"
        :item="item"
        @play="playItem"
        @select="openItem"
      />
    </div>
    <div
      v-else
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default bg-elevated/20 p-10 text-center"
    >
      <UIcon name="i-lucide-tag" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">这个类型下还没有内容</h3>
      <p class="text-muted max-w-md text-sm">
        等更多媒体被扫描并写入类型字段后，这里会自动丰富起来。
      </p>
    </div>
  </div>
</template>
