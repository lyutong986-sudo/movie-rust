<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type Player from 'video.js/dist/types/player';
import type Hls from 'hls.js';
import type { BaseItemDto, MediaStreamDto } from '../../api/emby';
import {
  api,
  fileSize,
  nextInQueue,
  playQueue,
  playQueueIndex,
  streamLabel,
  streamText
} from '../../store/app';
import { itemRoute, playbackRoute } from '../../utils/navigation';

const route = useRoute();
const router = useRouter();

const videoRef = ref<HTMLVideoElement | null>(null);
const playerRef = ref<Player | null>(null);
const hlsRef = ref<Hls | null>(null);
type VideoJsFactory = typeof import('video.js')['default'];
type HlsConstructor = typeof import('hls.js')['default'];
const videoJsFactoryRef = ref<VideoJsFactory | null>(null);
const hlsConstructorRef = ref<HlsConstructor | null>(null);
const playbackEngineReady = ref(false);
const loading = ref(false);
const error = ref('');
const item = ref<BaseItemDto | null>(null);
const currentSourceIndex = ref(0);
const playSessionId = ref('');
const overlayVisible = ref(true);
const paused = ref(true);

const currentTime = ref(0);
const duration = ref(0);
const buffered = ref(0);
const volume = ref(1);
const muted = ref(false);
const playbackRate = ref(1);
const fullscreen = ref(false);
const pip = ref(false);
const selectedAudioIndex = ref<number | null>(null);
const selectedSubtitleIndex = ref<number | null>(null);
const activeSourceCandidate = ref(0);
const nextUpEpisode = ref<BaseItemDto | null>(null);
const seekPreview = ref<{ x: number; time: number } | null>(null);

let overlayTimer = 0;
let hasStarted = false;
let hasStopped = false;
let lastProgressSecond = -10;

const itemId = computed(() => String(route.query.itemId || ''));
const currentSource = computed(() => item.value?.MediaSources?.[currentSourceIndex.value]);
const directSourceUrl = computed(() =>
  currentSource.value
    ? api.streamUrlForSource(currentSource.value)
    : item.value
    ? api.streamUrl(item.value)
    : ''
);
const hlsSourceUrl = computed(() => {
  if (!item.value) return '';
  return api.hlsUrlForSource(item.value.Id, currentSource.value, playSessionId.value);
});
const sourceCandidates = computed(() => {
  const candidates: Array<{ src: string; type: string }> = [];
  if (hlsSourceUrl.value) {
    candidates.push({ src: hlsSourceUrl.value, type: 'application/x-mpegURL' });
  }
  if (directSourceUrl.value) {
    candidates.push({ src: directSourceUrl.value, type: 'video/mp4' });
  }
  return candidates;
});
const posterImage = computed(() =>
  item.value ? api.backdropUrl(item.value) || api.itemImageUrl(item.value) : ''
);
const currentStreams = computed(
  () => currentSource.value?.MediaStreams || item.value?.MediaStreams || []
);
const audioStreams = computed(() => currentStreams.value.filter((s) => s.Type === 'Audio'));
const subtitleStreams = computed(() => currentStreams.value.filter((s) => s.Type === 'Subtitle'));
const subtitleTracks = computed(() =>
  subtitleStreams.value
    .filter((stream) => stream.DeliveryUrl)
    .map((stream) => ({
      key: `${stream.Type}-${stream.Index}`,
      index: stream.Index,
      label: stream.DisplayTitle || `字幕 ${stream.Index}`,
      src: api.subtitleUrl(stream.DeliveryUrl),
      srclang: (stream.Language || 'und').toLowerCase()
    }))
);

// Chapter markers
const chapterMarkers = computed(() => {
  const chapters = item.value?.Chapters;
  if (!chapters?.length || !duration.value) return [];
  const totalSec = duration.value;
  return chapters.map((c, i) => ({
    name: c.Name || `章节 ${i + 1}`,
    seconds: c.StartPositionTicks / 10_000_000,
    percent: Math.min(100, ((c.StartPositionTicks / 10_000_000) / totalSec) * 100),
    index: i
  }));
});

// Intro skip：简单启发 — 若存在名为 "Intro"/"片头"/"Opening" 的 chapter，且当前时间在其范围内，出现 skip。
const introChapter = computed(() => {
  const markers = chapterMarkers.value;
  if (!markers.length) return null;
  for (let i = 0; i < markers.length; i += 1) {
    const name = markers[i].name.toLowerCase();
    if (/intro|opening|片头|op$/i.test(name)) {
      const start = markers[i].seconds;
      const end = markers[i + 1]?.seconds ?? start + 90;
      return { start, end };
    }
  }
  return null;
});
const showSkipIntro = computed(() => {
  const intro = introChapter.value;
  if (!intro) return false;
  return currentTime.value >= intro.start && currentTime.value < Math.max(intro.end - 2, intro.start);
});

const showNextUp = computed(() => {
  if (!nextUpEpisode.value) return false;
  if (!duration.value) return false;
  return duration.value - currentTime.value <= 25 && duration.value - currentTime.value > 0;
});

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
    if (!videoRef.value) return;
    activeSourceCandidate.value = 0;
    await applyPlaybackSource(videoRef.value.currentTime);
  }
);

onMounted(async () => {
  await ensurePlaybackEngines();
  initVideoJsPlayer();
  if (sourceCandidates.value.length) {
    await applyPlaybackSource(0);
  }
  document.addEventListener('fullscreenchange', onFullscreenChange);
  window.addEventListener('keydown', onKeyDown);
});

onBeforeUnmount(async () => {
  window.clearTimeout(overlayTimer);
  document.removeEventListener('fullscreenchange', onFullscreenChange);
  window.removeEventListener('keydown', onKeyDown);
  destroyHls();
  playerRef.value?.dispose();
  playerRef.value = null;
  await stopPlayback();
});

async function ensurePlaybackEngines() {
  if (playbackEngineReady.value) return;
  const [{ default: videojs }, { default: HlsCtor }] = await Promise.all([
    import('video.js'),
    import('hls.js'),
    import('video.js/dist/video-js.css')
  ]);
  videoJsFactoryRef.value = videojs;
  hlsConstructorRef.value = HlsCtor;
  playbackEngineReady.value = true;
}

function initVideoJsPlayer() {
  if (playerRef.value || !videoRef.value) return;
  const videojs = videoJsFactoryRef.value;
  if (!videojs) return;
  const createPlayer = videojs as unknown as (element: HTMLVideoElement, options: unknown) => Player;
  playerRef.value = createPlayer(videoRef.value, {
    autoplay: true,
    controls: false,
    preload: 'auto',
    muted: false,
    html5: {
      vhs: {
        overrideNative: true
      }
    }
  });
}

function destroyHls() {
  if (hlsRef.value) {
    hlsRef.value.destroy();
    hlsRef.value = null;
  }
}

async function applyPlaybackSource(keepTime = 0) {
  await ensurePlaybackEngines();
  const player = playerRef.value;
  const media = videoRef.value;
  const candidate = sourceCandidates.value[activeSourceCandidate.value];
  const HlsCtor = hlsConstructorRef.value;
  if (!player || !media || !candidate) return;

  destroyHls();
  media.pause();
  media.currentTime = Math.max(0, keepTime);

  if (candidate.type === 'application/x-mpegURL' && HlsCtor?.isSupported()) {
    const hls = new HlsCtor({
      enableWorker: true
    });
    hlsRef.value = hls;
    hls.loadSource(candidate.src);
    hls.attachMedia(media);
    hls.on(HlsCtor.Events.ERROR, (_event, data) => {
      if (data.fatal) {
        void tryNextSource(data.details || 'hls_fatal_error');
      }
    });
    hls.on(HlsCtor.Events.MANIFEST_PARSED, () => {
      void media.play().catch(() => {
        // 自动播放被策略拦截时保持静默，等待用户交互
      });
    });
    return;
  }

  player.src({
    src: candidate.src,
    type: candidate.type
  });
  try {
    await media.play();
  } catch {
    // 自动播放被策略拦截时保持静默，等待用户交互
  }
}

async function tryNextSource(reason = '') {
  if (activeSourceCandidate.value + 1 >= sourceCandidates.value.length) {
    if (reason) {
      error.value = `播放失败：${reason}`;
    }
    return;
  }
  activeSourceCandidate.value += 1;
  await applyPlaybackSource(currentTime.value);
}

async function loadPlayback(nextItemId: string) {
  loading.value = true;
  error.value = '';
  hasStarted = false;
  hasStopped = false;
  lastProgressSecond = -10;
  nextUpEpisode.value = null;

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
    activeSourceCandidate.value = 0;
    playSessionId.value = playback.PlaySessionId;

    // 默认字幕 / 音频
    selectedAudioIndex.value =
      playback.MediaSources[0]?.DefaultAudioStreamIndex ?? audioStreams.value[0]?.Index ?? null;
    selectedSubtitleIndex.value = playback.MediaSources[0]?.DefaultSubtitleStreamIndex ?? null;

    // 如果是剧集，拉取下一集
    if (loadedItem.Type === 'Episode' && loadedItem.SeriesId) {
      try {
        const nextUp = await api.nextUp(loadedItem.SeriesId, 1);
        const candidate = nextUp.Items?.[0];
        if (candidate && candidate.Id !== loadedItem.Id) {
          nextUpEpisode.value = candidate;
        }
      } catch {
        // ignore
      }
    } else if (playQueue.value[playQueueIndex.value + 1]) {
      nextUpEpisode.value = playQueue.value[playQueueIndex.value + 1];
    }

    touchOverlay();
    if (videoRef.value) {
      initVideoJsPlayer();
      await applyPlaybackSource(0);
    }
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
  if (!player || !player.duration || Number.isNaN(player.duration)) return false;
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

async function onPlay() {
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

async function onPause() {
  paused.value = true;
  touchOverlay();
  await reportProgress(true);
}

async function onEnded() {
  paused.value = true;
  await stopPlayback(true);
  await playNextIfAny();
}

function onTimeUpdate() {
  const player = videoRef.value;
  if (!player) return;
  currentTime.value = player.currentTime;
  duration.value = player.duration || 0;
  if (player.buffered.length) {
    buffered.value = player.buffered.end(player.buffered.length - 1);
  }
  const second = Math.floor(player.currentTime);
  if (second - lastProgressSecond >= 10) {
    lastProgressSecond = second;
    void reportProgress(false);
  }
}

function onVolumeChange() {
  const player = videoRef.value;
  if (!player) return;
  volume.value = player.volume;
  muted.value = player.muted;
}

function onRateChange() {
  const player = videoRef.value;
  if (!player) return;
  playbackRate.value = player.playbackRate;
}

function onEnterPip() {
  pip.value = true;
}
function onLeavePip() {
  pip.value = false;
}

function onFullscreenChange() {
  fullscreen.value = Boolean(document.fullscreenElement);
}

function skip(seconds: number) {
  const player = videoRef.value;
  if (!player) return;
  player.currentTime = Math.max(
    0,
    Math.min(player.duration || Infinity, player.currentTime + seconds)
  );
}

function seekTo(seconds: number) {
  const player = videoRef.value;
  if (!player) return;
  player.currentTime = Math.max(0, Math.min(player.duration || seconds, seconds));
}

function toggleMute() {
  const player = videoRef.value;
  if (!player) return;
  player.muted = !player.muted;
}
function setVolume(v: number) {
  const player = videoRef.value;
  if (!player) return;
  player.muted = v === 0;
  player.volume = Math.max(0, Math.min(1, v));
}

function setRate(rate: number) {
  const player = videoRef.value;
  if (!player) return;
  player.playbackRate = rate;
  playbackRate.value = rate;
}

async function togglePip() {
  const player = videoRef.value;
  if (!player || !('requestPictureInPicture' in player)) return;
  try {
    if (document.pictureInPictureElement) {
      await document.exitPictureInPicture();
    } else {
      await player.requestPictureInPicture();
    }
  } catch {
    // ignore
  }
}

async function toggleFullscreen() {
  try {
    if (document.fullscreenElement) {
      await document.exitFullscreen();
    } else {
      await document.documentElement.requestFullscreen();
    }
  } catch {
    // ignore
  }
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

async function closePlayer() {
  await stopPlayback();
  await router.replace(item.value ? itemRoute(item.value) : '/');
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

function selectSubtitle(index: number | null) {
  selectedSubtitleIndex.value = index;
  const player = videoRef.value;
  if (!player) return;
  Array.from(player.textTracks).forEach((track) => {
    // 内部 textTracks 顺序与 subtitleTracks 对应
    track.mode = 'disabled';
  });
  if (index === null) return;
  const trackPos = subtitleTracks.value.findIndex((t) => t.index === index);
  if (trackPos >= 0 && player.textTracks[trackPos]) {
    player.textTracks[trackPos].mode = 'showing';
  }
}

function selectAudio(index: number) {
  selectedAudioIndex.value = index;
  // 原生 <video> 对 audioTracks 支持有限；暂时仅记录，后端切换时会用到。
}

function skipIntro() {
  if (introChapter.value) {
    seekTo(introChapter.value.end);
  }
}

async function playNextIfAny() {
  if (nextUpEpisode.value) {
    const next = nextUpEpisode.value;
    await router.replace(playbackRoute(next));
    return;
  }
  const next = nextInQueue();
  if (next) {
    await router.replace(playbackRoute(next));
  }
}

function onProgressPointerDown(e: PointerEvent) {
  const el = e.currentTarget as HTMLElement;
  el.setPointerCapture(e.pointerId);
  seekFromEvent(e, el);
}
function onProgressPointerMove(e: PointerEvent) {
  const el = e.currentTarget as HTMLElement;
  const rect = el.getBoundingClientRect();
  const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
  const ratio = x / rect.width;
  const time = ratio * duration.value;
  seekPreview.value = { x, time };
  if (e.buttons === 1) {
    seekTo(time);
  }
}
function onProgressPointerUp(e: PointerEvent) {
  const el = e.currentTarget as HTMLElement;
  el.releasePointerCapture(e.pointerId);
  seekPreview.value = null;
}
function onProgressLeave() {
  seekPreview.value = null;
}
function seekFromEvent(e: PointerEvent, el: HTMLElement) {
  const rect = el.getBoundingClientRect();
  const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
  seekTo(ratio * duration.value);
}

// 键盘快捷键 — 参考 YouTube / Jellyfin
function onKeyDown(e: KeyboardEvent) {
  if ((e.target as HTMLElement)?.tagName === 'INPUT' || (e.target as HTMLElement)?.isContentEditable) {
    return;
  }
  switch (e.key) {
    case ' ':
    case 'k':
    case 'K':
      e.preventDefault();
      togglePlayback();
      touchOverlay();
      break;
    case 'ArrowLeft':
      e.preventDefault();
      skip(e.shiftKey ? -30 : -10);
      touchOverlay();
      break;
    case 'ArrowRight':
      e.preventDefault();
      skip(e.shiftKey ? 30 : 10);
      touchOverlay();
      break;
    case 'ArrowUp':
      e.preventDefault();
      setVolume(Math.min(1, volume.value + 0.05));
      touchOverlay();
      break;
    case 'ArrowDown':
      e.preventDefault();
      setVolume(Math.max(0, volume.value - 0.05));
      touchOverlay();
      break;
    case 'm':
    case 'M':
      toggleMute();
      touchOverlay();
      break;
    case 'f':
    case 'F':
      void toggleFullscreen();
      break;
    case 'c':
    case 'C':
      e.preventDefault();
      if (selectedSubtitleIndex.value === null && subtitleTracks.value[0]) {
        selectSubtitle(subtitleTracks.value[0].index);
      } else {
        selectSubtitle(null);
      }
      touchOverlay();
      break;
    case 'n':
    case 'N':
      if (nextUpEpisode.value) {
        void playNextIfAny();
      }
      break;
    case 'p':
    case 'P':
      void togglePip();
      break;
    case 'Escape':
      void closePlayer();
      break;
    default:
      if (e.key >= '0' && e.key <= '9') {
        const ratio = Number(e.key) / 10;
        seekTo((duration.value || 0) * ratio);
        touchOverlay();
      }
  }
}

function fmt(seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00';
  const total = Math.floor(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(h > 0 ? 2 : 1, '0');
  const ss = String(s).padStart(2, '0');
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

const progressPercent = computed(() => {
  if (!duration.value) return 0;
  return (currentTime.value / duration.value) * 100;
});
const bufferedPercent = computed(() => {
  if (!duration.value) return 0;
  return (buffered.value / duration.value) * 100;
});

const rates = [0.5, 0.75, 1, 1.25, 1.5, 2];

const subtitleMenu = computed(() => [
  [
    {
      label: '关闭字幕',
      icon: 'i-lucide-subtitles',
      onSelect: () => selectSubtitle(null),
      kbd: selectedSubtitleIndex.value === null ? ['✓'] : undefined
    }
  ],
  subtitleTracks.value.map((t) => ({
    label: t.label,
    icon: 'i-lucide-captions',
    onSelect: () => selectSubtitle(t.index),
    kbd: selectedSubtitleIndex.value === t.index ? ['✓'] : undefined
  }))
]);

const audioMenu = computed(() =>
  audioStreams.value.map((s) => ({
    label: streamText(s as MediaStreamDto),
    icon: 'i-lucide-speaker',
    onSelect: () => selectAudio(s.Index),
    kbd: selectedAudioIndex.value === s.Index ? ['✓'] : undefined
  }))
);

const rateMenu = computed(() =>
  rates.map((r) => ({
    label: `${r}x`,
    icon: 'i-lucide-gauge',
    onSelect: () => setRate(r),
    kbd: playbackRate.value === r ? ['✓'] : undefined
  }))
);
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
    :class="{ 'cursor-none': !overlayVisible && !paused }"
    @mousemove="touchOverlay"
    @touchstart.passive="touchOverlay"
    @click.self="togglePlayback"
  >
    <video
      ref="videoRef"
      class="video-js absolute inset-0 h-full w-full bg-black object-contain"
      :poster="posterImage"
      autoplay
      playsinline
      @play="onPlay"
      @pause="onPause"
      @ended="onEnded"
      @timeupdate="onTimeUpdate"
      @volumechange="onVolumeChange"
      @ratechange="onRateChange"
      @enterpictureinpicture="onEnterPip"
      @leavepictureinpicture="onLeavePip"
      @error="tryNextSource('media_error')"
      @click="togglePlayback"
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

    <!-- Skip intro -->
    <transition name="fade">
      <button
        v-if="showSkipIntro"
        type="button"
        class="absolute bottom-28 right-8 z-20 flex items-center gap-2 rounded-lg bg-white/90 px-5 py-3 text-sm font-semibold text-black shadow-lg hover:bg-white"
        @click="skipIntro"
      >
        <UIcon name="i-lucide-skip-forward" class="size-4" />
        跳过片头
      </button>
    </transition>

    <!-- Next up -->
    <transition name="fade">
      <div
        v-if="showNextUp"
        class="absolute bottom-28 right-8 z-20 flex items-center gap-3 rounded-xl bg-black/80 p-3 pr-4 ring-1 ring-white/20 backdrop-blur"
      >
        <div class="relative h-16 w-28 flex-shrink-0 overflow-hidden rounded">
          <img
            v-if="api.backdropUrl(nextUpEpisode!) || api.itemImageUrl(nextUpEpisode!)"
            :src="api.backdropUrl(nextUpEpisode!) || api.itemImageUrl(nextUpEpisode!)"
            :alt="nextUpEpisode!.Name"
            class="h-full w-full object-cover"
          />
        </div>
        <div class="min-w-0 max-w-xs">
          <p class="text-xs uppercase tracking-wider text-white/60">即将播放</p>
          <p class="truncate text-sm font-semibold">{{ nextUpEpisode!.Name }}</p>
          <UButton size="xs" class="mt-2" icon="i-lucide-skip-forward" @click="playNextIfAny">
            立即播放 (N)
          </UButton>
        </div>
      </div>
    </transition>

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
        <!-- 顶部 -->
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

        <!-- 底部控件 -->
        <div
          class="pointer-events-auto flex flex-col gap-3 bg-gradient-to-t from-black/85 via-black/55 to-transparent px-4 pb-4 pt-8 sm:px-8"
        >
          <!-- 进度条 -->
          <div
            class="group relative h-2 cursor-pointer rounded-full bg-white/20"
            @pointerdown="onProgressPointerDown"
            @pointermove="onProgressPointerMove"
            @pointerup="onProgressPointerUp"
            @pointerleave="onProgressLeave"
          >
            <div
              class="absolute inset-y-0 left-0 rounded-full bg-white/30"
              :style="{ width: `${bufferedPercent}%` }"
            />
            <div
              class="bg-primary absolute inset-y-0 left-0 rounded-full"
              :style="{ width: `${progressPercent}%` }"
            />
            <!-- 章节标记 -->
            <div
              v-for="marker in chapterMarkers"
              :key="marker.index"
              class="absolute inset-y-0 w-0.5 bg-white/50"
              :style="{ left: `calc(${marker.percent}% - 1px)` }"
              :title="marker.name"
            />
            <div
              class="absolute top-1/2 size-3.5 -translate-y-1/2 rounded-full bg-white opacity-0 shadow transition-opacity group-hover:opacity-100"
              :style="{ left: `calc(${progressPercent}% - 7px)` }"
            />
            <!-- 悬浮时间提示 -->
            <div
              v-if="seekPreview"
              class="pointer-events-none absolute bottom-5 -translate-x-1/2 rounded bg-black/80 px-2 py-1 text-xs text-white shadow"
              :style="{ left: `${seekPreview.x}px` }"
            >
              {{ fmt(seekPreview.time) }}
            </div>
          </div>

          <!-- 控件行 -->
          <div class="flex flex-wrap items-center justify-between gap-3">
            <div class="flex items-center gap-2">
              <UButton
                color="neutral"
                variant="ghost"
                :icon="paused ? 'i-lucide-play' : 'i-lucide-pause'"
                class="!text-white"
                @click="togglePlayback"
                aria-label="播放/暂停"
              />
              <UButton
                color="neutral"
                variant="ghost"
                icon="i-lucide-rewind"
                class="!text-white"
                @click="skip(-10)"
                aria-label="后退 10 秒"
              />
              <UButton
                color="neutral"
                variant="ghost"
                icon="i-lucide-fast-forward"
                class="!text-white"
                @click="skip(10)"
                aria-label="前进 10 秒"
              />
              <UButton
                v-if="nextUpEpisode"
                color="neutral"
                variant="ghost"
                icon="i-lucide-skip-forward"
                class="!text-white"
                @click="playNextIfAny"
                aria-label="下一集"
              />

              <!-- 音量 -->
              <div class="group ms-2 flex items-center">
                <UButton
                  color="neutral"
                  variant="ghost"
                  :icon="
                    muted || volume === 0
                      ? 'i-lucide-volume-x'
                      : volume < 0.5
                      ? 'i-lucide-volume-1'
                      : 'i-lucide-volume-2'
                  "
                  class="!text-white"
                  @click="toggleMute"
                />
                <input
                  type="range"
                  class="h-1 w-0 cursor-pointer appearance-none rounded bg-white/30 opacity-0 transition-all duration-200 group-hover:w-20 group-hover:opacity-100"
                  min="0"
                  max="1"
                  step="0.01"
                  :value="muted ? 0 : volume"
                  @input="(e) => setVolume(Number((e.target as HTMLInputElement).value))"
                />
              </div>

              <span class="ms-2 hidden text-sm tabular-nums sm:inline">
                {{ fmt(currentTime) }}
                <span class="text-white/50"> / {{ fmt(duration) }}</span>
              </span>
            </div>

            <div class="flex items-center gap-1">
              <!-- 播放源 -->
              <UDropdownMenu v-if="item.MediaSources && item.MediaSources.length > 1">
                <UButton color="neutral" variant="ghost" class="!text-white" icon="i-lucide-layers">
                  源
                </UButton>
                <template #content>
                  <div class="p-1 min-w-40">
                    <button
                      v-for="(source, idx) in item.MediaSources"
                      :key="source.Id"
                      type="button"
                      class="hover:bg-elevated flex w-full items-center justify-between gap-3 rounded px-2 py-1.5 text-left text-sm"
                      @click="currentSourceIndex = idx"
                    >
                      <span class="truncate">
                        {{ source.Container || `版本 ${idx + 1}` }}
                      </span>
                      <span class="text-muted text-xs">{{ fileSize(source.Size) }}</span>
                      <UIcon v-if="idx === currentSourceIndex" name="i-lucide-check" class="text-primary size-4" />
                    </button>
                  </div>
                </template>
              </UDropdownMenu>

              <!-- 字幕 -->
              <UDropdownMenu :items="subtitleMenu">
                <UButton
                  color="neutral"
                  variant="ghost"
                  icon="i-lucide-captions"
                  class="!text-white"
                  aria-label="字幕"
                />
              </UDropdownMenu>

              <!-- 音轨 -->
              <UDropdownMenu v-if="audioStreams.length > 1" :items="audioMenu">
                <UButton
                  color="neutral"
                  variant="ghost"
                  icon="i-lucide-audio-lines"
                  class="!text-white"
                  aria-label="音轨"
                />
              </UDropdownMenu>

              <!-- 倍速 -->
              <UDropdownMenu :items="rateMenu">
                <UButton color="neutral" variant="ghost" class="!text-white">
                  {{ playbackRate }}x
                </UButton>
              </UDropdownMenu>

              <!-- PiP -->
              <UButton
                color="neutral"
                variant="ghost"
                icon="i-lucide-picture-in-picture-2"
                class="!text-white"
                @click="togglePip"
                aria-label="画中画"
              />

              <!-- 全屏 -->
              <UButton
                color="neutral"
                variant="ghost"
                :icon="fullscreen ? 'i-lucide-minimize-2' : 'i-lucide-maximize-2'"
                class="!text-white"
                @click="toggleFullscreen"
                aria-label="全屏"
              />
            </div>
          </div>
        </div>
      </div>
    </transition>
  </div>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.25s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

input[type='range']::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 12px;
  height: 12px;
  background: white;
  border-radius: 50%;
  cursor: pointer;
}
</style>
