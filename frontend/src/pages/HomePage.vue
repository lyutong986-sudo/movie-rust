<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import MediaRow from '../components/MediaRow.vue';
import HeroCarousel from '../components/HeroCarousel.vue';
import EmptyState from '../components/EmptyState.vue';
import {
  backToHome,
  continueWatching,
  enterHome,
  favorites,
  homeItems,
  isAdmin,
  latest,
  latestByLibrary,
  libraries,
  playQueue,
  state,
  watchLater
} from '../store/app';
import type { BaseItemDto } from '../api/emby';
import { itemRoute, playbackRoute } from '../utils/navigation';

const router = useRouter();

const libraryList = computed(() => libraries.value ?? []);

const heroItems = computed(() => {
  const pool: BaseItemDto[] = [];
  const seen = new Set<string>();
  const push = (item?: BaseItemDto) => {
    if (!item || seen.has(item.Id)) return;
    seen.add(item.Id);
    pool.push(item);
  };
  continueWatching.value.slice(0, 2).forEach(push);
  latest.value.slice(0, 5).forEach(push);
  homeItems.value.slice(0, 5).forEach(push);
  return pool.slice(0, 5);
});

const latestSections = computed(() =>
  libraryList.value
    .map((library) => ({
      library,
      label: librarySectionLabel(library.CollectionType),
      items: latestByLibrary.value[library.Id] || []
    }))
    .filter((section) => section.items.length)
);

onMounted(async () => {
  if (state.selectedLibraryId) {
    await backToHome();
    return;
  }

  if (!libraries.value.length || !homeItems.value.length) {
    await enterHome();
  }
});

function libraryIcon(collectionType?: string) {
  if (collectionType === 'movies') return 'i-lucide-clapperboard';
  if (collectionType === 'tvshows') return 'i-lucide-tv';
  return 'i-lucide-folder';
}

function librarySectionLabel(collectionType?: string) {
  if (collectionType === 'tvshows') return '最新剧集';
  if (collectionType === 'movies') return '最新电影';
  return '最新内容';
}

async function openItem(item: BaseItemDto) {
  await router.push(itemRoute(item));
}

async function playItem(item: BaseItemDto) {
  await router.push(playbackRoute(item));
}
</script>

<template>
  <div class="min-h-0 w-full min-w-0 flex-1">
    <div v-if="libraryList.length" class="flex flex-col gap-8">
      <HeroCarousel
        v-if="heroItems.length"
        :items="heroItems"
        @play="playItem"
        @select="openItem"
      />

      <section class="space-y-3">
        <div class="flex items-baseline justify-between">
          <h2 class="text-highlighted text-lg font-semibold">媒体库</h2>
          <span class="text-muted text-sm">{{ libraryList.length }} 个</span>
        </div>
        <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
          <button
            v-for="library in libraryList"
            :key="library.Id"
            type="button"
            class="group flex flex-col items-start gap-2 rounded-xl border border-default bg-elevated/30 p-4 text-start transition hover:bg-elevated/70 hover:ring-1 hover:ring-primary/40"
            @click="router.push(`/library/${library.Id}`)"
          >
            <div
              class="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10 text-primary transition group-hover:bg-primary group-hover:text-primary-contrast"
            >
              <UIcon :name="libraryIcon(library.CollectionType)" class="size-5" />
            </div>
            <div class="min-w-0">
              <p class="text-highlighted truncate text-sm font-medium">{{ library.Name }}</p>
              <p class="text-muted text-xs">{{ library.ChildCount || 0 }} 个条目</p>
            </div>
          </button>
        </div>
      </section>

      <MediaRow
        v-if="continueWatching.length"
        title="继续观看"
        icon="i-lucide-play-circle"
        :items="continueWatching"
        thumb
        @play="playItem"
        @select="openItem"
      />

      <MediaRow
        v-if="playQueue.length"
        title="播放队列"
        icon="i-lucide-list-video"
        :items="playQueue"
        thumb
        @play="playItem"
        @select="openItem"
      />

      <MediaRow
        v-if="watchLater.length"
        title="稍后观看"
        icon="i-lucide-clock"
        :items="watchLater"
        @play="playItem"
        @select="openItem"
      />

      <MediaRow
        v-if="favorites.length"
        title="收藏"
        icon="i-lucide-heart"
        :items="favorites"
        @play="playItem"
        @select="openItem"
      />

      <MediaRow
        v-if="latest.length"
        title="最近添加"
        icon="i-lucide-sparkles"
        :items="latest"
        @play="playItem"
        @select="openItem"
      />

      <MediaRow
        v-for="section in latestSections"
        :key="section.library.Id"
        :title="section.library.Name"
        :hint="section.label"
        :icon="libraryIcon(section.library.CollectionType)"
        :items="section.items"
        @play="playItem"
        @select="openItem"
      />
    </div>

    <EmptyState
      v-else
      icon="i-lucide-film"
      title="还没有媒体内容"
      description="先创建媒体库并执行扫描，首页的继续观看、最近添加、详情页和播放链路才会完整出现。"
      :action-label="isAdmin ? '创建媒体库' : '重新加载'"
      :action-icon="isAdmin ? 'i-lucide-plus' : 'i-lucide-refresh-cw'"
      @action="() => (isAdmin ? router.push('/settings/libraries') : enterHome())"
    />
  </div>
</template>
