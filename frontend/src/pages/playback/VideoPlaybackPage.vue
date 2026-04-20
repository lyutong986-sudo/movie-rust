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
  currentSource.value ? api.streamUrlForSource(currentSource.value) : item.value ? api.streamUrl(item.value) : ''
);
const posterImage = computed(() =>
  item.value ? api.backdropUrl(item.value) || api.itemImageUrl(item.value) : ''
);
const currentStreams = computed(() => currentSource.value?.MediaStreams || item.value?.MediaStreams || []);
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
      // Ignore autoplay failures; native controls remain available.
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
  if (!item.value || !playSessionId.value || !videoRef.value) {
    return;
  }

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
  if (!item.value || !playSessionId.value || hasStopped || !videoRef.value) {
    return;
  }

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

  if (!item.value || !playSessionId.value) {
    return;
  }

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
  if (!player) {
    return;
  }

  const second = Math.floor(player.currentTime);
  if (second - lastProgressSecond >= 10) {
    lastProgressSecond = second;
    void reportProgress(false);
  }
}

function skip(seconds: number) {
  const player = videoRef.value;
  if (!player) {
    return;
  }

  player.currentTime = Math.max(0, Math.min(player.duration || Infinity, player.currentTime + seconds));
}

async function closePlayer() {
  await stopPlayback();
  await router.replace(item.value ? itemRoute(item.value) : '/');
}

function togglePlayback() {
  const player = videoRef.value;
  if (!player) {
    return;
  }

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
  <section v-if="loading" class="player-loading">
    <p>正在准备播放器</p>
  </section>

  <section v-else-if="error || !item" class="player-loading">
    <div>
      <p>播放失败</p>
      <h2>{{ error || '没有找到播放内容' }}</h2>
      <button type="button" @click="router.back()">返回</button>
    </div>
  </section>

  <section
    v-else
    class="player-shell video-player-shell"
    @mousemove="touchOverlay"
    @touchstart.passive="touchOverlay"
  >
    <video
      ref="videoRef"
      class="video-surface"
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

    <div class="player-overlay" :class="{ hidden: !overlayVisible }">
      <div class="player-topbar">
        <div>
          <p>{{ item.SeriesName || item.SeasonName || item.Type }}</p>
          <h2>{{ item.Name }}</h2>
        </div>
        <div class="button-row">
          <button class="secondary" type="button" @click="closePlayer">关闭</button>
        </div>
      </div>

      <div class="player-bottombar">
        <div class="button-row">
          <button class="secondary" type="button" @click="skip(-10)">-10s</button>
          <button type="button" @click="togglePlayback">{{ paused ? '播放' : '暂停' }}</button>
          <button class="secondary" type="button" @click="skip(10)">+10s</button>
        </div>

        <div class="player-panels">
          <div class="player-panel">
            <h3>播放源</h3>
            <div class="admin-tabs">
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
            <p v-if="currentSource?.Size" class="panel-note">大小：{{ fileSize(currentSource.Size) }}</p>
          </div>

          <div class="player-panel">
            <h3>媒体流</h3>
            <div class="streams compact">
              <div v-for="stream in currentStreams" :key="`${stream.Type}-${stream.Index}`">
                <strong>{{ streamLabel(stream.Type) }}</strong>
                <span>{{ streamText(stream) || '默认轨道' }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
