<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import MediaRow from '../components/MediaRow.vue';
import {
  api,
  backToHome,
  continueWatching,
  enterHome,
  favorites,
  homeItems,
  isAdmin,
  latest,
  latestByLibrary,
  libraries,
  state
} from '../store/app';
import type { BaseItemDto } from '../api/emby';
import { itemRoute, playbackRoute } from '../utils/navigation';

const router = useRouter();

const heroItem = computed(
  () => continueWatching.value[0] || latest.value[0] || homeItems.value[0] || null
);
const heroImage = computed(() =>
  heroItem.value ? api.backdropUrl(heroItem.value) || api.itemImageUrl(heroItem.value) : ''
);

const latestSections = computed(() =>
  libraries.value
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
  if (collectionType === 'music') return 'i-lucide-music';
  if (collectionType === 'books') return 'i-lucide-book';
  return 'i-lucide-folder';
}

function librarySectionLabel(collectionType?: string) {
  if (collectionType === 'tvshows') return '电视剧';
  if (collectionType === 'movies') return '最新电影';
  if (collectionType === 'music') return '最新音乐';
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
  <div v-if="libraries.length" class="flex flex-col gap-8">
    <!-- Hero -->
    <section
      v-if="heroItem"
      class="relative overflow-hidden rounded-2xl ring-1 ring-default"
    >
      <div class="relative h-[320px] sm:h-[380px] lg:h-[440px]">
        <img
          v-if="heroImage"
          :src="heroImage"
          :alt="heroItem.Name"
          class="absolute inset-0 h-full w-full object-cover"
        />
        <div
          v-else
          class="absolute inset-0 bg-gradient-to-br from-primary/30 to-primary/5"
        />
        <div
          class="absolute inset-0 bg-gradient-to-t from-black via-black/70 to-transparent"
        />

        <div class="absolute inset-x-0 bottom-0 flex max-w-3xl flex-col gap-3 p-6 sm:p-8">
          <UBadge color="primary" variant="subtle" class="w-fit">最近添加</UBadge>
          <h1 class="text-3xl font-bold text-white sm:text-4xl">{{ heroItem.Name }}</h1>
          <p class="line-clamp-3 max-w-2xl text-white/80">
            {{ heroItem.Overview || '从这里继续浏览你的媒体库。' }}
          </p>
          <div class="mt-2 flex flex-wrap gap-2">
            <UButton
              v-if="heroItem.MediaSources?.length"
              icon="i-lucide-play"
              size="lg"
              @click="playItem(heroItem)"
            >
              立即播放
            </UButton>
            <UButton
              color="neutral"
              variant="subtle"
              size="lg"
              icon="i-lucide-info"
              @click="openItem(heroItem)"
            >
              查看详情
            </UButton>
          </div>
        </div>
      </div>
    </section>

    <!-- 媒体库 -->
    <section class="space-y-3">
      <div class="flex items-baseline justify-between">
        <h2 class="text-highlighted text-lg font-semibold">媒体库</h2>
        <span class="text-muted text-sm">{{ libraries.length }} 个</span>
      </div>
      <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
        <button
          v-for="library in libraries"
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

  <!-- 空状态 -->
  <div
    v-else
    class="flex min-h-[60vh] flex-col items-center justify-center gap-4 rounded-2xl border border-dashed border-default bg-elevated/20 p-10 text-center"
  >
    <div class="flex h-16 w-16 items-center justify-center rounded-2xl bg-primary/10 text-primary">
      <UIcon name="i-lucide-film" class="size-8" />
    </div>
    <h2 class="text-highlighted text-xl font-semibold">还没有媒体内容</h2>
    <p class="text-muted max-w-md text-sm">
      先创建媒体库并执行扫描，首页的继续观看、最近添加、详情页和播放链路才会完整出现。
    </p>
    <div class="flex flex-wrap gap-2">
      <UButton
        v-if="isAdmin"
        icon="i-lucide-plus"
        @click="router.push('/settings/libraries')"
      >
        创建媒体库
      </UButton>
      <UButton
        color="neutral"
        variant="subtle"
        icon="i-lucide-refresh-cw"
        @click="enterHome"
      >
        重新加载
      </UButton>
    </div>
  </div>
</template>
