<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../components/MediaCard.vue';
import { api } from '../store/app';
import type { BaseItemDto } from '../api/emby';
import { itemRoute, playbackRoute } from '../utils/navigation';

const route = useRoute();
const router = useRouter();

const loading = ref(false);
const error = ref('');
const results = ref<BaseItemDto[]>([]);
const tab = ref('0');

const searchQuery = computed(() => String(route.query.q || '').trim());
const groupedResults = computed(() => [
  {
    label: '电影',
    items: results.value.filter((item) => item.Type === 'Movie')
  },
  {
    label: '剧集',
    items: results.value.filter((item) =>
      ['Series', 'Season', 'Episode', 'Folder', 'BoxSet'].includes(item.Type)
    )
  },
  {
    label: '专辑',
    items: results.value.filter((item) => item.Type === 'MusicAlbum')
  },
  {
    label: '歌曲',
    items: results.value.filter((item) => item.Type === 'Audio')
  },
  {
    label: '书籍',
    items: results.value.filter((item) => item.Type === 'Book')
  },
  {
    label: '人物',
    items: results.value.filter((item) => item.Type === 'Person')
  },
  {
    label: '艺术家',
    items: results.value.filter((item) => item.Type === 'MusicArtist')
  }
]);

const tabItems = computed(() =>
  groupedResults.value.map((group, index) => ({
    value: String(index),
    label: group.label,
    slot: 'count' as const,
    count: group.items.length,
    disabled: !group.items.length
  }))
);

const activeItems = computed(() => groupedResults.value[Number(tab.value) || 0]?.items || []);

watch(
  () => route.query.tab,
  (value) => {
    tab.value = String(value || '0');
  },
  { immediate: true }
);

watch(tab, async (value) => {
  if (String(route.query.tab || '0') !== value) {
    await router.replace({ query: { ...route.query, tab: value } });
  }
});

watch(
  () => route.query.q,
  async () => {
    const query = searchQuery.value;
    if (!query) {
      results.value = [];
      error.value = '';
      return;
    }

    loading.value = true;
    error.value = '';

    try {
      const response = await api.items(undefined, query, true, {
        sortBy: 'SortName',
        sortOrder: 'Ascending',
        limit: 200
      });
      results.value = response.Items;
      const firstNonEmpty = groupedResults.value.findIndex((group) => group.items.length);
      if (firstNonEmpty >= 0) {
        tab.value = String(firstNonEmpty);
      }
    } catch (loadError) {
      error.value = loadError instanceof Error ? loadError.message : String(loadError);
      results.value = [];
    } finally {
      loading.value = false;
    }
  },
  { immediate: true }
);

async function openItem(item: BaseItemDto) {
  await router.push(itemRoute(item));
}

async function playItem(item: BaseItemDto) {
  await router.push(playbackRoute(item));
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <div class="flex items-baseline justify-between">
      <h2 class="text-highlighted text-lg font-semibold">搜索结果</h2>
      <span v-if="searchQuery" class="text-muted text-sm">
        "{{ searchQuery }}" 共 {{ results.length }} 项
      </span>
    </div>

    <UTabs
      v-if="searchQuery"
      v-model="tab"
      :items="tabItems"
      variant="link"
      :content="false"
    >
      <template #count="{ item }">
        <span class="ms-1 text-dimmed text-xs">({{ item.count }})</span>
      </template>
    </UTabs>

    <div v-if="loading" class="py-20 text-center">
      <UProgress animation="carousel" class="mx-auto mb-3 w-48" />
      <p class="text-muted text-sm">正在搜索 "{{ searchQuery }}"…</p>
    </div>
    <UAlert
      v-else-if="error"
      color="error"
      variant="subtle"
      icon="i-lucide-triangle-alert"
      title="搜索失败"
      :description="error"
    />
    <div
      v-else-if="searchQuery && activeItems.length"
      class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-7"
    >
      <MediaCard
        v-for="item in activeItems"
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
      <UIcon
        :name="searchQuery ? 'i-lucide-search-x' : 'i-lucide-search'"
        class="size-10 text-muted"
      />
      <h3 class="text-highlighted text-lg font-semibold">
        {{ searchQuery ? '没有找到匹配内容' : '输入关键字开始搜索' }}
      </h3>
      <p class="text-muted max-w-md text-sm">
        这里对应 Jellyfin 的独立搜索页，而不是在首页上临时过滤。
      </p>
    </div>
  </div>
</template>
