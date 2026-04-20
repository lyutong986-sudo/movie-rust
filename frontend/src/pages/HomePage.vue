<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import MediaCard from '../components/MediaCard.vue';
import { api, backToHome, continueWatching, enterHome, favorites, homeItems, isAdmin, latest, latestByLibrary, libraries, state } from '../store/app';
import type { BaseItemDto } from '../api/emby';
import { itemRoute, playbackRoute } from '../utils/navigation';

const router = useRouter();

const heroItem = computed(() => continueWatching.value[0] || latest.value[0] || homeItems.value[0] || null);
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
  if (collectionType === 'movies') return '电影';
  if (collectionType === 'tvshows') return '剧集';
  if (collectionType === 'music') return '音乐';
  return '目录';
}

async function openItem(item: BaseItemDto) {
  await router.push(itemRoute(item));
}

function librarySectionLabel(collectionType?: string) {
  if (collectionType === 'tvshows') return '电视剧';
  if (collectionType === 'movies') return '最新电影';
  if (collectionType === 'music') return '最新音乐';
  return '最新内容';
}

async function playItem(item: BaseItemDto) {
  await router.push(playbackRoute(item));
}
</script>

<template>
  <section v-if="libraries.length" class="home-sections">
    <section v-if="heroItem" class="hero-strip">
      <img v-if="heroImage" :src="heroImage" :alt="heroItem.Name" />
      <div v-else class="poster-fallback">{{ heroItem.Name.slice(0, 1).toUpperCase() }}</div>
      <div>
        <p>最近添加</p>
        <h2>{{ heroItem.Name }}</h2>
        <p>{{ heroItem.Overview || '从这里继续浏览你的媒体库。' }}</p>
        <div class="button-row">
          <button
            v-if="heroItem.MediaSources?.length"
            type="button"
            @click="playItem(heroItem)"
          >
            立即播放
          </button>
          <button class="secondary" type="button" @click="openItem(heroItem)">查看详情</button>
        </div>
      </div>
    </section>

    <section class="media-row">
      <div class="section-heading">
        <h3>媒体库</h3>
        <span>{{ libraries.length }} 个</span>
      </div>
      <div class="rail">
        <button
          v-for="library in libraries"
          :key="library.Id"
          class="library-tile"
          type="button"
          @click="router.push(`/library/${library.Id}`)"
        >
          <span class="library-icon">{{ libraryIcon(library.CollectionType) }}</span>
          <strong>{{ library.Name }}</strong>
          <p>{{ library.ChildCount || 0 }} 个条目</p>
        </button>
      </div>
    </section>

    <section v-if="continueWatching.length" class="media-row">
      <div class="section-heading">
        <h3>继续观看</h3>
        <span>{{ continueWatching.length }} 项</span>
      </div>
      <div class="rail poster-rail">
        <MediaCard
          v-for="item in continueWatching"
          :key="item.Id"
          :item="item"
          @play="playItem"
          @select="openItem"
        />
      </div>
    </section>

    <section v-if="favorites.length" class="media-row">
      <div class="section-heading">
        <h3>收藏</h3>
        <span>{{ favorites.length }} 项</span>
      </div>
      <div class="rail poster-rail">
        <MediaCard
          v-for="item in favorites"
          :key="item.Id"
          :item="item"
          @play="playItem"
          @select="openItem"
        />
      </div>
    </section>

    <section v-if="latest.length" class="media-row">
      <div class="section-heading">
        <h3>最近添加</h3>
        <span>{{ latest.length }} 项</span>
      </div>
      <div class="rail poster-rail">
        <MediaCard
          v-for="item in latest"
          :key="item.Id"
          :item="item"
          @play="playItem"
          @select="openItem"
        />
      </div>
    </section>

    <section v-for="section in latestSections" :key="section.library.Id" class="media-row">
      <div class="section-heading">
        <h3>{{ section.library.Name }}</h3>
        <span>{{ section.label }}</span>
      </div>
      <div class="rail poster-rail">
        <MediaCard
          v-for="item in section.items"
          :key="item.Id"
          :item="item"
          @play="playItem"
          @select="openItem"
        />
      </div>
    </section>
  </section>

  <section v-else class="empty">
    <p>Jellyfin 风格首页至少需要一个媒体库。</p>
    <h2>还没有媒体内容</h2>
    <p>先创建媒体库并执行扫描，首页的继续观看、最近添加、详情页和播放链路才会完整出现。</p>
    <div class="button-row">
      <button v-if="isAdmin" type="button" @click="router.push('/settings/libraries')">创建媒体库</button>
      <button class="secondary" type="button" @click="enterHome">重新加载</button>
    </div>
  </section>
</template>
