<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../components/MediaCard.vue';
import { api } from '../store/app';
import type { BaseItemDto } from '../api/emby';

const route = useRoute();
const router = useRouter();

const loading = ref(false);
const error = ref('');
const results = ref<BaseItemDto[]>([]);
const tab = ref(0);

const searchQuery = computed(() => String(route.query.q || '').trim());
const groupedResults = computed(() => [
  {
    label: '电影',
    items: results.value.filter((item) => item.Type === 'Movie')
  },
  {
    label: '剧集',
    items: results.value.filter((item) => ['Series', 'Season', 'Episode', 'Folder', 'BoxSet'].includes(item.Type))
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
const activeItems = computed(() => groupedResults.value[tab.value]?.items || []);

watch(
  () => route.query.tab,
  (value) => {
    const parsed = Number(value || 0);
    tab.value = Number.isFinite(parsed) ? parsed : 0;
  },
  { immediate: true }
);

watch(tab, async (value) => {
  if (Number(route.query.tab || 0) !== value) {
    await router.replace({
      query: {
        ...route.query,
        tab: String(value)
      }
    });
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
        tab.value = firstNonEmpty;
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
  if (item.Type === 'CollectionFolder') {
    await router.push(`/library/${item.Id}`);
    return;
  }

  await router.push(`/item/${item.Id}`);
}

function playItem(item: BaseItemDto) {
  window.open(api.streamUrl(item), '_blank', 'noopener');
}
</script>

<template>
  <section class="home-sections">
    <section class="media-row">
      <div class="section-heading">
        <div>
          <h3>搜索结果</h3>
          <span v-if="searchQuery">“{{ searchQuery }}” 共 {{ results.length }} 项</span>
        </div>
      </div>

      <div v-if="searchQuery" class="admin-tabs">
        <button
          v-for="(group, index) in groupedResults"
          :key="group.label"
          type="button"
          :class="{ active: tab === index }"
          :disabled="!group.items.length"
          @click="tab = index"
        >
          {{ group.label }}
        </button>
      </div>

      <div v-if="loading" class="empty">
        <p>{{ searchQuery }}</p>
        <h2>正在搜索</h2>
        <p>正在按 Jellyfin 前端的分类逻辑整理搜索结果。</p>
      </div>
      <div v-else-if="error" class="empty">
        <p>搜索失败</p>
        <h2>请求没有成功</h2>
        <p>{{ error }}</p>
      </div>
      <div v-else-if="searchQuery && activeItems.length" class="poster-grid">
        <MediaCard
          v-for="item in activeItems"
          :key="item.Id"
          :item="item"
          @play="playItem"
          @select="openItem"
        />
      </div>
      <div v-else class="empty">
        <p>全局搜索</p>
        <h2>{{ searchQuery ? '没有找到匹配内容' : '输入关键字开始搜索' }}</h2>
        <p>这里对应 Jellyfin 的独立搜索页，而不是在首页上临时过滤。</p>
      </div>
    </section>
  </section>
</template>
