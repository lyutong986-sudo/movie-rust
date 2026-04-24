<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import MediaRow from '../../components/MediaRow.vue';
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
const currentStreams = computed(
  () => currentSource.value?.MediaStreams || item.value?.MediaStreams || []
);
const backdrop = computed(() => (item.value ? api.backdropUrl(item.value) : ''));
const poster = computed(() =>
  item.value ? api.itemImageUrl(item.value) || api.backdropUrl(item.value) : ''
);
const playable = computed(
  () => Boolean(item.value && !item.value.IsFolder && item.value.MediaSources?.length)
);
const metaChips = computed(() => {
  if (!item.value) return [];
  return [
    item.value.Type,
    item.value.ProductionYear ? String(item.value.ProductionYear) : '',
    item.value.Container || '',
    item.value.MediaType || '',
    runtimeText(item.value),
    item.value.OfficialRating || ''
  ].filter(Boolean);
});

const sourceTabs = computed(() =>
  (item.value?.MediaSources || []).map((source, index) => ({
    value: String(index),
    label: source.Container || `版本 ${index + 1}`
  }))
);

const sourceTabValue = computed({
  get: () => String(currentSourceIndex.value),
  set: (v) => (currentSourceIndex.value = Number(v) || 0)
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

async function toggleFavorite() {
  if (!item.value) return;
  const userData = await api.markFavorite(item.value.Id, !item.value.UserData.IsFavorite);
  item.value = { ...item.value, UserData: { ...item.value.UserData, ...userData } };
}

async function togglePlayed() {
  if (!item.value) return;
  const userData = await api.markPlayed(item.value.Id, !item.value.UserData.Played);
  item.value = { ...item.value, UserData: { ...item.value.UserData, ...userData } };
}

async function openGenre(name: string) {
  if (!item.value) return;
  await router.push(genreRoute(name, item.value.Type));
}

function runtimeText(target: BaseItemDto) {
  if (!target.RunTimeTicks) return '';
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
  <div v-if="loading" class="flex min-h-[50vh] flex-col items-center justify-center gap-2">
    <UProgress animation="carousel" class="w-48" />
    <p class="text-muted text-sm">正在读取媒体元数据…</p>
  </div>

  <UAlert
    v-else-if="error"
    color="error"
    variant="subtle"
    icon="i-lucide-triangle-alert"
    title="加载失败"
    :description="error"
  />

  <div v-else-if="item" class="flex flex-col gap-6">
    <!-- 面包屑 -->
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
      <span class="text-muted">{{ item.Type }}</span>
      <UIcon name="i-lucide-chevron-right" class="size-3 text-muted" />
      <span class="text-highlighted font-medium">{{ item.Name }}</span>
    </nav>

    <!-- Hero -->
    <section class="relative overflow-hidden rounded-2xl ring-1 ring-default">
      <img
        v-if="backdrop"
        :src="backdrop"
        :alt="item.Name"
        class="absolute inset-0 h-full w-full object-cover opacity-40 blur-sm"
      />
      <div class="absolute inset-0 bg-gradient-to-br from-(--ui-bg)/80 via-(--ui-bg)/70 to-(--ui-bg)/95" />

      <div class="relative grid gap-6 p-5 sm:p-8 lg:grid-cols-[220px_1fr] lg:gap-10">
        <div
          class="aspect-[2/3] w-44 overflow-hidden rounded-xl bg-elevated ring-1 ring-default lg:w-full"
        >
          <img
            v-if="poster"
            :src="poster"
            :alt="item.Name"
            class="h-full w-full object-cover"
          />
          <div
            v-else
            class="flex h-full w-full items-center justify-center bg-gradient-to-br from-primary/30 to-primary/5 text-3xl font-bold text-primary"
          >
            {{ item.IsFolder ? '目录' : item.Name.slice(0, 1).toUpperCase() }}
          </div>
        </div>

        <div class="flex flex-col gap-4">
          <div>
            <p class="text-muted text-xs uppercase tracking-wider">
              {{ item.SeriesName || item.SeasonName || item.Type }}
            </p>
            <h1 class="text-highlighted mt-1 text-2xl font-bold sm:text-3xl">{{ item.Name }}</h1>
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
              v-if="item.CommunityRating"
              color="warning"
              variant="soft"
              icon="i-lucide-star"
            >
              {{ Number(item.CommunityRating).toFixed(1) }}
            </UBadge>
          </div>

          <p
            v-if="item.Tagline || item.Taglines?.[0]"
            class="text-primary/90 max-w-3xl text-sm italic"
          >
            “{{ item.Tagline || item.Taglines?.[0] }}”
          </p>

          <p v-if="item.Overview" class="text-default max-w-3xl text-sm leading-relaxed">
            {{ item.Overview }}
          </p>

          <div class="flex flex-wrap gap-2">
            <UButton
              v-if="playable"
              icon="i-lucide-play"
              size="lg"
              @click="playItem(item)"
            >
              播放
            </UButton>
            <UButton
              color="neutral"
              variant="subtle"
              :icon="item.UserData.IsFavorite ? 'i-lucide-heart-off' : 'i-lucide-heart'"
              @click="toggleFavorite"
            >
              {{ item.UserData.IsFavorite ? '取消收藏' : '加入收藏' }}
            </UButton>
            <UButton
              color="neutral"
              variant="subtle"
              :icon="item.UserData.Played ? 'i-lucide-eye-off' : 'i-lucide-eye'"
              @click="togglePlayed"
            >
              {{ item.UserData.Played ? '标记未看' : '标记已看' }}
            </UButton>
          </div>

          <p v-if="item.Path" class="text-muted truncate font-mono text-xs">
            {{ item.Path }}
          </p>
        </div>
      </div>
    </section>

    <!-- 媒体源 / 流 -->
    <UCard
      v-if="currentStreams.length"
      variant="soft"
      :ui="{ body: 'p-4 sm:p-5' }"
    >
      <UTabs
        v-if="sourceTabs.length > 1"
        v-model="sourceTabValue"
        :items="sourceTabs"
        variant="pill"
        size="xs"
        :content="false"
        class="mb-4"
      />

      <dl class="grid gap-3 sm:grid-cols-2">
        <div
          v-for="stream in currentStreams"
          :key="`${stream.Type}-${stream.Index}`"
          class="flex flex-col gap-1 rounded-lg bg-elevated/40 p-3"
        >
          <dt class="text-muted text-xs">{{ streamLabel(stream.Type) }}</dt>
          <dd class="text-default text-sm">{{ streamLine(stream) }}</dd>
        </div>
        <div v-if="currentSource?.Size" class="flex flex-col gap-1 rounded-lg bg-elevated/40 p-3">
          <dt class="text-muted text-xs">文件大小</dt>
          <dd class="text-default text-sm">{{ fileSize(currentSource.Size) }}</dd>
        </div>
      </dl>
    </UCard>

    <!-- 类型 -->
    <section v-if="item.Genres?.length" class="space-y-3">
      <h3 class="text-highlighted text-sm font-semibold">类型</h3>
      <div class="flex flex-wrap gap-2">
        <UButton
          v-for="genre in item.Genres"
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

    <!-- 演职人员 -->
    <section v-if="item.People?.length" class="space-y-3">
      <div class="flex items-baseline justify-between">
        <h3 class="text-highlighted text-base font-semibold">演职人员</h3>
        <span class="text-muted text-sm">{{ item.People.length }} 人</span>
      </div>
      <div
        class="flex snap-x snap-mandatory gap-3 overflow-x-auto pb-2
               [-ms-overflow-style:none] [scrollbar-width:thin]
               [&::-webkit-scrollbar]:h-1.5
               [&::-webkit-scrollbar-thumb]:rounded-full
               [&::-webkit-scrollbar-thumb]:bg-default"
      >
        <div
          v-for="(person, index) in item.People"
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

    <!-- 标签 -->
    <section v-if="item.Tags?.length" class="space-y-3">
      <h3 class="text-highlighted text-sm font-semibold">标签</h3>
      <div class="flex flex-wrap gap-1.5">
        <UBadge
          v-for="tag in item.Tags"
          :key="tag"
          color="neutral"
          variant="outline"
          size="sm"
        >
          {{ tag }}
        </UBadge>
      </div>
    </section>

    <!-- 子项 -->
    <section v-if="childItems.length" class="space-y-3">
      <div class="flex items-baseline justify-between">
        <h3 class="text-highlighted text-base font-semibold">
          {{ item.Type === 'Season' ? '分集' : '内容' }}
        </h3>
        <span class="text-muted text-sm">{{ childItems.length }} 项</span>
      </div>
      <div
        class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-7"
      >
        <MediaCard
          v-for="child in childItems"
          :key="child.Id"
          :item="child"
          :thumb="item.Type === 'Season'"
          @play="playItem"
          @select="openChild"
        />
      </div>
    </section>

    <!-- 相关 -->
    <MediaRow
      v-if="relatedItems.length"
      :title="item.Type === 'Episode' ? '同季内容' : '更多内容'"
      :items="relatedItems"
      icon="i-lucide-layers"
      @play="playItem"
      @select="openChild"
    />
  </div>
</template>
