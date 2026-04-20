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
    items.value = response.Items;
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
  <section class="home-sections">
    <nav class="crumbs">
      <button type="button" title="返回上一页" @click="router.back()">←</button>
      <span>类型</span>
      <span>{{ genreName }}</span>
    </nav>

    <section class="media-row">
      <div class="section-heading">
        <div>
          <h3>{{ genreName }}</h3>
          <span>{{ selectedType || '全部类型' }}</span>
        </div>
      </div>

      <div v-if="loading" class="empty">
        <p>{{ genreName }}</p>
        <h2>正在整理内容</h2>
        <p>正在按类型筛选媒体项目。</p>
      </div>
      <div v-else-if="error" class="empty">
        <p>{{ genreName }}</p>
        <h2>加载失败</h2>
        <p>{{ error }}</p>
      </div>
      <div v-else-if="items.length" class="poster-grid">
        <MediaCard
          v-for="item in items"
          :key="item.Id"
          :item="item"
          @play="playItem"
          @select="openItem"
        />
      </div>
      <div v-else class="empty">
        <p>{{ genreName }}</p>
        <h2>这个类型下还没有内容</h2>
        <p>等更多媒体被扫描并写入类型字段后，这里会自动丰富起来。</p>
      </div>
    </section>
  </section>
</template>
