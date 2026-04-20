<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import type { BaseItemDto, MediaStreamDto } from '../../api/emby';
import { api, fileSize, streamLabel, streamText } from '../../store/app';
import { genreRoute, itemRoute, playbackRoute } from '../../utils/navigation';

const route = useRoute();
const router = useRouter();

const loading = ref(false);
const error = ref('');
const item = ref<BaseItemDto | null>(null);
const childItems = ref<BaseItemDto[]>([]);
const relatedItems = ref<BaseItemDto[]>([]);
const currentSourceIndex = ref(0);

const currentSource = computed(() => item.value?.MediaSources?.[currentSourceIndex.value]);
const currentStreams = computed(() => currentSource.value?.MediaStreams || item.value?.MediaStreams || []);
const itemImage = computed(() => (item.value ? api.itemImageUrl(item.value) || api.backdropUrl(item.value) : ''));
const playable = computed(() => Boolean(item.value && isPlayableItem(item.value)));
const metaChips = computed(() => {
  if (!item.value) {
    return [];
  }

  return [
    item.value.Type,
    item.value.ProductionYear ? String(item.value.ProductionYear) : '',
    item.value.Container || '',
    item.value.MediaType || '',
    runtimeText(item.value)
  ].filter(Boolean);
});

watch(
  () => route.params.id,
  async (value) => {
    if (typeof value === 'string' && value) {
      await loadItem(value);
    }
  },
  { immediate: true }
);

async function loadItem(itemId: string) {
  loading.value = true;
  error.value = '';

  try {
    const currentItem = await api.item(itemId);
    if (currentItem.Type === 'CollectionFolder') {
      await router.replace(`/library/${currentItem.Id}`);
      return;
    }

    if (currentItem.Type === 'Series') {
      await router.replace(`/series/${currentItem.Id}`);
      return;
    }

    item.value = currentItem;
    currentSourceIndex.value = 0;

    childItems.value = currentItem.IsFolder
      ? (
          await api.items(currentItem.Id, '', false, {
            sortBy: currentItem.Type === 'Season' ? 'IndexNumber' : 'SortName',
            sortOrder: 'Ascending',
            limit: 120
          })
        ).Items
      : [];

    if (currentItem.ParentId) {
      relatedItems.value = (
        await api.items(currentItem.ParentId, '', false, {
          sortBy: currentItem.Type === 'Episode' ? 'IndexNumber' : 'SortName',
          sortOrder: 'Ascending',
          limit: 60
        })
      ).Items.filter((candidate) => candidate.Id !== currentItem.Id);
    } else if (currentItem.Type !== 'Movie') {
      relatedItems.value = [];
    } else {
      relatedItems.value = (
        await api.items(undefined, '', true, {
          includeTypes: ['Movie'],
          sortBy: 'DateCreated',
          sortOrder: 'Descending',
          limit: 36
        })
      ).Items.filter((candidate) => candidate.Id !== currentItem.Id);
    }
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : String(loadError);
    item.value = null;
    childItems.value = [];
    relatedItems.value = [];
  } finally {
    loading.value = false;
  }
}

async function openChild(target: BaseItemDto) {
  await router.push(itemRoute(target));
}

async function playItem(target: BaseItemDto) {
  await router.push(playbackRoute(target));
}

function isPlayableItem(target: BaseItemDto) {
  if (target.IsFolder) {
    return false;
  }

  return ['Movie', 'Episode', 'Video', 'Audio', 'MusicVideo'].includes(target.Type) || ['Video', 'Audio'].includes(target.MediaType || '');
}

async function toggleFavorite() {
  if (!item.value) {
    return;
  }

  const userData = await api.markFavorite(item.value.Id, !item.value.UserData.IsFavorite);
  item.value = {
    ...item.value,
    UserData: {
      ...item.value.UserData,
      ...userData
    }
  };
}

async function togglePlayed() {
  if (!item.value) {
    return;
  }

  const userData = await api.markPlayed(item.value.Id, !item.value.UserData.Played);
  item.value = {
    ...item.value,
    UserData: {
      ...item.value.UserData,
      ...userData
    }
  };
}

async function openGenre(name: string) {
  if (!item.value) {
    return;
  }

  await router.push(genreRoute(name, item.value.Type));
}

function runtimeText(target: BaseItemDto) {
  if (!target.RunTimeTicks) {
    return '';
  }

  const totalMinutes = Math.round(target.RunTimeTicks / 10_000_000 / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  return hours ? `${hours} 小时 ${minutes} 分钟` : `${minutes} 分钟`;
}

function streamLine(stream: MediaStreamDto) {
  return streamText(stream) || '默认轨道';
}
</script>

<template>
  <section v-if="loading" class="empty">
    <p>媒体详情</p>
    <h2>正在加载</h2>
    <p>正在读取条目元数据、播放源和子项目。</p>
  </section>

  <section v-else-if="error" class="empty">
    <p>媒体详情</p>
    <h2>加载失败</h2>
    <p>{{ error }}</p>
  </section>

  <section v-else-if="item" class="home-sections">
    <nav class="crumbs">
      <button type="button" title="返回上一页" @click="router.back()">←</button>
      <span>{{ item.Type }}</span>
      <span>{{ item.Name }}</span>
    </nav>

    <section class="detail-hero">
      <img v-if="itemImage" :src="itemImage" :alt="item.Name" />
      <div v-else class="poster-fallback" :class="{ folder: item.IsFolder }">
        {{ item.IsFolder ? '目录' : item.Name.slice(0, 1).toUpperCase() }}
      </div>

      <div class="detail-copy">
        <div>
          <p>{{ item.SeriesName || item.SeasonName || item.Type }}</p>
          <h2>{{ item.Name }}</h2>
        </div>

        <div class="meta">
          <span v-for="chip in metaChips" :key="chip">{{ chip }}</span>
        </div>

        <p v-if="item.Overview">{{ item.Overview }}</p>

        <div class="button-row">
          <button v-if="playable" type="button" @click="playItem(item)">播放</button>
          <button type="button" class="secondary" @click="toggleFavorite">
            {{ item.UserData.IsFavorite ? '取消收藏' : '加入收藏' }}
          </button>
          <button type="button" class="secondary" @click="togglePlayed">
            {{ item.UserData.Played ? '标记未看' : '标记已看' }}
          </button>
        </div>

        <p v-if="item.Path" class="path">{{ item.Path }}</p>
      </div>
    </section>

    <section v-if="item.MediaSources?.length" class="settings-page">
      <div v-if="item.MediaSources.length > 1" class="admin-tabs">
        <button
          v-for="(source, index) in item.MediaSources"
          :key="source.Id"
          type="button"
          :class="{ active: currentSourceIndex === index }"
          @click="currentSourceIndex = index"
        >
          {{ source.Container || `版本 ${index + 1}` }}
        </button>
      </div>

      <div class="streams">
        <div v-for="stream in currentStreams" :key="`${stream.Type}-${stream.Index}`">
          <strong>{{ streamLabel(stream.Type) }}</strong>
          <span>{{ streamLine(stream) }}</span>
        </div>
        <div v-if="currentSource?.Size">
          <strong>文件大小</strong>
          <span>{{ fileSize(currentSource.Size) }}</span>
        </div>
      </div>
    </section>

    <section v-if="item.Genres?.length" class="media-row">
      <div class="section-heading">
        <h3>类型</h3>
      </div>
      <div class="meta meta-links">
        <button
          v-for="genre in item.Genres"
          :key="genre"
          type="button"
          class="chip-button secondary"
          @click="openGenre(genre)"
        >
          {{ genre }}
        </button>
      </div>
    </section>

    <section v-if="childItems.length" class="media-row">
      <div class="section-heading">
        <h3>{{ item.Type === 'Season' ? '分集' : '内容' }}</h3>
        <span>{{ childItems.length }} 项</span>
      </div>
      <div class="poster-grid">
        <MediaCard
          v-for="child in childItems"
          :key="child.Id"
          :item="child"
          @play="playItem"
          @select="openChild"
        />
      </div>
    </section>

    <section v-if="relatedItems.length" class="media-row">
      <div class="section-heading">
        <h3>{{ item.Type === 'Episode' ? '同季内容' : '更多内容' }}</h3>
        <span>{{ relatedItems.length }} 项</span>
      </div>
      <div class="rail poster-rail">
        <MediaCard
          v-for="related in relatedItems"
          :key="related.Id"
          :item="related"
          @play="playItem"
          @select="openChild"
        />
      </div>
    </section>
  </section>
</template>
