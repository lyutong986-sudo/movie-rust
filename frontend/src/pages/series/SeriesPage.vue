<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaRow from '../../components/MediaRow.vue';
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
const similarItems = ref<BaseItemDto[]>([]);
const activeSeasonId = ref('');
const nextUpEpisode = ref<BaseItemDto | null>(null);

const backdrop = computed(() => (series.value ? api.backdropUrl(series.value) : ''));
const poster = computed(() =>
  series.value ? api.itemImageUrl(series.value) || api.backdropUrl(series.value) : ''
);
const logo = computed(() => (series.value ? api.logoUrl(series.value) : ''));
const activeSeason = computed(
  () =>
    seasons.value.find((entry) => entry.season.Id === activeSeasonId.value) ||
    seasons.value[0] ||
    null
);
const firstEpisode = computed(() => seasons.value.flatMap((entry) => entry.episodes)[0] || null);
// 上次观看的集：取所有集中 PlaybackPositionTicks>0 且最近 LastPlayedDate 的那一集。
const lastPlayedEpisode = computed(() => {
  const all = seasons.value.flatMap((s) => s.episodes);
  const candidates = all
    .filter((ep) => (ep.UserData?.PlaybackPositionTicks || 0) > 0)
    .sort((a, b) => {
      const ta = Date.parse(a.UserData?.LastPlayedDate || '') || 0;
      const tb = Date.parse(b.UserData?.LastPlayedDate || '') || 0;
      return tb - ta;
    });
  return candidates[0] || null;
});
const startEpisode = computed(
  () => nextUpEpisode.value || lastPlayedEpisode.value || firstEpisode.value
);
const metaChips = computed(() => {
  if (!series.value) return [];
  return [
    '剧集',
    series.value.ProductionYear ? String(series.value.ProductionYear) : '',
    seasons.value.length ? `${seasons.value.length} 季` : '',
    firstEpisode.value?.RunTimeTicks ? runtimeText(firstEpisode.value) : '',
    series.value.OfficialRating || ''
  ].filter(Boolean);
});

const seasonTabs = computed(() =>
  seasons.value.map((entry) => ({ value: entry.season.Id, label: entry.season.Name || '未命名' }))
);

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

    // 取得下一集
    try {
      const nu = await api.nextUp(currentSeries.Id, 1);
      nextUpEpisode.value = nu.Items?.[0] || null;
    } catch {
      nextUpEpisode.value = null;
    }

    // 默认选中含 nextUp / 最近播放 集对应的季
    const targetEp = nextUpEpisode.value || lastPlayedEpisode.value;
    activeSeasonId.value = targetEp?.SeasonId || seasons.value[0]?.season.Id || '';

    relatedItems.value = (
      await api.items(undefined, '', true, {
        includeTypes: ['Series'],
        sortBy: 'DateCreated',
        sortOrder: 'Descending',
        limit: 36
      })
    ).Items.filter((candidate) => candidate.Id !== currentSeries.Id);

    try {
      similarItems.value = (await api.similar(currentSeries.Id, 20)).Items || [];
    } catch {
      similarItems.value = [];
    }
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
  const target = startEpisode.value;
  if (target) {
    await playItem(target);
  }
}

function episodeLabel(ep: BaseItemDto) {
  const season = ep.ParentIndexNumber ? `S${String(ep.ParentIndexNumber).padStart(2, '0')}` : '';
  const episode = ep.IndexNumber ? `E${String(ep.IndexNumber).padStart(2, '0')}` : '';
  return `${season}${episode}`;
}

async function openGenre(name: string) {
  if (!series.value) return;
  await router.push(genreRoute(name, 'Series'));
}

function runtimeText(item: BaseItemDto) {
  if (!item.RunTimeTicks) return '';
  const totalMinutes = Math.round(item.RunTimeTicks / 10_000_000 / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return hours ? `${hours} 小时 ${minutes} 分钟` : `${minutes} 分钟`;
}

function episodeThumb(episode: BaseItemDto) {
  return api.itemImageUrl(episode) || api.backdropUrl(episode);
}
</script>

<template>
  <div v-if="loading" class="flex min-h-[50vh] flex-col items-center justify-center gap-2">
    <UProgress animation="carousel" class="w-48" />
    <p class="text-muted text-sm">正在读取剧集结构…</p>
  </div>

  <UAlert
    v-else-if="error"
    color="error"
    variant="subtle"
    icon="i-lucide-triangle-alert"
    title="加载失败"
    :description="error"
  />

  <div v-else-if="series" class="flex flex-col gap-10">
    <!-- Hero -->
    <section class="relative overflow-hidden rounded-2xl ring-1 ring-default">
      <img
        v-if="backdrop"
        :src="backdrop"
        :alt="series.Name"
        class="absolute inset-0 h-full w-full object-cover opacity-40 blur-sm"
      />
      <div class="absolute inset-0 bg-gradient-to-br from-(--ui-bg)/80 via-(--ui-bg)/70 to-(--ui-bg)/95" />

      <div class="relative grid gap-6 p-5 sm:p-8 lg:grid-cols-[220px_1fr] lg:gap-10">
        <div class="aspect-[2/3] w-44 overflow-hidden rounded-xl bg-elevated ring-1 ring-default lg:w-full">
          <img
            v-if="poster"
            :src="poster"
            :alt="series.Name"
            class="h-full w-full object-cover"
          />
          <div
            v-else
            class="flex h-full w-full items-center justify-center bg-gradient-to-br from-primary/30 to-primary/5 text-3xl font-bold text-primary"
          >
            {{ series.Name.slice(0, 1).toUpperCase() }}
          </div>
        </div>

        <div class="flex flex-col gap-4">
          <div>
            <p class="text-muted text-xs uppercase tracking-wider">剧集</p>
            <img v-if="logo" :src="logo" :alt="series.Name" class="mt-1 max-h-16 w-auto" />
            <h1 v-else class="text-highlighted display-font mt-1 text-2xl font-bold sm:text-3xl">
              {{ series.Name }}
            </h1>
          </div>

          <div class="flex flex-wrap items-center gap-2">
            <UBadge
              v-for="chip in metaChips"
              :key="chip"
              color="neutral"
              variant="soft"
            >
              {{ chip }}
            </UBadge>
            <UBadge
              v-if="series.CommunityRating"
              color="warning"
              variant="soft"
              icon="i-lucide-star"
            >
              {{ Number(series.CommunityRating).toFixed(1) }}
            </UBadge>
          </div>

          <p
            v-if="series.Tagline || series.Taglines?.[0]"
            class="text-primary/90 max-w-3xl text-sm italic"
          >
            “{{ series.Tagline || series.Taglines?.[0] }}”
          </p>

          <p v-if="series.Overview" class="text-default max-w-3xl text-sm leading-relaxed">
            {{ series.Overview }}
          </p>

          <div class="flex flex-wrap gap-2">
            <UButton v-if="startEpisode" icon="i-lucide-play" size="lg" @click="playSeries">
              <template v-if="lastPlayedEpisode">继续观看 {{ episodeLabel(startEpisode) }}</template>
              <template v-else-if="nextUpEpisode">播放下一集 {{ episodeLabel(startEpisode) }}</template>
              <template v-else>
                {{ startEpisode.IndexNumber ? `从 ${episodeLabel(startEpisode)} 开始播放` : '播放' }}
              </template>
            </UButton>
          </div>
        </div>
      </div>
    </section>

    <!-- 类型 -->
    <section v-if="series.Genres?.length" class="space-y-3">
      <h3 class="text-highlighted text-sm font-semibold">类型</h3>
      <div class="flex flex-wrap gap-2">
        <UButton
          v-for="genre in series.Genres"
          :key="genre"
          color="neutral"
          variant="outline"
          size="sm"
          @click="openGenre(genre)"
        >
          {{ genre }}
        </UButton>
      </div>
    </section>

    <!-- 季 + 分集 -->
    <section v-if="seasons.length" class="space-y-3">
      <div class="flex items-baseline justify-between">
        <h3 class="text-highlighted text-base font-semibold">分集</h3>
        <span class="text-muted text-sm">{{ seasons.length }} 季</span>
      </div>

      <UTabs
        v-model="activeSeasonId"
        :items="seasonTabs"
        variant="pill"
        size="xs"
        :content="false"
      />

      <div v-if="activeSeason" class="flex flex-col gap-3">
        <article
          v-for="episode in activeSeason.episodes"
          :key="episode.Id"
          class="group flex flex-col gap-3 rounded-xl border border-default bg-elevated/20 p-3 transition hover:bg-elevated/40 sm:flex-row"
        >
          <button
            type="button"
            class="group/thumb relative block aspect-video w-full shrink-0 overflow-hidden rounded-lg bg-elevated ring-1 ring-default sm:w-56"
            @click="playItem(episode)"
          >
            <img
              v-if="episodeThumb(episode)"
              :src="episodeThumb(episode)"
              :alt="episode.Name"
              class="h-full w-full object-cover transition-transform group-hover/thumb:scale-105"
            />
            <div
              v-else
              class="flex h-full w-full items-center justify-center bg-gradient-to-br from-primary/20 to-primary/5 text-primary"
            >
              <UIcon name="i-lucide-film" class="size-6" />
            </div>
            <div class="absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 transition-opacity group-hover/thumb:opacity-100">
              <span
                class="flex h-11 w-11 items-center justify-center rounded-full bg-primary text-primary-contrast shadow-lg"
              >
                <UIcon name="i-lucide-play" class="size-5" />
              </span>
            </div>
          </button>

          <div class="flex min-w-0 flex-1 flex-col gap-2">
            <div class="flex flex-wrap items-baseline gap-2">
              <span class="text-primary font-mono text-sm">
                {{ episode.IndexNumber ? String(episode.IndexNumber).padStart(2, '0') : '—' }}
              </span>
              <strong class="text-highlighted text-sm">{{ episode.Name }}</strong>
              <span v-if="episode.RunTimeTicks" class="text-muted ms-auto text-xs">
                {{ runtimeText(episode) }}
              </span>
            </div>
            <p class="text-muted line-clamp-2 text-xs">
              {{ episode.Overview || '暂无剧情简介。' }}
            </p>
            <div class="flex flex-wrap gap-2">
              <UButton size="xs" icon="i-lucide-play" @click="playItem(episode)">播放</UButton>
              <UButton
                size="xs"
                color="neutral"
                variant="subtle"
                icon="i-lucide-info"
                @click="openItem(episode)"
              >
                详情
              </UButton>
            </div>
          </div>
        </article>
      </div>
    </section>

    <!-- 演职人员 -->
    <section v-if="series.People?.length" class="space-y-3">
      <div class="flex items-baseline justify-between">
        <h3 class="text-highlighted text-base font-semibold">演职人员</h3>
        <span class="text-muted text-sm">{{ series.People.length }} 人</span>
      </div>
      <div
        class="flex snap-x snap-mandatory gap-3 overflow-x-auto pb-2
               [-ms-overflow-style:none] [scrollbar-width:thin]
               [&::-webkit-scrollbar]:h-1.5
               [&::-webkit-scrollbar-thumb]:rounded-full
               [&::-webkit-scrollbar-thumb]:bg-default"
      >
        <div
          v-for="(person, index) in series.People"
          :key="`${person.Id || person.Name}-${index}`"
          class="flex w-28 shrink-0 snap-start flex-col items-center gap-2 rounded-lg border border-default bg-elevated/20 p-3 text-center"
        >
          <UAvatar
            :alt="person.Name"
            :src="api.personImageUrl(person) || undefined"
            :text="(person.Name || '?').slice(0, 1).toUpperCase()"
            size="xl"
          />
          <div class="min-w-0">
            <p class="text-highlighted truncate text-xs font-medium" :title="person.Name">
              {{ person.Name }}
            </p>
            <p v-if="person.Role || person.Type" class="text-muted truncate text-[10px]" :title="person.Role || person.Type">
              {{ person.Role || person.Type }}
            </p>
          </div>
        </div>
      </div>
    </section>

    <!-- 相似 -->
    <MediaRow
      v-if="similarItems.length"
      title="类似剧集"
      icon="i-lucide-sparkles"
      :items="similarItems"
      @play="playItem"
      @select="openItem"
    />

    <!-- 相关 -->
    <MediaRow
      v-if="relatedItems.length"
      title="更多剧集"
      icon="i-lucide-tv"
      :items="relatedItems"
      @play="playItem"
      @select="openItem"
    />
  </div>
</template>
