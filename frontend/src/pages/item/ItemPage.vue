<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import MediaCard from '../../components/MediaCard.vue';
import MediaRow from '../../components/MediaRow.vue';
import MediaQualityBadges from '../../components/MediaQualityBadges.vue';
import MetadataEditorDialog from '../../components/MetadataEditorDialog.vue';
import IdentifyDialog from '../../components/IdentifyDialog.vue';
import MediaInfoDialog from '../../components/MediaInfoDialog.vue';
import CollectionEditorDialog from '../../components/CollectionEditorDialog.vue';
import type { BaseItemDto, ImageInfo, MediaStreamDto, PlaylistInfo, RemoteImageInfo, RemoteSubtitleInfo } from '../../api/emby';
import {
  api,
  enqueue,
  fileSize,
  isAdmin,
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
const specialFeatures = ref<BaseItemDto[]>([]);
const similarItems = ref<BaseItemDto[]>([]);
const similarLoading = ref(false);
const currentSourceIndex = ref(0);
const trailerOpen = ref(false);
const trailerEmbed = ref('');
const playlistPickerOpen = ref(false);
const playlistOptions = ref<PlaylistInfo[]>([]);
const playlistLoading = ref(false);
const newPlaylistName = ref('');
let itemRequestToken = 0;

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
  const chips = [
    item.value.Type,
    item.value.ProductionYear ? String(item.value.ProductionYear) : '',
    runtimeText(item.value),
    item.value.OfficialRating || ''
  ];
  if (item.value.PremiereDate) {
    const d = new Date(item.value.PremiereDate);
    if (!isNaN(d.getTime())) chips.push(d.toLocaleDateString('zh-CN'));
  }
  return chips.filter(Boolean);
});

const episodeTag = computed(() => {
  const it = item.value;
  if (!it || it.Type !== 'Episode') return '';
  const s = it.ParentIndexNumber;
  const e = it.IndexNumber;
  if (s != null && e != null) {
    const pad = (n: number) => String(n).padStart(2, '0');
    return `S${pad(s)}E${pad(e)}`;
  }
  return '';
});

// 当前条目在同季列表里的前后集（Episode 详情页上下集导航使用）。
const episodePrev = computed<BaseItemDto | null>(() => {
  const it = item.value;
  if (!it || it.Type !== 'Episode') return null;
  const sorted = [it, ...relatedItems.value]
    .filter((x) => x.Type === 'Episode')
    .slice()
    .sort((a, b) => (a.IndexNumber ?? 0) - (b.IndexNumber ?? 0));
  const idx = sorted.findIndex((x) => x.Id === it.Id);
  return idx > 0 ? sorted[idx - 1] : null;
});

const episodeNext = computed<BaseItemDto | null>(() => {
  const it = item.value;
  if (!it || it.Type !== 'Episode') return null;
  const sorted = [it, ...relatedItems.value]
    .filter((x) => x.Type === 'Episode')
    .slice()
    .sort((a, b) => (a.IndexNumber ?? 0) - (b.IndexNumber ?? 0));
  const idx = sorted.findIndex((x) => x.Id === it.Id);
  return idx >= 0 && idx < sorted.length - 1 ? sorted[idx + 1] : null;
});

function goToSeries() {
  const it = item.value;
  if (it?.SeriesId) {
    router.push(`/series/${it.SeriesId}`);
  }
}

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
  const requestToken = ++itemRequestToken;
  loading.value = true;
  error.value = '';
  similarItems.value = [];
  specialFeatures.value = [];
  similarLoading.value = false;

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

    if (requestToken !== itemRequestToken) {
      return;
    }
    item.value = currentItem;
    currentSourceIndex.value = 0;

    // 首屏并行仅加载关键数据：子项 + 同级相关，Similar 延迟加载。
    const childPromise = currentItem.IsFolder
      ? api.items(currentItem.Id, '', false, {
          sortBy: currentItem.Type === 'Season' ? 'IndexNumber' : 'SortName',
          sortOrder: 'Ascending',
          limit: 80,
          fields: ['Overview', 'MediaStreams', 'MediaSources', 'ChildCount', 'PrimaryImageAspectRatio'],
          enableImages: true,
          imageTypeLimit: 1,
          enableImageTypes: ['Primary', 'Thumb', 'Backdrop', 'Logo'],
          enableTotalRecordCount: false
        }).then((r) => r.Items).catch(() => [] as BaseItemDto[])
      : Promise.resolve([] as BaseItemDto[]);

    const relatedPromise = currentItem.ParentId
      ? api
          .items(currentItem.ParentId, '', false, {
            sortBy: currentItem.Type === 'Episode' ? 'IndexNumber' : 'SortName',
            sortOrder: 'Ascending',
            limit: 48,
            fields: ['Overview', 'MediaStreams', 'MediaSources', 'ChildCount', 'PrimaryImageAspectRatio'],
            enableImages: true,
            imageTypeLimit: 1,
            enableImageTypes: ['Primary', 'Thumb', 'Backdrop', 'Logo'],
            enableTotalRecordCount: false
          })
          .then((r) => r.Items.filter((c) => c.Id !== currentItem.Id))
          .catch(() => [] as BaseItemDto[])
      : Promise.resolve([] as BaseItemDto[]);

    const specialPromise = (currentItem.SpecialFeatureCount && currentItem.SpecialFeatureCount > 0)
      ? api.getSpecialFeatures(currentItem.Id).catch(() => [] as BaseItemDto[])
      : Promise.resolve([] as BaseItemDto[]);

    const [children, related, specials] = await Promise.all([
      childPromise,
      relatedPromise,
      specialPromise
    ]);
    if (requestToken !== itemRequestToken) {
      return;
    }
    childItems.value = children;
    relatedItems.value = related;
    specialFeatures.value = specials;
    void loadSimilarItems(currentItem, requestToken);
  } catch (loadError) {
    if (requestToken === itemRequestToken) {
      error.value = loadError instanceof Error ? loadError.message : String(loadError);
    }
    item.value = null;
    childItems.value = [];
    relatedItems.value = [];
    specialFeatures.value = [];
  } finally {
    if (requestToken === itemRequestToken) {
      loading.value = false;
    }
  }
}

async function loadSimilarItems(currentItem: BaseItemDto, requestToken: number) {
  if (currentItem.Type !== 'Movie' && currentItem.Type !== 'Series') {
    similarItems.value = [];
    return;
  }
  similarLoading.value = true;
  try {
    const result = await api.similar(currentItem.Id, {
      limit: 12,
      fields: ['MediaStreams', 'MediaSources', 'ChildCount', 'PrimaryImageAspectRatio'],
      enableImages: true,
      imageTypeLimit: 1,
      enableImageTypes: ['Primary', 'Thumb', 'Backdrop', 'Logo'],
      enableUserData: true
    });
    if (requestToken !== itemRequestToken) {
      return;
    }
    similarItems.value = result.Items || [];
  } catch {
    if (requestToken === itemRequestToken) {
      similarItems.value = [];
    }
  } finally {
    if (requestToken === itemRequestToken) {
      similarLoading.value = false;
    }
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
  const userData = await api.markFavorite(item.value.Id, !item.value.UserData?.IsFavorite);
  item.value = { ...item.value, UserData: { ...item.value.UserData, ...userData } };
  toast.success(userData.IsFavorite ? '已加入收藏' : '已取消收藏');
}

async function togglePlayed() {
  if (!item.value) return;
  const userData = await api.markPlayed(item.value.Id, !item.value.UserData?.Played);
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

function downloadFile() {
  if (!item.value || item.value.IsFolder) return;
  const url = api.streamUrl(item.value);
  if (!url) { toast.error('无法获取下载地址'); return; }
  const a = document.createElement('a');
  a.href = url;
  a.download = item.value.Name || 'download';
  a.target = '_blank';
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
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

const refreshing = ref(false);
const mediaInfoOpen = ref(false);
const metadataEditorOpen = ref(false);
const identifyOpen = ref(false);
const collectionDialogOpen = ref(false);
const subtitleOpen = ref(false);
const subtitleLang = ref('chi');
const subtitleResults = ref<RemoteSubtitleInfo[]>([]);
const subtitleSearching = ref(false);
const subtitleDownloading = ref<string | null>(null);

const subtitleLangOptions = [
  { label: '中文', value: 'chi' },
  { label: '英文', value: 'eng' },
  { label: '日语', value: 'jpn' },
  { label: '韩语', value: 'kor' },
  { label: '法语', value: 'fre' },
  { label: '德语', value: 'ger' },
  { label: '西班牙语', value: 'spa' }
];

const imageEditorOpen = ref(false);
const itemImages = ref<ImageInfo[]>([]);
const remoteImages = ref<RemoteImageInfo[]>([]);
const remoteImageType = ref('Primary');
const remoteImageLoading = ref(false);
const imageUploading = ref(false);
const imageDeletingType = ref<string | null>(null);

const imageTypeLabels: Record<string, string> = {
  Primary: '海报', Backdrop: '壁纸', Logo: '徽标', Thumb: '缩略图',
  Banner: '横幅图', Disc: '光盘封面', Art: '艺术图'
};

async function openImageEditor() {
  if (!item.value) return;
  imageEditorOpen.value = true;
  remoteImages.value = [];
  try {
    itemImages.value = await api.listItemImages(item.value.Id);
  } catch {
    itemImages.value = [];
  }
}

async function searchRemoteImages() {
  if (!item.value) return;
  remoteImageLoading.value = true;
  try {
    const result = await api.listRemoteImages(item.value.Id, {
      type: remoteImageType.value,
      IncludeAllLanguages: true,
      limit: 20
    });
    remoteImages.value = result.Images || [];
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '搜索远程图片失败');
    remoteImages.value = [];
  } finally {
    remoteImageLoading.value = false;
  }
}

async function downloadRemoteImage(img: RemoteImageInfo) {
  if (!item.value || imageUploading.value) return;
  imageUploading.value = true;
  try {
    await api.downloadRemoteImage(item.value.Id, img.Url, img.Type);
    toast.success('图片已下载');
    itemImages.value = await api.listItemImages(item.value.Id);
    await loadItem(item.value.Id);
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '图片下载失败');
  } finally {
    imageUploading.value = false;
  }
}

async function handleImageUpload(event: Event) {
  if (!item.value) return;
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  imageUploading.value = true;
  try {
    await api.uploadItemImage(item.value.Id, remoteImageType.value, file);
    toast.success('图片已上传');
    itemImages.value = await api.listItemImages(item.value.Id);
    await loadItem(item.value.Id);
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '图片上传失败');
  } finally {
    imageUploading.value = false;
    input.value = '';
  }
}

async function deleteImage(imageType: string, index?: number) {
  if (!item.value || imageDeletingType.value) return;
  imageDeletingType.value = imageType;
  try {
    await api.deleteItemImage(item.value.Id, imageType, index);
    toast.success('图片已删除');
    itemImages.value = await api.listItemImages(item.value.Id);
    await loadItem(item.value.Id);
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '删除失败');
  } finally {
    imageDeletingType.value = null;
  }
}

function itemImageUrl(imageType: string, index?: number) {
  if (!item.value) return '';
  const tag = imageType === 'Backdrop'
    ? item.value.BackdropImageTags?.[index ?? 0]
    : item.value.ImageTags?.[imageType];
  if (!tag) return '';
  const base = `${api.baseUrl}/Items/${item.value.Id}/Images/${imageType}`;
  const qs = `api_key=${encodeURIComponent(api.token)}&tag=${encodeURIComponent(tag)}&quality=90&maxWidth=300`;
  return index !== undefined ? `${base}/${index}?${qs}` : `${base}?${qs}`;
}

function openMetadataEditor() {
  metadataEditorOpen.value = true;
}

async function onMetadataSaved() {
  if (item.value) {
    await loadItem(item.value.Id);
  }
}

async function onIdentified() {
  if (item.value) {
    await loadItem(item.value.Id);
  }
}

async function refreshMetadata() {
  if (!item.value || refreshing.value) return;
  refreshing.value = true;
  try {
    await api.refreshItemMetadata(item.value.Id);
    toast.success('元数据刷新完成');
    if (item.value) {
      await loadItem(item.value.Id);
      try {
        itemImages.value = await api.listItemImages(item.value.Id);
      } catch {
        /* ignore */
      }
    }
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '刷新失败');
  } finally {
    refreshing.value = false;
  }
}

async function openSubtitleSearch() {
  subtitleOpen.value = true;
  subtitleResults.value = [];
  await searchSubtitles();
}

async function searchSubtitles() {
  if (!item.value) return;
  subtitleSearching.value = true;
  subtitleResults.value = [];
  try {
    subtitleResults.value = await api.searchSubtitles(item.value.Id, subtitleLang.value);
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '字幕搜索失败');
  } finally {
    subtitleSearching.value = false;
  }
}

async function downloadSubtitle(sub: RemoteSubtitleInfo) {
  if (!item.value || subtitleDownloading.value) return;
  subtitleDownloading.value = sub.Id;
  try {
    await api.downloadSubtitle(item.value.Id, sub.Id);
    toast.success(`字幕已下载: ${sub.Name}`);
    await loadItem(item.value.Id);
  } catch (err) {
    toast.error(err instanceof Error ? err.message : '字幕下载失败');
  } finally {
    subtitleDownloading.value = null;
  }
}

const externalLinks = computed(() => {
  const ids = item.value?.ProviderIds;
  if (!ids) return [];
  const links: Array<{ name: string; url: string; icon: string }> = [];
  if (ids.Tmdb) {
    const type = item.value?.Type === 'Series' ? 'tv' : item.value?.Type === 'Episode' ? 'tv' : 'movie';
    links.push({ name: 'TMDB', url: `https://www.themoviedb.org/${type}/${ids.Tmdb}`, icon: 'i-lucide-database' });
  }
  if (ids.Imdb) {
    links.push({ name: 'IMDb', url: `https://www.imdb.com/title/${ids.Imdb}`, icon: 'i-lucide-star' });
  }
  if (ids.Douban) {
    links.push({ name: '豆瓣', url: `https://movie.douban.com/subject/${ids.Douban}`, icon: 'i-lucide-book-open' });
  }
  if (ids.Tvdb) {
    links.push({ name: 'TheTVDB', url: `https://thetvdb.com/dereferrer/series/${ids.Tvdb}`, icon: 'i-lucide-tv' });
  }
  return links;
});

const hasExternalLinks = computed(() => externalLinks.value.length > 0);

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
            <!-- Episode：Series 名做成可点击链接；其他类型只显示静态标签。 -->
            <button
              v-if="item.Type === 'Episode' && item.SeriesId"
              type="button"
              class="text-primary hover:text-primary/80 flex items-center gap-1 text-sm font-medium"
              @click="goToSeries"
            >
              <UIcon name="i-lucide-arrow-left" class="size-3" />
              {{ item.SeriesName || '返回剧集' }}
            </button>
            <p v-else class="text-muted text-xs uppercase tracking-wider">
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
              <span
                v-if="episodeTag"
                class="text-primary mr-2 font-mono text-base font-semibold"
              >
                {{ episodeTag }}
              </span>
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
              :icon="item.UserData?.IsFavorite ? 'i-lucide-heart-off' : 'i-lucide-heart'"
              @click="toggleFavorite"
            >
              {{ item.UserData?.IsFavorite ? '取消收藏' : '加入收藏' }}
            </UButton>
            <UButton
              color="neutral"
              variant="subtle"
              :icon="item.UserData?.Played ? 'i-lucide-eye-off' : 'i-lucide-eye'"
              @click="togglePlayed"
            >
              {{ item.UserData?.Played ? '标记未看' : '标记已看' }}
            </UButton>
            <UButton
              v-if="!item.IsFolder"
              color="neutral"
              variant="subtle"
              icon="i-lucide-captions"
              @click="openSubtitleSearch"
            >
              字幕
            </UButton>
            <UButton
              color="neutral"
              variant="subtle"
              icon="i-lucide-folder-plus"
              @click="collectionDialogOpen = true"
            >
              添加到合集
            </UButton>
            <UButton
              v-if="!item.IsFolder"
              color="neutral"
              variant="subtle"
              icon="i-lucide-file-video"
              @click="mediaInfoOpen = true"
            >
              媒体信息
            </UButton>
            <UDropdownMenu
              :items="[
                [
                  { label: '刷新元数据', icon: 'i-lucide-refresh-cw', onSelect: refreshMetadata, disabled: refreshing },
                  ...(isAdmin ? [
                    { label: '编辑元数据', icon: 'i-lucide-file-edit', onSelect: openMetadataEditor },
                    { label: '识别', icon: 'i-lucide-search', onSelect: () => { identifyOpen = true; } }
                  ] : []),
                  { label: '搜索字幕', icon: 'i-lucide-captions', onSelect: openSubtitleSearch, disabled: item.IsFolder },
                  { label: '编辑图像', icon: 'i-lucide-image', onSelect: openImageEditor }
                ],
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
                  { label: '下载文件', icon: 'i-lucide-download', onSelect: downloadFile, disabled: item.IsFolder },
                  { label: '复制文件路径', icon: 'i-lucide-clipboard-copy', onSelect: copyPath, disabled: !item.Path }
                ]
              ]"
            >
              <UButton color="neutral" variant="subtle" icon="i-lucide-more-horizontal">
                更多
              </UButton>
            </UDropdownMenu>
          </div>

          <!-- Episode 专用：上一集 / 下一集 / 回到剧集 -->
          <div v-if="item.Type === 'Episode'" class="flex flex-wrap gap-2">
            <UButton
              color="neutral"
              variant="soft"
              size="sm"
              icon="i-lucide-skip-back"
              :disabled="!episodePrev"
              @click="() => episodePrev && openChild(episodePrev)"
            >
              上一集
              <span v-if="episodePrev" class="text-muted text-xs">
                · {{ episodePrev.Name }}
              </span>
            </UButton>
            <UButton
              color="neutral"
              variant="soft"
              size="sm"
              icon="i-lucide-skip-forward"
              :disabled="!episodeNext"
              trailing-icon="i-lucide-chevron-right"
              @click="() => episodeNext && openChild(episodeNext)"
            >
              下一集
              <span v-if="episodeNext" class="text-muted text-xs">
                · {{ episodeNext.Name }}
              </span>
            </UButton>
            <UButton
              v-if="item.SeriesId"
              color="primary"
              variant="subtle"
              size="sm"
              icon="i-lucide-tv"
              @click="goToSeries"
            >
              查看剧集详情
            </UButton>
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

    <!-- 花絮 / 特别内容 -->
    <section v-if="specialFeatures.length" class="space-y-3">
      <div class="flex items-baseline justify-between">
        <h3 class="text-highlighted text-base font-semibold">花絮 / 特别内容</h3>
        <span class="text-muted text-sm">{{ specialFeatures.length }} 项</span>
      </div>
      <div
        class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 xl:grid-cols-7"
      >
        <MediaCard
          v-for="sf in specialFeatures"
          :key="sf.Id"
          :item="sf"
          @play="playItem"
          @select="openChild"
        />
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
        <button
          v-for="(person, index) in item.People"
          :key="`${person.Id || person.Name}-${index}`"
          type="button"
          class="border-default bg-elevated/20 hover:bg-elevated/40 hover:ring-primary/40 flex w-28 shrink-0 snap-start cursor-pointer flex-col items-center gap-2 rounded-lg border p-3 text-center transition hover:ring-1"
          @click="router.push(person.Id ? `/person/${person.Id}` : `/search?q=${encodeURIComponent(person.Name)}`)"
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
        </button>
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

    <!-- 外部链接 -->
    <section v-if="hasExternalLinks" class="space-y-3">
      <h3 class="text-highlighted text-sm font-semibold">外部链接</h3>
      <div class="flex flex-wrap gap-2">
        <a
          v-for="link in externalLinks"
          :key="link.name"
          :href="link.url"
          target="_blank"
          rel="noopener noreferrer"
          class="inline-flex items-center gap-1.5 rounded-lg border border-default bg-elevated/30 px-3 py-1.5 text-sm text-highlighted transition hover:bg-elevated/60 hover:text-primary"
        >
          <UIcon :name="link.icon" class="size-4" />
          {{ link.name }}
          <UIcon name="i-lucide-external-link" class="size-3 text-muted" />
        </a>
      </div>
    </section>

    <!-- 制片工作室 -->
    <section v-if="item.Studios?.length" class="space-y-3">
      <h3 class="text-highlighted text-sm font-semibold">制片工作室</h3>
      <div class="flex flex-wrap gap-2">
        <UButton
          v-for="studio in item.Studios"
          :key="studio.Name"
          color="neutral"
          variant="outline"
          size="sm"
          @click="router.push(`/studio/${encodeURIComponent(studio.Name)}`)"
        >
          {{ studio.Name }}
        </UButton>
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
    <div v-else-if="similarLoading" class="text-muted text-sm">正在加载类似内容...</div>

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

    <!-- 字幕搜索 modal -->
    <UModal v-model:open="subtitleOpen" :ui="{ content: 'max-w-2xl' }">
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="text-highlighted text-base font-semibold">搜索字幕</h3>
        </div>
      </template>
      <template #body>
        <div class="space-y-4">
          <div class="flex items-center gap-3">
            <USelectMenu
              v-model="subtitleLang"
              :items="subtitleLangOptions"
              value-key="value"
              class="w-36"
            />
            <UButton
              icon="i-lucide-search"
              :loading="subtitleSearching"
              @click="searchSubtitles"
            >
              搜索
            </UButton>
            <span v-if="subtitleResults.length" class="text-muted text-sm">
              找到 {{ subtitleResults.length }} 条结果
            </span>
          </div>

          <div v-if="subtitleSearching" class="flex flex-col items-center gap-2 py-8">
            <UProgress animation="carousel" class="w-48" />
            <p class="text-muted text-sm">正在搜索字幕…</p>
          </div>

          <div v-else-if="subtitleResults.length" class="max-h-96 space-y-2 overflow-y-auto">
            <div
              v-for="sub in subtitleResults"
              :key="sub.Id"
              class="border-default hover:bg-elevated/40 flex items-center gap-3 rounded-lg border p-3 transition"
            >
              <div class="min-w-0 flex-1">
                <p class="text-highlighted truncate text-sm font-medium" :title="sub.Name">
                  {{ sub.Name }}
                </p>
                <div class="text-muted mt-0.5 flex flex-wrap gap-2 text-xs">
                  <span>{{ sub.ThreeLetterISOLanguageName || sub.Language }}</span>
                  <span>{{ sub.Format?.toUpperCase() }}</span>
                  <span v-if="sub.Author">{{ sub.Author }}</span>
                  <span v-if="sub.DownloadCount">下载 {{ sub.DownloadCount?.toLocaleString() }} 次</span>
                  <span v-if="sub.CommunityRating">评分 {{ sub.CommunityRating?.toFixed(1) }}</span>
                  <UBadge v-if="sub.IsHearingImpaired" size="xs" color="warning" variant="soft">SDH</UBadge>
                  <UBadge v-if="sub.IsHashMatch" size="xs" color="success" variant="soft">哈希匹配</UBadge>
                </div>
                <p v-if="sub.Comment" class="text-muted mt-0.5 truncate text-xs italic" :title="sub.Comment">
                  {{ sub.Comment }}
                </p>
              </div>
              <UButton
                size="sm"
                icon="i-lucide-download"
                :loading="subtitleDownloading === sub.Id"
                :disabled="!!subtitleDownloading"
                @click="downloadSubtitle(sub)"
              >
                下载
              </UButton>
            </div>
          </div>

          <div v-else class="text-muted py-8 text-center text-sm">
            <UIcon name="i-lucide-captions" class="mx-auto mb-2 size-8 opacity-50" />
            <p>选择语言后点击搜索</p>
          </div>
        </div>
      </template>
    </UModal>

    <!-- 合集编辑 modal -->
    <CollectionEditorDialog
      v-if="item"
      :item-ids="[item.Id]"
      v-model:open="collectionDialogOpen"
    />

    <!-- 元数据编辑 modal -->
    <MetadataEditorDialog
      v-if="item"
      :item="item"
      v-model:open="metadataEditorOpen"
      @saved="onMetadataSaved"
    />

    <!-- 识别 modal -->
    <IdentifyDialog
      v-if="item"
      :item="item"
      v-model:open="identifyOpen"
      @identified="onIdentified"
    />

    <!-- 媒体信息 modal -->
    <MediaInfoDialog
      v-if="item"
      :item="item"
      v-model:open="mediaInfoOpen"
    />

    <!-- 图像编辑 modal -->
    <UModal v-model:open="imageEditorOpen" :ui="{ content: 'max-w-3xl' }">
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="text-highlighted text-base font-semibold">编辑图像</h3>
        </div>
      </template>
      <template #body>
        <div class="space-y-6">
          <!-- 当前图片列表 -->
          <div>
            <h4 class="text-highlighted mb-3 text-sm font-semibold">图像</h4>
            <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
              <div
                v-for="imgType in ['Primary', 'Logo', 'Thumb', 'Banner', 'Disc', 'Art']"
                :key="imgType"
                class="border-default overflow-hidden rounded-lg border"
              >
                <div class="bg-elevated/30 relative aspect-video">
                  <img
                    v-if="itemImageUrl(imgType)"
                    :src="itemImageUrl(imgType)"
                    :alt="imageTypeLabels[imgType]"
                    class="size-full object-contain"
                  />
                  <div v-else class="text-muted flex size-full items-center justify-center text-xs">
                    {{ imageTypeLabels[imgType] || imgType }}
                  </div>
                </div>
                <div class="flex items-center justify-between p-2">
                  <span class="text-muted text-xs">{{ imageTypeLabels[imgType] || imgType }}</span>
                  <UButton
                    v-if="itemImageUrl(imgType)"
                    size="xs"
                    color="error"
                    variant="ghost"
                    icon="i-lucide-trash-2"
                    :loading="imageDeletingType === imgType"
                    @click="deleteImage(imgType)"
                  />
                </div>
              </div>
            </div>
          </div>

          <!-- 壁纸列表 -->
          <div v-if="item?.BackdropImageTags?.length">
            <h4 class="text-highlighted mb-3 text-sm font-semibold">壁纸</h4>
            <div class="grid grid-cols-2 gap-3 sm:grid-cols-3">
              <div
                v-for="(tag, idx) in item.BackdropImageTags"
                :key="tag"
                class="border-default overflow-hidden rounded-lg border"
              >
                <div class="bg-elevated/30 relative aspect-video">
                  <img
                    :src="itemImageUrl('Backdrop', idx)"
                    alt="壁纸"
                    class="size-full object-cover"
                  />
                </div>
                <div class="flex items-center justify-between p-2">
                  <span class="text-muted text-xs">壁纸 {{ idx + 1 }}</span>
                  <UButton
                    size="xs"
                    color="error"
                    variant="ghost"
                    icon="i-lucide-trash-2"
                    :loading="imageDeletingType === `Backdrop-${idx}`"
                    @click="deleteImage('Backdrop', idx)"
                  />
                </div>
              </div>
            </div>
          </div>

          <USeparator />

          <!-- 远程图片搜索 -->
          <div>
            <h4 class="text-highlighted mb-3 text-sm font-semibold">搜索远程图片</h4>
            <div class="flex items-center gap-3">
              <USelectMenu
                v-model="remoteImageType"
                :items="[
                  { label: '海报', value: 'Primary' },
                  { label: '壁纸', value: 'Backdrop' },
                  { label: '徽标', value: 'Logo' },
                  { label: '缩略图', value: 'Thumb' },
                  { label: '横幅图', value: 'Banner' }
                ]"
                value-key="value"
                class="w-32"
              />
              <UButton
                icon="i-lucide-search"
                :loading="remoteImageLoading"
                @click="searchRemoteImages"
              >
                搜索
              </UButton>
              <label class="cursor-pointer">
                <UButton
                  as="span"
                  icon="i-lucide-upload"
                  variant="outline"
                  :loading="imageUploading"
                >
                  上传本地图片
                </UButton>
                <input
                  type="file"
                  accept="image/*"
                  class="hidden"
                  @change="handleImageUpload"
                />
              </label>
            </div>

            <div v-if="remoteImageLoading" class="flex flex-col items-center gap-2 py-8">
              <UProgress animation="carousel" class="w-48" />
              <p class="text-muted text-sm">正在搜索远程图片…</p>
            </div>

            <div v-else-if="remoteImages.length" class="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-3">
              <div
                v-for="(img, idx) in remoteImages"
                :key="idx"
                class="border-default hover:border-primary cursor-pointer overflow-hidden rounded-lg border transition"
                @click="downloadRemoteImage(img)"
              >
                <div class="bg-elevated/30 relative aspect-video">
                  <img
                    :src="img.ThumbnailUrl || img.Url"
                    :alt="img.ProviderName"
                    class="size-full object-contain"
                    loading="lazy"
                  />
                </div>
                <div class="p-2">
                  <div class="text-muted flex items-center justify-between text-xs">
                    <span>{{ img.ProviderName }}</span>
                    <span v-if="img.Width && img.Height">{{ img.Width }}×{{ img.Height }}</span>
                  </div>
                  <div v-if="img.CommunityRating" class="text-muted text-xs">
                    评分 {{ img.CommunityRating.toFixed(1) }}
                    <span v-if="img.VoteCount">({{ img.VoteCount }})</span>
                  </div>
                  <div v-if="img.Language" class="text-muted text-xs">{{ img.Language }}</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </template>
    </UModal>
  </div>
</template>
