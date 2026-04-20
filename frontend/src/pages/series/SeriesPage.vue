<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import type { BaseItemDto } from '../../api/emby';
import { api } from '../../store/app';
import { genreRoute, itemRoute, playbackRoute } from '../../utils/navigation';

interface SeasonSection {
  season: BaseItemDto;
  episodes: BaseItemDto[];
}

const route = useRoute();
const router = useRouter();

const loading = ref(false);
const error = ref('');
const series = ref<BaseItemDto | null>(null);
const seasons = ref<SeasonSection[]>([]);
const relatedItems = ref<BaseItemDto[]>([]);
const activeSeasonId = ref('');

const heroImage = computed(() =>
  series.value ? api.backdropUrl(series.value) || api.itemImageUrl(series.value) : ''
);
const activeSeason = computed(
  () => seasons.value.find((entry) => entry.season.Id === activeSeasonId.value) || seasons.value[0] || null
);
const firstEpisode = computed(() => seasons.value.flatMap((entry) => entry.episodes)[0] || null);
const metaChips = computed(() => {
  if (!series.value) {
    return [];
  }

  return [
    'Series',
    series.value.ProductionYear ? String(series.value.ProductionYear) : '',
    seasons.value.length ? `${seasons.value.length} 季` : '',
    firstEpisode.value?.RunTimeTicks ? runtimeText(firstEpisode.value) : ''
  ].filter(Boolean);
});

watch(
  () => route.params.id,
  async (value) => {
    if (typeof value === 'string' && value) {
      await loadSeries(value);
    }
  },
  { immediate: true }
);

async function loadSeries(itemId: string) {
  loading.value = true;
  error.value = '';

  try {
    const currentSeries = await api.item(itemId);
    if (currentSeries.Type !== 'Series') {
      await router.replace(itemRoute(currentSeries));
      return;
    }

    series.value = currentSeries;

    const seasonItems = (
      await api.items(currentSeries.Id, '', false, {
        includeTypes: ['Season'],
        sortBy: 'IndexNumber',
        sortOrder: 'Ascending',
        limit: 100
      })
    ).Items;

    seasons.value = await Promise.all(
      seasonItems.map(async (season) => ({
        season,
        episodes: (
          await api.items(season.Id, '', false, {
            includeTypes: ['Episode'],
            sortBy: 'IndexNumber',
            sortOrder: 'Ascending',
            limit: 200
          })
        ).Items
      }))
    );

    activeSeasonId.value = seasons.value[0]?.season.Id || '';

    relatedItems.value = (
      await api.items(undefined, '', true, {
        includeTypes: ['Series'],
        sortBy: 'DateCreated',
        sortOrder: 'Descending',
        limit: 36
      })
    ).Items.filter((candidate) => candidate.Id !== currentSeries.Id);
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : String(loadError);
    series.value = null;
    seasons.value = [];
    relatedItems.value = [];
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

async function playSeries() {
  if (firstEpisode.value) {
    await playItem(firstEpisode.value);
  }
}

async function openGenre(name: string) {
  if (!series.value) {
    return;
  }

  await router.push(genreRoute(name, 'Series'));
}

function runtimeText(item: BaseItemDto) {
  if (!item.RunTimeTicks) {
    return '';
  }

  const totalMinutes = Math.round(item.RunTimeTicks / 10_000_000 / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  return hours ? `${hours} 小时 ${minutes} 分钟` : `${minutes} 分钟`;
}
</script>

<template>
  <section v-if="loading" class="empty">
    <p>剧集详情</p>
    <h2>正在加载</h2>
    <p>正在读取季和分集结构。</p>
  </section>

  <section v-else-if="error" class="empty">
    <p>剧集详情</p>
    <h2>加载失败</h2>
    <p>{{ error }}</p>
  </section>

  <section v-else-if="series" class="home-sections">
    <nav class="crumbs">
      <button type="button" title="返回上一页" @click="router.back()">←</button>
      <span>Series</span>
      <span>{{ series.Name }}</span>
    </nav>

    <section class="detail-hero">
      <img v-if="heroImage" :src="heroImage" :alt="series.Name" />
      <div v-else class="poster-fallback folder">{{ series.Name.slice(0, 1).toUpperCase() }}</div>

      <div class="detail-copy">
        <div>
          <p>剧集</p>
          <h2>{{ series.Name }}</h2>
        </div>

        <div class="meta">
          <span v-for="chip in metaChips" :key="chip">{{ chip }}</span>
        </div>

        <p v-if="series.Overview">{{ series.Overview }}</p>

        <div class="button-row">
          <button v-if="firstEpisode" type="button" @click="playSeries">播放</button>
          <button type="button" class="secondary" @click="router.back()">返回</button>
        </div>
      </div>
    </section>

    <section v-if="series.Genres?.length" class="media-row">
      <div class="section-heading">
        <h3>类型</h3>
      </div>
      <div class="meta meta-links">
        <button
          v-for="genre in series.Genres"
          :key="genre"
          type="button"
          class="chip-button secondary"
          @click="openGenre(genre)"
        >
          {{ genre }}
        </button>
      </div>
    </section>

    <section v-if="seasons.length" class="media-row">
      <div class="section-heading">
        <h3>季</h3>
        <span>{{ seasons.length }} 季</span>
      </div>
      <div class="admin-tabs">
        <button
          v-for="entry in seasons"
          :key="entry.season.Id"
          type="button"
          :class="{ active: activeSeasonId === entry.season.Id }"
          @click="activeSeasonId = entry.season.Id"
        >
          {{ entry.season.Name }}
        </button>
      </div>

      <div v-if="activeSeason" class="episode-list">
        <article v-for="episode in activeSeason.episodes" :key="episode.Id" class="episode-row">
          <div class="episode-number">
            {{ episode.IndexNumber || '—' }}
          </div>
          <div class="episode-copy">
            <div class="episode-heading">
              <strong>{{ episode.Name }}</strong>
              <span>{{ runtimeText(episode) }}</span>
            </div>
            <p>{{ episode.Overview || '暂无剧情简介。' }}</p>
            <div class="button-row">
              <button type="button" @click="playItem(episode)">播放</button>
              <button class="secondary" type="button" @click="openItem(episode)">详情</button>
            </div>
          </div>
        </article>
      </div>
    </section>

    <section v-if="relatedItems.length" class="media-row">
      <div class="section-heading">
        <h3>更多剧集</h3>
        <span>{{ relatedItems.length }} 项</span>
      </div>
      <div class="rail poster-rail">
        <MediaCard
          v-for="item in relatedItems"
          :key="item.Id"
          :item="item"
          @play="playItem"
          @select="openItem"
        />
      </div>
    </section>
  </section>
</template>
