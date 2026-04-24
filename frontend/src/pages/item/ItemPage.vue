<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import MediaRow from '../../components/MediaRow.vue';
import MediaQualityBadges from '../../components/MediaQualityBadges.vue';
import type { BaseItemDto, MediaStreamDto, PlaylistInfo } from '../../api/emby';
import {
  api,
  enqueue,
  fileSize,
  isInWatchLater,
  streamLabel,
  streamText,
  toggleWatchLater
} from '../../store/app';
import { useAppToast } from '../../composables/toast';
import { genreRoute, itemRoute, playbackRoute } from '../../utils/navigation';

const route = useRoute();
const router = useRouter();
const toast = useAppToast();

const loading = ref(false);
const error = ref('');
const item = ref<BaseItemDto | null>(null);
const childItems = ref<BaseItemDto[]>([]);
const relatedItems = ref<BaseItemDto[]>([]);
const similarItems = ref<BaseItemDto[]>([]);
const currentSourceIndex = ref(0);
const trailerOpen = ref(false);
const trailerEmbed = ref('');
const playlistPickerOpen = ref(false);
const playlistOptions = ref<PlaylistInfo[]>([]);
const playlistLoading = ref(false);
const newPlaylistName = ref('');

async function openPlaylistPicker() {
  if (!item.value) return;
  playlistPickerOpen.value = true;
  playlistLoading.value = true;
  try {
    const result = await api.listPlaylists();
    playlistOptions.value = result.Items;
  } catch (error) {
    toast.error(error instanceof Error ? error.message : String(error));
  } finally {
    playlistLoading.value = false;
  }
}

async function addCurrentToPlaylist(playlistId: string) {
  if (!item.value) return;
  try {
    await api.addPlaylistItems(playlistId, [item.value.Id]);
    toast.success('已加入播放列表');
    playlistPickerOpen.value = false;
  } catch (error) {
    toast.error(error instanceof Error ? error.message : String(error));
  }
}

async function createAndAddPlaylist() {
  if (!item.value) return;
  const name = newPlaylistName.value.trim();
  if (!name) {
    toast.error('请输入播放列表名称');
    return;
  }
  try {
    const created = await api.createPlaylist({
      Name: name,
      MediaType: 'Video',
      Ids: [item.value.Id]
    });
    toast.success(`已创建「${created.Name}」并加入条目`);
    newPlaylistName.value = '';
    playlistPickerOpen.value = false;
  } catch (error) {
    toast.error(error instanceof Error ? error.message : String(error));
  }
}

const currentSource = computed(() => item.value?.MediaSources?.[currentSourceIndex.value]);
const currentStreams = computed(
  () => currentSource.value?.MediaStreams || item.value?.MediaStreams || []
);
const backdrop = computed(() => (item.value ? api.backdropUrl(item.value) : ''));
const poster = computed(() =>
  item.value ? api.itemImageUrl(item.value) || api.backdropUrl(item.value) : ''
);
const logo = computed(() => (item.value ? api.logoUrl(item.value) : ''));
const playable = computed(
  () => Boolean(item.value && !item.value.IsFolder && item.value.MediaSources?.length)
);
const metaChips = computed(() => {
  if (!item.value) return [];
  return [
    item.value.Type,
    item.value.ProductionYear ? String(item.value.ProductionYear) : '',
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

const hasTrailer = computed(() =>
  Boolean(item.value?.RemoteTrailers?.length || item.value?.LocalTrailerCount)
);

const chapters = computed(() => {
  const runtime = item.value?.RunTimeTicks || 0;
  return (item.value?.Chapters || []).map((c, i) => ({
    name: c.Name || `章节 ${i + 1}`,
    timestamp: formatTicks(c.StartPositionTicks),
    imageUrl: api.chapterImageUrl(item.value!, i, c.ImageTag),
    percent: runtime ? (c.StartPositionTicks / runtime) * 100 : 0,
    index: i
  }));
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
  similarItems.value = [];

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
      ).Items.filter((c) => c.Id !== currentItem.Id);
    } else {
      relatedItems.value = [];
    }

    // Similar：Emby 的 /Items/{id}/Similar
    if (currentItem.Type === 'Movie' || currentItem.Type === 'Series') {
      try {
        const sim = await api.similar(currentItem.Id, 20);
        similarItems.value = sim.Items || [];
      } catch {
        similarItems.value = [];
      }
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
  toast.success(userData.IsFavorite ? '已加入收藏' : '已取消收藏');
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

function formatTicks(ticks: number) {
  const total = Math.floor(ticks / 10_000_000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(h > 0 ? 2 : 1, '0');
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

function openTrailer() {
  const url = item.value?.RemoteTrailers?.[0]?.Url;
  if (!url) return;
  const match = url.match(/(?:youtu\.be\/|v=)([\w-]{11})/);
  if (match) {
    trailerEmbed.value = `https://www.youtube-nocookie.com/embed/${match[1]}?autoplay=1`;
    trailerOpen.value = true;
  } else {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

async function copyPath() {
  if (!item.value?.Path) return;
  try {
    await navigator.clipboard.writeText(item.value.Path);
    toast.success('路径已复制');
  } catch {
    toast.error('复制失败', '浏览器未授权剪贴板');
  }
}

function enqueueCurrent(position: 'next' | 'last') {
  if (!item.value) return;
  enqueue(item.value, position);
  toast.success(position === 'next' ? '已加入下一首' : '已加入播放队列');
}

function toggleLater() {
  if (!item.value) return;
  const wasIn = isInWatchLater(item.value.Id);
  toggleWatchLater(item.value);
  toast.success(wasIn ? '已从稍后观看移除' : '已添加到稍后观看');
}

const tocTabs = computed(() => {
  const tabs: Array<{ label: string; value: string }> = [];
  if (chapters.value.length) tabs.push({ label: '章节', value: 'chapters' });
  if (currentStreams.value.length) tabs.push({ label: '媒体流', value: 'streams' });
  return tabs;
});
const tocValue = ref('chapters');
watch(
  () => chapters.value.length,
  (len) => {
    tocValue.value = len ? 'chapters' : 'streams';
  }
);
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

  <div v-else-if="item" class="flex flex-col gap-10">
    <!-- Hero -->
    <section class="ring-default relative overflow-hidden rounded-2xl ring-1">
      <img
        v-if="backdrop"
        :src="backdrop"
        :alt="item.Name"
        class="absolute inset-0 h-full w-full object-cover opacity-40 blur-sm"
      />
      <div class="absolute inset-0 bg-gradient-to-br from-(--ui-bg)/80 via-(--ui-bg)/70 to-(--ui-bg)/95" />

      <div class="relative grid gap-6 p-5 sm:p-8 lg:grid-cols-[220px_1fr] lg:gap-10">
        <div
          class="bg-elevated ring-default aspect-[2/3] w-44 overflow-hidden rounded-xl ring-1 lg:w-full"
        >
          <img v-if="poster" :src="poster" :alt="item.Name" class="h-full w-full object-cover" />
          <div
            v-else
            class="from-primary/30 to-primary/5 text-primary flex h-full w-full items-center justify-center bg-gradient-to-br text-3xl font-bold"
          >
            {{ item.IsFolder ? '目录' : item.Name.slice(0, 1).toUpperCase() }}
          </div>
        </div>

        <div class="flex flex-col gap-4">
          <div>
            <p class="text-muted text-xs uppercase tracking-wider">
              {{ item.SeriesName || item.SeasonName || item.Type }}
            </p>
            <img
              v-if="logo"
              :src="logo"
              :alt="item.Name"
              class="mt-1 max-h-16 w-auto"
            />
            <h1
              v-else
              class="text-highlighted display-font mt-1 text-2xl font-bold sm:text-3xl"
            >
              {{ item.Name }}
            </h1>
          </div>

          <div class="flex flex-wrap items-center gap-2">
            <MediaQualityBadges :item="item" />
            <UBadge v-for="chip in metaChips" :key="chip" color="neutral" variant="soft">
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
            "{{ item.Tagline || item.Taglines?.[0] }}"
          </p>

          <p v-if="item.Overview" class="text-default max-w-3xl text-sm leading-relaxed">
            {{ item.Overview }}
          </p>

          <div class="flex flex-wrap gap-2">
            <UButton v-if="playable" icon="i-lucide-play" size="lg" @click="playItem(item)">
              播放
            </UButton>
            <UButton
              v-if="hasTrailer"
              color="neutral"
              variant="subtle"
              size="lg"
              icon="i-lucide-film"
              @click="openTrailer"
            >
              预告片
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
            <UDropdownMenu
              :items="[
                [
                  { label: '添加到队列', icon: 'i-lucide-list-plus', onSelect: () => enqueueCurrent('last') },
                  { label: '作为下一首', icon: 'i-lucide-play-circle', onSelect: () => enqueueCurrent('next') },
                  {
                    label: isInWatchLater(item.Id) ? '从稍后观看移除' : '添加到稍后观看',
                    icon: 'i-lucide-clock',
                    onSelect: toggleLater
                  },
                  { label: '加入播放列表', icon: 'i-lucide-list-music', onSelect: openPlaylistPicker }
                ],
                [
                  { label: '复制文件路径', icon: 'i-lucide-clipboard-copy', onSelect: copyPath, disabled: !item.Path }
                ]
              ]"
            >
              <UButton color="neutral" variant="subtle" icon="i-lucide-more-horizontal">
                更多
              </UButton>
            </UDropdownMenu>
          </div>
        </div>
      </div>
    </section>

    <!-- 章节 -->
    <section v-if="chapters.length" class="space-y-3">
      <h3 class="text-highlighted text-base font-semibold">章节</h3>
      <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
        <div
          v-for="chapter in chapters"
          :key="chapter.index"
          class="bg-elevated/30 ring-default overflow-hidden rounded-lg ring-1"
        >
          <div class="bg-elevated aspect-video">
            <img
              v-if="chapter.imageUrl"
              :src="chapter.imageUrl"
              :alt="chapter.name"
              class="h-full w-full object-cover"
            />
          </div>
          <div class="p-2">
            <p class="text-highlighted truncate text-xs font-medium">{{ chapter.name }}</p>
            <p class="text-muted font-mono text-[10px]">{{ chapter.timestamp }}</p>
          </div>
        </div>
      </div>
    </section>

    <!-- 媒体源 / 流 折叠 -->
    <UCard v-if="currentStreams.length" variant="soft" :ui="{ body: 'p-4 sm:p-5' }">
      <div class="mb-3 flex items-center justify-between">
        <h3 class="text-highlighted text-sm font-semibold">媒体信息</h3>
        <UTabs
          v-if="sourceTabs.length > 1"
          v-model="sourceTabValue"
          :items="sourceTabs"
          variant="pill"
          size="xs"
          :content="false"
        />
      </div>

      <dl class="grid gap-3 sm:grid-cols-2">
        <div
          v-for="stream in currentStreams"
          :key="`${stream.Type}-${stream.Index}`"
          class="bg-elevated/40 flex flex-col gap-1 rounded-lg p-3"
        >
          <dt class="text-muted text-xs">{{ streamLabel(stream.Type) }}</dt>
          <dd class="text-default text-sm">{{ streamLine(stream) }}</dd>
        </div>
        <div v-if="currentSource?.Size" class="bg-elevated/40 flex flex-col gap-1 rounded-lg p-3">
          <dt class="text-muted text-xs">文件大小</dt>
          <dd class="text-default text-sm">{{ fileSize(currentSource.Size) }}</dd>
        </div>
        <div v-if="item.Path" class="bg-elevated/40 flex flex-col gap-1 rounded-lg p-3 sm:col-span-2">
          <dt class="text-muted flex items-center gap-2 text-xs">
            文件路径
            <UButton size="xs" variant="ghost" icon="i-lucide-clipboard-copy" @click="copyPath" />
          </dt>
          <dd class="text-default break-all font-mono text-xs">{{ item.Path }}</dd>
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
          class="border-default bg-elevated/20 flex w-28 shrink-0 snap-start flex-col items-center gap-2 rounded-lg border p-3 text-center"
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
            <p
              v-if="person.Role || person.Type"
              class="text-muted truncate text-[10px]"
              :title="person.Role || person.Type"
            >
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

    <!-- 相似 -->
    <MediaRow
      v-if="similarItems.length"
      title="类似内容"
      icon="i-lucide-sparkles"
      :items="similarItems"
      @play="playItem"
      @select="openChild"
    />

    <!-- 相关 -->
    <MediaRow
      v-if="relatedItems.length"
      :title="item.Type === 'Episode' ? '同季内容' : '更多内容'"
      :items="relatedItems"
      icon="i-lucide-layers"
      @play="playItem"
      @select="openChild"
    />

    <!-- 预告片 modal -->
    <UModal v-model:open="trailerOpen" :ui="{ content: 'max-w-4xl' }">
      <template #content>
        <div class="relative aspect-video w-full bg-black">
          <iframe
            v-if="trailerEmbed"
            :src="trailerEmbed"
            class="h-full w-full"
            allow="accelerometer; autoplay; encrypted-media; gyroscope; picture-in-picture"
            allowfullscreen
          />
        </div>
      </template>
    </UModal>

    <!-- 加入播放列表 modal -->
    <UModal v-model:open="playlistPickerOpen" :ui="{ content: 'max-w-lg' }">
      <template #header>
        <h3 class="text-highlighted text-base font-semibold">加入播放列表</h3>
      </template>
      <template #body>
        <div class="space-y-3">
          <div v-if="playlistLoading" class="text-muted text-center text-sm">加载中...</div>
          <div v-else-if="playlistOptions.length" class="max-h-72 space-y-2 overflow-y-auto">
            <button
              v-for="option in playlistOptions"
              :key="option.Id"
              type="button"
              class="border-default hover:bg-elevated/60 flex w-full items-center justify-between rounded-lg border p-3 text-start transition"
              @click="addCurrentToPlaylist(option.Id)"
            >
              <div class="min-w-0">
                <p class="text-highlighted truncate text-sm font-medium">{{ option.Name }}</p>
                <p class="text-muted text-xs">{{ option.ChildCount }} 个条目</p>
              </div>
              <UIcon name="i-lucide-plus" class="text-primary size-4" />
            </button>
          </div>
          <p v-else class="text-muted rounded-lg border border-default bg-elevated/30 p-3 text-sm">
            还没有播放列表，使用下方表单创建一个。
          </p>

          <div class="space-y-2">
            <p class="text-muted text-xs">或新建一个播放列表</p>
            <div class="flex gap-2">
              <UInput v-model="newPlaylistName" placeholder="新播放列表名称" class="flex-1" />
              <UButton icon="i-lucide-list-plus" @click="createAndAddPlaylist">创建并加入</UButton>
            </div>
          </div>
        </div>
      </template>
    </UModal>
  </div>
</template>
