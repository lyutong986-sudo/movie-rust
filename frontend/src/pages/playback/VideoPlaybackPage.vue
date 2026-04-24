<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { BaseItemDto } from '../../api/emby';
import { api, fileSize, streamLabel, streamText } from '../../store/app';
import { itemRoute } from '../../utils/navigation';

const route = useRoute();
const router = useRouter();

const videoRef = ref<HTMLVideoElement | null>(null);
const loading = ref(false);
const error = ref('');
const item = ref<BaseItemDto | null>(null);
const currentSourceIndex = ref(0);
const playSessionId = ref('');
const overlayVisible = ref(true);
const paused = ref(true);

let overlayTimer = 0;
let hasStarted = false;
let hasStopped = false;
let lastProgressSecond = -10;

const itemId = computed(() => String(route.query.itemId || ''));
const currentSource = computed(() => item.value?.MediaSources?.[currentSourceIndex.value]);
const sourceUrl = computed(() =>
  currentSource.value
    ? api.streamUrlForSource(currentSource.value)
    : item.value
    ? api.streamUrl(item.value)
    : ''
);
const posterImage = computed(() =>
  item.value ? api.backdropUrl(item.value) || api.itemImageUrl(item.value) : ''
);
const currentStreams = computed(
  () => currentSource.value?.MediaStreams || item.value?.MediaStreams || []
);
const subtitleTracks = computed(() =>
  currentStreams.value
    .filter((stream) => stream.Type === 'Subtitle' && stream.DeliveryUrl)
    .map((stream) => ({
      key: `${stream.Type}-${stream.Index}`,
      label: stream.DisplayTitle || `字幕 ${stream.Index}`,
      src: api.subtitleUrl(stream.DeliveryUrl),
      srclang: (stream.Language || 'und').toLowerCase()
    }))
);

watch(
  () => route.query.itemId,
  async (value) => {
    if (typeof value === 'string' && value) {
      await loadPlayback(value);
    }
  },
  { immediate: true }
);

watch(
  () => currentSourceIndex.value,
  async () => {
    if (!videoRef.value || !sourceUrl.value) {
      return;
    }
    const currentTime = videoRef.value.currentTime;
    videoRef.value.load();
    try {
      await videoRef.value.play();
      videoRef.value.currentTime = currentTime;
    } catch {
      // Ignore autoplay failures
    }
  }
);

onBeforeUnmount(async () => {
  window.clearTimeout(overlayTimer);
  await stopPlayback();
});

async function loadPlayback(nextItemId: string) {
  loading.value = true;
  error.value = '';
  hasStarted = false;
  hasStopped = false;
  lastProgressSecond = -10;

  try {
    const [loadedItem, playback] = await Promise.all([
      api.item(nextItemId),
      api.playbackInfo(nextItemId)
    ]);

    item.value = {
      ...loadedItem,
      MediaSources: playback.MediaSources,
      MediaStreams: playback.MediaSources[0]?.MediaStreams || loadedItem.MediaStreams
    };
    currentSourceIndex.value = 0;
    playSessionId.value = playback.PlaySessionId;
    touchOverlay();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : String(loadError);
    item.value = null;
  } finally {
    loading.value = false;
  }
}

function toTicks(seconds: number) {
  return Math.max(0, Math.round(seconds * 10_000_000));
}

function playedToCompletion() {
  const player = videoRef.value;
  if (!player || !player.duration || Number.isNaN(player.duration)) {
    return false;
  }
  return player.currentTime / player.duration >= 0.9;
}

async function reportProgress(isPaused = false) {
  if (!item.value || !playSessionId.value || !videoRef.value) return;
  await api.playbackProgress({
    ItemId: item.value.Id,
    PlaySessionId: playSessionId.value,
    MediaSourceId: currentSource.value?.Id,
    PositionTicks: toTicks(videoRef.value.currentTime),
    IsPaused: isPaused,
    PlayedToCompletion: false
  });
}

async function stopPlayback(forceCompleted = false) {
  if (!item.value || !playSessionId.value || hasStopped || !videoRef.value) return;
  hasStopped = true;
  await api.playbackStopped({
    ItemId: item.value.Id,
    PlaySessionId: playSessionId.value,
    MediaSourceId: currentSource.value?.Id,
    PositionTicks: toTicks(videoRef.value.currentTime),
    IsPaused: paused.value,
    PlayedToCompletion: forceCompleted || playedToCompletion()
  });
}

async function handlePlay() {
  paused.value = false;
  touchOverlay();
  if (!item.value || !playSessionId.value) return;
  if (!hasStarted) {
    hasStarted = true;
    await api.playbackStarted({
      ItemId: item.value.Id,
      PlaySessionId: playSessionId.value,
      MediaSourceId: currentSource.value?.Id,
      PositionTicks: toTicks(videoRef.value?.currentTime || 0)
    });
  } else {
    await reportProgress(false);
  }
}

async function handlePause() {
  paused.value = true;
  touchOverlay();
  await reportProgress(true);
}

async function handleEnded() {
  paused.value = true;
  await stopPlayback(true);
}

function handleTimeUpdate() {
  const player = videoRef.value;
  if (!player) return;
  const second = Math.floor(player.currentTime);
  if (second - lastProgressSecond >= 10) {
    lastProgressSecond = second;
    void reportProgress(false);
  }
}

function skip(seconds: number) {
  const player = videoRef.value;
  if (!player) return;
  player.currentTime = Math.max(
    0,
    Math.min(player.duration || Infinity, player.currentTime + seconds)
  );
}

async function closePlayer() {
  await stopPlayback();
  await router.replace(item.value ? itemRoute(item.value) : '/');
}

function togglePlayback() {
  const player = videoRef.value;
  if (!player) return;
  if (player.paused) {
    void player.play();
  } else {
    player.pause();
  }
}

function touchOverlay() {
  overlayVisible.value = true;
  window.clearTimeout(overlayTimer);
  if (!paused.value) {
    overlayTimer = window.setTimeout(() => {
      overlayVisible.value = false;
    }, 3500);
  }
}
</script>

<template>
  <div
    v-if="loading"
    class="fixed inset-0 z-50 flex flex-col items-center justify-center gap-3 bg-black text-white"
  >
    <UProgress animation="carousel" class="w-48" />
    <p class="text-sm opacity-80">正在准备播放器…</p>
  </div>

  <div
    v-else-if="error || !item"
    class="fixed inset-0 z-50 flex flex-col items-center justify-center gap-3 bg-black text-white"
  >
    <UIcon name="i-lucide-circle-alert" class="size-12 opacity-80" />
    <p class="text-sm opacity-70">播放失败</p>
    <h2 class="text-xl font-semibold">{{ error || '没有找到播放内容' }}</h2>
    <UButton color="neutral" variant="subtle" icon="i-lucide-arrow-left" @click="router.back()">返回</UButton>
  </div>

  <div
    v-else
    class="relative h-screen w-screen overflow-hidden bg-black text-white"
    @mousemove="touchOverlay"
    @touchstart.passive="touchOverlay"
  >
    <video
      ref="videoRef"
      class="absolute inset-0 h-full w-full bg-black object-contain"
      :src="sourceUrl"
      :poster="posterImage"
      controls
      autoplay
      playsinline
      @play="handlePlay"
      @pause="handlePause"
      @ended="handleEnded"
      @timeupdate="handleTimeUpdate"
    >
      <track
        v-for="track in subtitleTracks"
        :key="track.key"
        kind="subtitles"
        :label="track.label"
        :src="track.src"
        :srclang="track.srclang"
      />
    </video>

    <transition
      enter-active-class="transition-opacity duration-200"
      leave-active-class="transition-opacity duration-500"
      enter-from-class="opacity-0"
      leave-to-class="opacity-0"
    >
      <div
        v-show="overlayVisible"
        class="pointer-events-none absolute inset-0 flex flex-col justify-between"
      >
        <div
          class="pointer-events-auto flex items-start justify-between gap-4 bg-gradient-to-b from-black/70 via-black/40 to-transparent p-4 sm:p-6"
        >
          <div class="min-w-0">
            <p class="text-xs uppercase tracking-wider text-white/60">
              {{ item.SeriesName || item.SeasonName || item.Type }}
            </p>
            <h2 class="mt-1 truncate text-2xl font-semibold">{{ item.Name }}</h2>
          </div>
          <UButton color="neutral" variant="solid" icon="i-lucide-x" @click="closePlayer">关闭</UButton>
        </div>

        <div
          class="pointer-events-auto flex flex-col gap-4 bg-gradient-to-t from-black/80 via-black/50 to-transparent p-4 sm:p-6"
        >
          <div class="flex items-center justify-center gap-3">
            <UButton
              color="neutral"
              variant="solid"
              size="lg"
              icon="i-lucide-rewind"
              @click="skip(-10)"
            >
              -10s
            </UButton>
            <UButton
              color="primary"
              variant="solid"
              size="xl"
              :icon="paused ? 'i-lucide-play' : 'i-lucide-pause'"
              @click="togglePlayback"
            >
              {{ paused ? '播放' : '暂停' }}
            </UButton>
            <UButton
              color="neutral"
              variant="solid"
              size="lg"
              icon="i-lucide-fast-forward"
              @click="skip(10)"
            >
              +10s
            </UButton>
          </div>

          <div class="grid gap-3 md:grid-cols-2">
            <div class="rounded-xl border border-white/10 bg-black/40 p-3 backdrop-blur">
              <h3 class="mb-2 text-xs font-semibold uppercase tracking-wider text-white/70">播放源</h3>
              <div class="flex flex-wrap gap-2">
                <UButton
                  v-for="(source, index) in item.MediaSources"
                  :key="source.Id"
                  :color="currentSourceIndex === index ? 'primary' : 'neutral'"
                  :variant="currentSourceIndex === index ? 'solid' : 'subtle'"
                  size="xs"
                  @click="currentSourceIndex = index"
                >
                  {{ source.Container || `版本 ${index + 1}` }}
                </UButton>
              </div>
              <p v-if="currentSource?.Size" class="mt-2 text-xs text-white/60">
                大小：{{ fileSize(currentSource.Size) }}
              </p>
            </div>

            <div class="rounded-xl border border-white/10 bg-black/40 p-3 backdrop-blur">
              <h3 class="mb-2 text-xs font-semibold uppercase tracking-wider text-white/70">媒体流</h3>
              <div class="space-y-1 text-xs">
                <div
                  v-for="stream in currentStreams"
                  :key="`${stream.Type}-${stream.Index}`"
                  class="flex gap-2"
                >
                  <span class="w-12 shrink-0 text-white/50">{{ streamLabel(stream.Type) }}</span>
                  <span class="text-white/80">{{ streamText(stream) || '默认轨道' }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </transition>
  </div>
</template>
