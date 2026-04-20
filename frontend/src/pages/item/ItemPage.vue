<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import type { BaseItemDto, MediaStreamDto } from '../../api/emby';
import { api, fileSize } from '../../store/app';

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
const streamUrl = computed(() =>
  item.value && !item.value.IsFolder && item.value.MediaSources?.length ? api.streamUrl(item.value) : ''
);
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

    item.value = currentItem;
    currentSourceIndex.value = 0;

    childItems.value = currentItem.IsFolder
      ? (await api.items(currentItem.Id, '', false, { limit: 60 })).Items
      : [];

    relatedItems.value = currentItem.ParentId
      ? (await api.items(currentItem.ParentId, '', false, { limit: 24 })).Items.filter(
          (candidate) => candidate.Id !== currentItem.Id
        )
      : [];
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
  if (target.Type === 'CollectionFolder') {
    await router.push(`/library/${target.Id}`);
    return;
  }

  await router.push(`/item/${target.Id}`);
}

function playItem(target: BaseItemDto) {
  window.open(api.streamUrl(target), '_blank', 'noopener');
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

function streamLabel(type: string) {
  if (type === 'Video') return '视频';
  if (type === 'Audio') return '音频';
  if (type === 'Subtitle') return '字幕';
  return type;
}

function streamText(stream: MediaStreamDto) {
  return [
    stream.DisplayTitle,
    stream.Codec,
    stream.Language,
    stream.Width && stream.Height ? `${stream.Width}x${stream.Height}` : '',
    stream.IsExternal ? '外挂' : ''
  ]
    .filter(Boolean)
    .join(' · ');
}

function runtimeText(target: BaseItemDto) {
  if (!target.RunTimeTicks) {
    return '';
  }

  const totalMinutes = Math.round(target.RunTimeTicks / 10_000_000 / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours) {
    return `${hours} 小时 ${minutes} 分`;
  }
  return `${minutes} 分钟`;
}
</script>

<template>
  <section v-if="loading" class="empty">
    <p>媒体详情</p>
    <h2>正在加载</h2>
    <p>正在读取项目元数据、播放源和子条目。</p>
  </section>

  <section v-else-if="error" class="empty">
    <p>媒体详情</p>
    <h2>加载失败</h2>
    <p>{{ error }}</p>
  </section>

  <section v-else-if="item" class="home-sections">
    <nav class="crumbs">
      <button type="button" title="返回上一页" @click="router.back()">‹</button>
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
          <a
            v-if="streamUrl"
            class="play-link"
            :href="streamUrl"
            target="_blank"
            rel="noreferrer"
          >
            播放
          </a>
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

    <video v-if="streamUrl" controls :src="streamUrl"></video>

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
          <span>{{ streamText(stream) || '默认轨道' }}</span>
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
      <div class="meta">
        <span v-for="genre in item.Genres" :key="genre">{{ genre }}</span>
      </div>
    </section>

    <section v-if="childItems.length" class="media-row">
      <div class="section-heading">
        <h3>内容</h3>
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
        <h3>更多内容</h3>
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
