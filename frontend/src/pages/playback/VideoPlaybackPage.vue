<script setup lang="ts">
import Hls, { ErrorTypes, Events, type ErrorData } from 'hls.js';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { BaseItemDto, MediaStreamDto } from '../../api/emby';
import { api, fileSize, streamLabel, streamText } from '../../store/app';
import { itemRoute } from '../../utils/navigation';

enum PlaybackStatus {
  Stopped = 'Stopped',
  Playing = 'Playing',
  Paused = 'Paused',
  Buffering = 'Buffering',
  Error = 'Error'
}

type PlayerPanel = 'tracks' | 'settings' | 'info' | null;
type PlaybackEngine = 'hls' | 'mpegts' | 'native';

interface MpegtsRuntime {
  isSupported?: () => boolean;
  getFeatureList?: () => {
    msePlayback?: boolean;
    mseLivePlayback?: boolean;
  };
  createPlayer: (
    mediaDataSource: Record<string, unknown>,
    config?: Record<string, unknown>
  ) => MpegtsPlayer;
}

interface MpegtsPlayer {
  attachMediaElement: (element: HTMLMediaElement) => void;
  load: () => void;
  play?: () => Promise<void> | void;
  unload?: () => void;
  detachMediaElement?: () => void;
  destroy: () => void;
}

const route = useRoute();
const router = useRouter();

const videoContainerRef = ref<HTMLElement | null>(null);
const mediaElementRef = ref<HTMLVideoElement | null>(null);
const loading = ref(false);
const error = ref('');
const item = ref<BaseItemDto | null>(null);
const playSessionId = ref('');
const currentSourceIndex = ref(0);
const currentTime = ref(0);
const duration = ref(0);
const volume = ref(1);
const muted = ref(false);
const isFullscreen = ref(false);
const overlayOsd = shallowRef(true);
const panelMode = ref<PlayerPanel>(null);
const selectedSubtitleIndex = ref<number | null>(null);
const selectedAudioIndex = ref<number | null>(null);
const playbackRate = ref(1);
const status = ref<PlaybackStatus>(PlaybackStatus.Stopped);

let hls: Hls | null = null;
let mpegtsPlayer: MpegtsPlayer | null = null;
let overlayTimer = 0;
let hasStarted = false;
let hasStopped = false;
let lastProgressSecond = -10;
let pendingSeekSeconds = 0;
let rememberedVolume = 1;

const isPaused = computed(() => status.value === PlaybackStatus.Paused || status.value === PlaybackStatus.Stopped);
const isBuffering = computed(() => status.value === PlaybackStatus.Buffering);
const staticOverlay = computed(() => isPaused.value || Boolean(panelMode.value) || Boolean(error.value));
const overlay = computed({
  get: () => staticOverlay.value || overlayOsd.value,
  set: (value: boolean) => {
    overlayOsd.value = value;
  }
});

const currentSource = computed(() => item.value?.MediaSources?.[currentSourceIndex.value] || null);
const currentStreams = computed(() => currentSource.value?.MediaStreams || item.value?.MediaStreams || []);
const videoStream = computed(() => currentStreams.value.find((stream) => stream.Type === 'Video') || null);
const audioStreams = computed(() => currentStreams.value.filter((stream) => stream.Type === 'Audio'));
const subtitleStreams = computed(() =>
  currentStreams.value.filter((stream) => stream.Type === 'Subtitle' && stream.DeliveryUrl)
);
const currentSourceUrl = computed(() => {
  if (currentSource.value) {
    return api.streamUrlForSource(currentSource.value);
  }

  return item.value ? api.streamUrl(item.value) : '';
});
const posterUrl = computed(() => (item.value ? api.backdropUrl(item.value) || api.itemImageUrl(item.value) : ''));
const progressValue = computed(() =>
  duration.value > 0 ? Math.min(100, Math.max(0, (currentTime.value / duration.value) * 100)) : 0
);
const supportsPictureInPicture = computed(
  () => typeof document !== 'undefined' && 'pictureInPictureEnabled' in document && document.pictureInPictureEnabled
);
const titleLine = computed(() => {
  if (!item.value) {
    return '';
  }

  if (item.value.Type === 'Episode') {
    return item.value.Name;
  }

  return item.value.Name;
});
const subtitleLine = computed(() => {
  if (!item.value) {
    return '';
  }

  if (item.value.Type === 'Episode') {
    return [item.value.SeriesName, seasonEpisode(item.value.ParentIndexNumber, item.value.IndexNumber)]
      .filter(Boolean)
      .join(' · ');
  }

  return [item.value.ProductionYear, item.value.Type].filter(Boolean).join(' · ');
});
const endsAtText = computed(() => {
  if (!duration.value || currentTime.value >= duration.value) {
    return '';
  }

  const endTime = new Date(Date.now() + (duration.value - currentTime.value) * 1000);
  return `预计 ${String(endTime.getHours()).padStart(2, '0')}:${String(endTime.getMinutes()).padStart(2, '0')} 结束`;
});
const selectedSubtitleLabel = computed(() => {
  if (selectedSubtitleIndex.value === null) {
    return '关闭';
  }

  return subtitleStreams.value.find((stream) => stream.Index === selectedSubtitleIndex.value)?.DisplayTitle || '字幕';
});
const selectedAudioLabel = computed(() => {
  if (selectedAudioIndex.value === null) {
    return audioStreams.value[0]?.DisplayTitle || '默认';
  }

  return audioStreams.value.find((stream) => stream.Index === selectedAudioIndex.value)?.DisplayTitle || '默认';
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

watch(staticOverlay, (value) => {
  if (value) {
    window.clearTimeout(overlayTimer);
    overlay.value = true;
  } else {
    startOverlayTimeout();
  }
});

watch(currentSourceIndex, async () => {
  const player = mediaElementRef.value;
  if (!player || !currentSourceUrl.value) {
    return;
  }

  pendingSeekSeconds = player.currentTime;
  status.value = PlaybackStatus.Buffering;
  panelMode.value = null;
  await attachCurrentSource();
  await requestElementPlay();
});

watch(playbackRate, (value) => {
  if (mediaElementRef.value) {
    mediaElementRef.value.playbackRate = value;
  }
});

onMounted(() => {
  document.addEventListener('fullscreenchange', syncFullscreenState);
  window.addEventListener('keydown', handleKeydown);
});

onBeforeUnmount(async () => {
  window.clearTimeout(overlayTimer);
  document.removeEventListener('fullscreenchange', syncFullscreenState);
  window.removeEventListener('keydown', handleKeydown);
  destroyStreamingPlayers();
  await stopPlayback();
});

async function loadPlayback(itemId: string) {
  loading.value = true;
  error.value = '';
  status.value = PlaybackStatus.Buffering;
  panelMode.value = null;
  overlay.value = true;

  try {
    await stopPlayback();
    hasStarted = false;
    hasStopped = false;
    lastProgressSecond = -10;
    pendingSeekSeconds = 0;

    const [loadedItem, playbackInfo] = await Promise.all([api.item(itemId), api.playbackInfo(itemId)]);
    item.value = {
      ...loadedItem,
      MediaSources: playbackInfo.MediaSources,
      MediaStreams: playbackInfo.MediaSources[0]?.MediaStreams || loadedItem.MediaStreams
    };
    playSessionId.value = playbackInfo.PlaySessionId;
    currentSourceIndex.value = 0;
    duration.value = ticksToSeconds(playbackInfo.MediaSources[0]?.RunTimeTicks || loadedItem.RunTimeTicks);
    currentTime.value = 0;
    selectedAudioIndex.value = playbackInfo.MediaSources[0]?.DefaultAudioStreamIndex ?? null;
    selectedSubtitleIndex.value = playbackInfo.MediaSources[0]?.DefaultSubtitleStreamIndex ?? null;
    pendingSeekSeconds = ticksToSeconds(loadedItem.UserData?.PlaybackPositionTicks);

    await nextTick();
    syncVolumeState();
    await attachCurrentSource();
    await requestElementPlay();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : String(loadError);
    status.value = PlaybackStatus.Error;
  } finally {
    loading.value = false;
  }
}

async function attachCurrentSource() {
  const player = mediaElementRef.value;
  const url = currentSourceUrl.value;
  if (!player || !url) {
    return;
  }

  destroyStreamingPlayers();
  error.value = '';
  player.pause();
  player.removeAttribute('src');
  player.load();

  const engine = await detectPlaybackEngine(url);
  if (engine === 'hls' && Hls.isSupported() && !canPlayNativeHls(player)) {
    attachHls(player, url);
    return;
  }

  if (engine === 'mpegts') {
    const attached = await attachMpegts(player, url);
    if (attached) {
      return;
    }
  }

  player.src = url;
  player.load();
}

function attachHls(player: HTMLVideoElement, url: string) {
  const instance = new Hls({
    testBandwidth: false,
    lowLatencyMode: false,
    manifestLoadingTimeOut: 20000
  });
  hls = instance;
  instance.on(Events.ERROR, onHlsError);
  instance.attachMedia(player);
  instance.on(Events.MEDIA_ATTACHED, () => {
    instance.loadSource(url);
  });
}

async function attachMpegts(player: HTMLVideoElement, url: string) {
  const runtime = await loadMpegtsRuntime();
  if (!runtime) {
    return false;
  }

  const featureList = runtime.getFeatureList?.();
  const supported = runtime.isSupported?.() || featureList?.msePlayback || featureList?.mseLivePlayback;
  if (!supported) {
    return false;
  }

  const container = normalizedContainer();
  const type = container === 'flv' || /\.flv(?:$|[?#])/i.test(url) ? 'flv' : 'mpegts';
  mpegtsPlayer = runtime.createPlayer(
    {
      type,
      isLive: !currentSource.value?.RunTimeTicks,
      url
    },
    {
      enableWorker: true,
      lazyLoad: false,
      reuseRedirectedURL: true
    }
  );
  mpegtsPlayer.attachMediaElement(player);
  mpegtsPlayer.load();
  return true;
}

async function requestElementPlay() {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  try {
    await player.play();
  } catch {
    status.value = PlaybackStatus.Paused;
    overlay.value = true;
  }
}

function handleLoadedData() {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  if (pendingSeekSeconds > 0) {
    try {
      player.currentTime = Math.min(
        pendingSeekSeconds,
        Number.isFinite(player.duration) && player.duration > 0 ? player.duration : pendingSeekSeconds
      );
      currentTime.value = player.currentTime;
    } catch {
      // 某些浏览器在媒体尚未完全准备好时不允许立即 seek。
    } finally {
      pendingSeekSeconds = 0;
    }
  }

  status.value = player.paused ? PlaybackStatus.Paused : PlaybackStatus.Playing;
  applySubtitleSelection();
}

function handleCanPlay() {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  status.value = player.paused ? PlaybackStatus.Paused : PlaybackStatus.Playing;
}

function handlePlay() {
  status.value = PlaybackStatus.Playing;
  touchOverlay();

  if (!item.value || !playSessionId.value) {
    return;
  }

  if (!hasStarted) {
    hasStarted = true;
    void api
      .playbackStarted({
        ItemId: item.value.Id,
        PlaySessionId: playSessionId.value,
        MediaSourceId: currentSource.value?.Id,
        PositionTicks: toTicks(mediaElementRef.value?.currentTime || 0)
      })
      .catch(() => undefined);
  } else {
    void reportProgress(false).catch(() => undefined);
  }
}

function handlePause() {
  status.value = PlaybackStatus.Paused;
  touchOverlay();
  void reportProgress(true).catch(() => undefined);
}

function handleWaiting() {
  if (mediaElementRef.value?.paused) {
    return;
  }

  status.value = PlaybackStatus.Buffering;
  overlay.value = true;
}

function handlePlaying() {
  status.value = PlaybackStatus.Playing;
  startOverlayTimeout();
}

function handleEnded() {
  status.value = PlaybackStatus.Paused;
  currentTime.value = duration.value;
  void stopPlayback(true);
}

function handleTimeUpdate() {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  currentTime.value = player.currentTime;
  duration.value = Number.isFinite(player.duration) && player.duration > 0 ? player.duration : duration.value;

  const second = Math.floor(player.currentTime);
  if (second - lastProgressSecond >= 10) {
    lastProgressSecond = second;
    void reportProgress(false).catch(() => undefined);
  }
}

function handleDurationChange() {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  if (Number.isFinite(player.duration) && player.duration > 0) {
    duration.value = player.duration;
  }
}

function handleMediaError() {
  const mediaError = mediaElementRef.value?.error;
  if (!mediaError) {
    return;
  }

  status.value = PlaybackStatus.Error;
  const container = currentSource.value?.Container?.toUpperCase() || 'UNKNOWN';
  const codec = videoStream.value?.Codec ? ` / ${videoStream.value.Codec}` : '';

  if (mediaError.code === MediaError.MEDIA_ERR_NETWORK) {
    error.value = '播放网络连接失败，请查看容器日志中的 /Videos 流请求和 STRM 上游响应。';
  } else if (mediaError.code === MediaError.MEDIA_ERR_DECODE) {
    error.value = `浏览器无法解码当前媒体流（${container}${codec}）。`;
  } else if (mediaError.code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED) {
    error.value = `浏览器不支持当前播放源（${container}${codec}）。`;
  } else {
    error.value = '媒体播放失败，请查看浏览器控制台和容器日志。';
  }
}

function onHlsError(_event: typeof Events.ERROR, data: ErrorData) {
  if (!data.fatal || !hls) {
    return;
  }

  if (data.type === ErrorTypes.NETWORK_ERROR) {
    hls.startLoad();
    return;
  }

  if (data.type === ErrorTypes.MEDIA_ERROR) {
    hls.recoverMediaError();
    return;
  }

  error.value = 'HLS 播放失败，请检查 STRM 地址和 HLS 清单。';
  status.value = PlaybackStatus.Error;
}

async function stopPlayback(forceCompleted = false) {
  if (!item.value || !playSessionId.value || hasStopped || !mediaElementRef.value) {
    return;
  }

  hasStopped = true;
  try {
    await api.playbackStopped({
      ItemId: item.value.Id,
      PlaySessionId: playSessionId.value,
      MediaSourceId: currentSource.value?.Id,
      PositionTicks: toTicks(mediaElementRef.value.currentTime),
      IsPaused: isPaused.value,
      PlayedToCompletion: forceCompleted || playedToCompletion()
    });
  } catch {
    // 退出播放器时不阻塞页面。
  }
}

async function reportProgress(isPausedReport = false) {
  if (!item.value || !playSessionId.value || !mediaElementRef.value) {
    return;
  }

  await api.playbackProgress({
    ItemId: item.value.Id,
    PlaySessionId: playSessionId.value,
    MediaSourceId: currentSource.value?.Id,
    PositionTicks: toTicks(mediaElementRef.value.currentTime),
    IsPaused: isPausedReport,
    PlayedToCompletion: false
  });
}

function playedToCompletion() {
  const player = mediaElementRef.value;
  if (!player || !player.duration || Number.isNaN(player.duration)) {
    return false;
  }

  return player.currentTime / player.duration >= 0.9;
}

function togglePlayback() {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  if (player.paused) {
    void requestElementPlay();
  } else {
    player.pause();
  }
}

function seek(event: Event) {
  const player = mediaElementRef.value;
  if (!player || !duration.value) {
    return;
  }

  const value = Number((event.target as HTMLInputElement).value);
  player.currentTime = duration.value * (value / 100);
  currentTime.value = player.currentTime;
  touchOverlay();
}

function skip(seconds: number) {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  const nextValue = Math.max(0, Math.min(player.duration || Infinity, player.currentTime + seconds));
  player.currentTime = nextValue;
  currentTime.value = nextValue;
  touchOverlay();
}

function setSubtitle(index: number | null) {
  selectedSubtitleIndex.value = index;
  applySubtitleSelection();
  touchOverlay();
}

function applySubtitleSelection() {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  Array.from(player.textTracks).forEach((track, index) => {
    const stream = subtitleStreams.value[index];
    track.mode = stream && stream.Index === selectedSubtitleIndex.value ? 'showing' : 'disabled';
  });
}

function handleVolumeInput(event: Event) {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  const nextVolume = Math.max(0, Math.min(1, Number((event.target as HTMLInputElement).value)));
  player.volume = nextVolume;
  player.muted = nextVolume === 0;
  if (nextVolume > 0) {
    rememberedVolume = nextVolume;
  }
  syncVolumeState();
}

function toggleMuted() {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  if (player.muted || player.volume === 0) {
    player.muted = false;
    player.volume = rememberedVolume > 0 ? rememberedVolume : 1;
  } else {
    rememberedVolume = player.volume || rememberedVolume;
    player.muted = true;
  }

  syncVolumeState();
  touchOverlay();
}

function syncVolumeState() {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  volume.value = player.muted ? 0 : player.volume;
  muted.value = player.muted || player.volume === 0;
}

async function toggleFullscreen() {
  const container = videoContainerRef.value;
  if (!container) {
    return;
  }

  if (document.fullscreenElement) {
    await document.exitFullscreen();
  } else {
    await container.requestFullscreen();
  }
}

async function togglePictureInPicture() {
  const player = mediaElementRef.value;
  if (!player || !supportsPictureInPicture.value) {
    return;
  }

  if (document.pictureInPictureElement) {
    await document.exitPictureInPicture();
  } else {
    await player.requestPictureInPicture();
  }
}

async function closePlayer() {
  await stopPlayback();
  await router.replace(item.value ? itemRoute(item.value) : '/');
}

function togglePanel(panel: Exclude<PlayerPanel, null>) {
  panelMode.value = panelMode.value === panel ? null : panel;
  overlay.value = true;
}

function touchOverlay() {
  overlay.value = true;
  if (!staticOverlay.value) {
    startOverlayTimeout();
  }
}

function startOverlayTimeout() {
  window.clearTimeout(overlayTimer);
  overlayTimer = window.setTimeout(() => {
    overlay.value = false;
  }, 5000);
}

function syncFullscreenState() {
  isFullscreen.value = Boolean(document.fullscreenElement);
}

function handleKeydown(event: KeyboardEvent) {
  if (event.ctrlKey || event.metaKey || event.altKey) {
    return;
  }

  const target = event.target as HTMLElement | null;
  if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) {
    return;
  }

  switch (event.key) {
    case ' ':
    case 'k':
    case 'K':
      event.preventDefault();
      togglePlayback();
      break;
    case 'ArrowLeft':
      event.preventDefault();
      skip(-10);
      break;
    case 'ArrowRight':
      event.preventDefault();
      skip(10);
      break;
    case 'ArrowUp':
      event.preventDefault();
      setVolumeDelta(0.05);
      break;
    case 'ArrowDown':
      event.preventDefault();
      setVolumeDelta(-0.05);
      break;
    case 'f':
    case 'F':
      event.preventDefault();
      void toggleFullscreen();
      break;
    case 'm':
    case 'M':
      event.preventDefault();
      toggleMuted();
      break;
    case 'Escape':
      if (panelMode.value) {
        panelMode.value = null;
      }
      break;
    default:
      touchOverlay();
  }
}

function setVolumeDelta(delta: number) {
  const player = mediaElementRef.value;
  if (!player) {
    return;
  }

  player.volume = Math.max(0, Math.min(1, player.volume + delta));
  player.muted = player.volume === 0;
  if (player.volume > 0) {
    rememberedVolume = player.volume;
  }
  syncVolumeState();
  touchOverlay();
}

function destroyStreamingPlayers() {
  if (hls) {
    hls.off(Events.ERROR, onHlsError);
    hls.destroy();
    hls = null;
  }

  if (mpegtsPlayer) {
    try {
      mpegtsPlayer.unload?.();
      mpegtsPlayer.detachMediaElement?.();
      mpegtsPlayer.destroy();
    } finally {
      mpegtsPlayer = null;
    }
  }
}

async function loadMpegtsRuntime() {
  try {
    const imported = (await import('mpegts.js')) as unknown as MpegtsRuntime & {
      default?: MpegtsRuntime;
    };
    return imported.default || imported;
  } catch {
    return null;
  }
}

async function detectPlaybackEngine(url: string): Promise<PlaybackEngine> {
  const container = normalizedContainer();
  const sourcePath = currentSource.value?.Path || '';

  if (looksLikeHls(container, url, sourcePath)) {
    return 'hls';
  }

  if (looksLikeMpegts(container, url, sourcePath)) {
    return 'mpegts';
  }

  if (shouldProbeStreamType(container, url, sourcePath)) {
    const contentType = await probeStreamContentType(url);
    if (isHlsMime(contentType)) {
      return 'hls';
    }

    if (isMpegtsMime(contentType)) {
      return 'mpegts';
    }
  }

  return 'native';
}

function normalizedContainer() {
  return (currentSource.value?.Container || '').trim().replace(/^\./, '').toLowerCase();
}

function looksLikeHls(container: string, url: string, path: string) {
  return container === 'm3u8' || container === 'hls' || hasAnyExtension([url, path], ['m3u8']);
}

function looksLikeMpegts(container: string, url: string, path: string) {
  return ['flv', 'ts', 'm2ts', 'mts'].includes(container) || hasAnyExtension([url, path], ['flv', 'ts', 'm2ts', 'mts']);
}

function shouldProbeStreamType(container: string, url: string, path: string) {
  if (!currentSource.value?.IsRemote) {
    return false;
  }

  if (!container || container === 'strm') {
    return true;
  }

  return !hasAnyExtension([url, path], ['m3u8', 'flv', 'ts', 'm2ts', 'mts', 'mp4', 'm4v', 'mov', 'webm', 'ogv', 'ogg']);
}

async function probeStreamContentType(url: string) {
  try {
    const response = await fetch(url, { method: 'HEAD' });
    return response.ok ? response.headers.get('content-type') || '' : '';
  } catch {
    return '';
  }
}

function isHlsMime(contentType: string) {
  const value = contentType.toLowerCase();
  return value.includes('mpegurl') || value.includes('m3u8');
}

function isMpegtsMime(contentType: string) {
  const value = contentType.toLowerCase();
  return value.includes('video/x-flv') || value.includes('video/mp2t') || value.includes('mp2t');
}

function canPlayNativeHls(player: HTMLVideoElement) {
  return Boolean(
    player.canPlayType('application/vnd.apple.mpegurl').replace('no', '') ||
      player.canPlayType('application/x-mpegURL').replace('no', '')
  );
}

function hasAnyExtension(values: string[], extensions: string[]) {
  return values.some((value) => {
    const cleanValue = value.split('#')[0].split('?')[0].toLowerCase();
    return extensions.some((extension) => cleanValue.endsWith(`.${extension}`));
  });
}

function trackUrl(stream: MediaStreamDto) {
  return api.subtitleUrl(stream.DeliveryUrl);
}

function toTicks(seconds: number) {
  return Math.max(0, Math.round(seconds * 10_000_000));
}

function ticksToSeconds(ticks?: number) {
  return ticks ? ticks / 10_000_000 : 0;
}

function timeText(value: number) {
  const totalSeconds = Math.max(0, Math.floor(value));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${String(hours).padStart(2, '0')}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
  }

  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}

function seasonEpisode(seasonNumber?: number, episodeNumber?: number) {
  const season = seasonNumber ? `S${String(seasonNumber).padStart(2, '0')}` : '';
  const episode = episodeNumber ? `E${String(episodeNumber).padStart(2, '0')}` : '';
  return `${season}${episode}`;
}

function sourceLabel(source: NonNullable<BaseItemDto['MediaSources']>[number], index: number) {
  return source.Container ? `${source.Container.toUpperCase()} 源 ${index + 1}` : `播放源 ${index + 1}`;
}
</script>

<template>
  <section v-if="loading" class="jelly-player-loading">
    <div class="jelly-player-loading__spinner" />
    <p>正在准备播放器</p>
  </section>

  <section v-else-if="error || !item" class="jelly-player-loading">
    <div class="jelly-player-error">
      <p>播放失败</p>
      <h1>{{ error || '没有找到播放内容' }}</h1>
      <button type="button" @click="router.back()">返回</button>
    </div>
  </section>

  <section
    v-else
    ref="videoContainerRef"
    class="jelly-video-page"
    :class="{ 'is-cursor-hidden': !overlay }"
    @mousemove.passive="touchOverlay"
    @touchend.passive="touchOverlay"
  >
    <video
      ref="mediaElementRef"
      class="jelly-video-element"
      :poster="posterUrl"
      autoplay
      playsinline
      :crossorigin="'anonymous'"
      preload="auto"
      @click="togglePlayback"
      @loadeddata="handleLoadedData"
      @canplay="handleCanPlay"
      @play="handlePlay"
      @pause="handlePause"
      @waiting="handleWaiting"
      @playing="handlePlaying"
      @ended="handleEnded"
      @timeupdate="handleTimeUpdate"
      @durationchange="handleDurationChange"
      @volumechange="syncVolumeState"
      @error="handleMediaError"
    >
      <track
        v-for="stream in subtitleStreams"
        :key="`${currentSource?.Id}-${stream.Index}`"
        kind="subtitles"
        :label="stream.DisplayTitle || `字幕 ${stream.Index}`"
        :srclang="stream.Language || 'und'"
        :src="trackUrl(stream)"
      />
    </video>

    <div class="jelly-osd" :class="{ 'is-hidden': !overlay }">
      <header class="jelly-osd__top">
        <div class="jelly-osd__bar">
          <div class="jelly-osd__left">
            <button class="icon-button" type="button" title="关闭" @click="closePlayer">×</button>
            <button class="icon-button" type="button" title="返回窗口" @click="closePlayer">⌄</button>
          </div>
          <div class="jelly-osd__right">
            <button class="icon-button" type="button" title="投屏" disabled>▣</button>
          </div>
        </div>
      </header>

      <main class="jelly-osd__center">
        <button class="center-play" type="button" :title="isPaused ? '播放' : '暂停'" @click="togglePlayback">
          {{ isPaused ? '▶' : 'Ⅱ' }}
        </button>
        <p v-if="isBuffering" class="buffering-text">正在缓冲媒体流…</p>
      </main>

      <footer class="jelly-osd__bottom">
        <div class="jelly-osd__inner">
          <div class="time-slider">
            <span>{{ timeText(currentTime) }}</span>
            <input min="0" max="100" step="0.1" type="range" :value="progressValue" @input="seek" />
            <span>{{ timeText(duration) }}</span>
          </div>

          <div class="control-row">
            <div class="video-title">
              <strong>{{ titleLine }}</strong>
              <span>{{ subtitleLine }}</span>
              <small v-if="endsAtText">{{ endsAtText }}</small>
            </div>

            <div class="transport">
              <button class="icon-button" type="button" title="上一项" disabled>⏮</button>
              <button class="icon-button large" type="button" title="后退 10 秒" @click="skip(-10)">↶</button>
              <button class="icon-button primary" type="button" :title="isPaused ? '播放' : '暂停'" @click="togglePlayback">
                {{ isPaused ? '▶' : 'Ⅱ' }}
              </button>
              <button class="icon-button large" type="button" title="快进 10 秒" @click="skip(10)">↷</button>
              <button class="icon-button" type="button" title="下一项" disabled>⏭</button>
            </div>

            <div class="actions">
              <label class="volume-slider" title="音量">
                <button class="icon-button inline" type="button" :title="muted ? '取消静音' : '静音'" @click.prevent="toggleMuted">
                  {{ muted ? 'M' : 'V' }}
                </button>
                <input min="0" max="1" step="0.05" type="range" :value="volume" @input="handleVolumeInput" />
              </label>
              <button class="icon-button" type="button" title="字幕和音轨" @click="togglePanel('tracks')">▤</button>
              <button class="icon-button" type="button" title="播放设置" @click="togglePanel('settings')">⚙</button>
              <button class="icon-button" type="button" title="媒体信息" @click="togglePanel('info')">ⓘ</button>
              <button
                v-if="supportsPictureInPicture"
                class="icon-button"
                type="button"
                title="画中画"
                @click="togglePictureInPicture"
              >
                ◱
              </button>
              <button class="icon-button" type="button" :title="isFullscreen ? '退出全屏' : '全屏'" @click="toggleFullscreen">
                {{ isFullscreen ? '⛶' : '⛶' }}
              </button>
            </div>
          </div>

          <aside v-if="panelMode" class="jelly-panel">
            <template v-if="panelMode === 'tracks'">
              <div class="panel-head">
                <h2>字幕和音轨</h2>
                <button class="icon-button inline" type="button" title="关闭" @click="panelMode = null">×</button>
              </div>
              <section class="panel-section">
                <h3>音轨</h3>
                <button
                  v-for="stream in audioStreams"
                  :key="stream.Index"
                  type="button"
                  :class="{ 'is-selected': selectedAudioIndex === stream.Index || (selectedAudioIndex === null && stream.IsDefault) }"
                  @click="selectedAudioIndex = stream.Index"
                >
                  <strong>{{ stream.DisplayTitle || `音轨 ${stream.Index}` }}</strong>
                  <span>{{ streamText(stream) || selectedAudioLabel }}</span>
                </button>
              </section>
              <section class="panel-section">
                <h3>字幕</h3>
                <button type="button" :class="{ 'is-selected': selectedSubtitleIndex === null }" @click="setSubtitle(null)">
                  <strong>关闭字幕</strong>
                  <span>不显示字幕</span>
                </button>
                <button
                  v-for="stream in subtitleStreams"
                  :key="stream.Index"
                  type="button"
                  :class="{ 'is-selected': selectedSubtitleIndex === stream.Index }"
                  @click="setSubtitle(stream.Index)"
                >
                  <strong>{{ stream.DisplayTitle || `字幕 ${stream.Index}` }}</strong>
                  <span>{{ stream.Language || stream.Codec || 'External' }}</span>
                </button>
              </section>
            </template>

            <template v-else-if="panelMode === 'settings'">
              <div class="panel-head">
                <h2>播放设置</h2>
                <button class="icon-button inline" type="button" title="关闭" @click="panelMode = null">×</button>
              </div>
              <section class="panel-section">
                <h3>播放源</h3>
                <button
                  v-for="(source, index) in item.MediaSources"
                  :key="source.Id"
                  type="button"
                  :class="{ 'is-selected': currentSourceIndex === index }"
                  @click="currentSourceIndex = index"
                >
                  <strong>{{ sourceLabel(source, index) }}</strong>
                  <span>{{ source.Size ? fileSize(source.Size) : 'Direct Stream' }}</span>
                </button>
              </section>
              <section class="panel-section">
                <h3>播放速度</h3>
                <button
                  v-for="rate in [0.5, 0.75, 1, 1.25, 1.5, 2]"
                  :key="rate"
                  type="button"
                  :class="{ 'is-selected': playbackRate === rate }"
                  @click="playbackRate = rate"
                >
                  <strong>{{ rate }}x</strong>
                  <span>{{ rate === 1 ? '正常' : '变速播放' }}</span>
                </button>
              </section>
            </template>

            <template v-else>
              <div class="panel-head">
                <h2>媒体信息</h2>
                <button class="icon-button inline" type="button" title="关闭" @click="panelMode = null">×</button>
              </div>
              <div class="info-grid">
                <div>
                  <strong>播放源</strong>
                  <span>{{ currentSource ? sourceLabel(currentSource, currentSourceIndex) : '默认' }}</span>
                </div>
                <div>
                  <strong>容器</strong>
                  <span>{{ currentSource?.Container?.toUpperCase() || item.Container || '未知' }}</span>
                </div>
                <div>
                  <strong>字幕</strong>
                  <span>{{ selectedSubtitleLabel }}</span>
                </div>
                <div>
                  <strong>音轨</strong>
                  <span>{{ selectedAudioLabel }}</span>
                </div>
              </div>
              <div class="stream-list">
                <div v-for="stream in currentStreams" :key="`${stream.Type}-${stream.Index}`">
                  <strong>{{ streamLabel(stream.Type) }}</strong>
                  <span>{{ streamText(stream) || '默认轨道' }}</span>
                </div>
              </div>
            </template>
          </aside>
        </div>
      </footer>
    </div>
  </section>
</template>

<style scoped>
.jelly-video-page,
.jelly-player-loading {
  width: 100%;
  min-height: 100vh;
  background: #000;
  color: #fff;
}

.jelly-video-page {
  position: relative;
  overflow: hidden;
}

.jelly-video-page.is-cursor-hidden {
  cursor: none;
}

.jelly-video-element {
  width: 100vw;
  height: 100vh;
  display: block;
  object-fit: contain;
  background: #000;
}

.jelly-osd {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  pointer-events: none;
  opacity: 1;
  transition: opacity 160ms ease;
}

.jelly-osd.is-hidden {
  opacity: 0;
}

.jelly-osd__top,
.jelly-osd__bottom {
  pointer-events: auto;
  width: 100%;
  padding: 8px;
}

.jelly-osd__top {
  padding-bottom: 5em;
  background: linear-gradient(
    to bottom,
    rgba(0, 0, 0, 0.75) 0%,
    rgba(0, 0, 0, 0.56) 35%,
    rgba(0, 0, 0, 0.19) 65%,
    rgba(0, 0, 0, 0) 100%
  );
}

.jelly-osd__bottom {
  padding-top: 6em;
  background: linear-gradient(
    to top,
    rgba(0, 0, 0, 0.78) 0%,
    rgba(0, 0, 0, 0.58) 35%,
    rgba(0, 0, 0, 0.19) 65%,
    rgba(0, 0, 0, 0) 100%
  );
}

.jelly-osd__bar,
.jelly-osd__inner {
  width: min(175vh, calc(100vw - 16px));
  margin: 0 auto;
}

.jelly-osd__bar,
.jelly-osd__left,
.jelly-osd__right,
.transport,
.actions,
.control-row {
  display: flex;
  align-items: center;
}

.jelly-osd__bar {
  justify-content: space-between;
  padding: 8px 16px;
}

.jelly-osd__center {
  display: grid;
  justify-items: center;
  align-content: center;
  gap: 14px;
  padding: 24px;
  pointer-events: none;
}

.center-play {
  pointer-events: auto;
  width: 76px;
  height: 76px;
  border: 0;
  border-radius: 50%;
  background: rgba(0, 164, 220, 0.86);
  color: #fff;
  font-size: 2rem;
  line-height: 1;
  box-shadow: 0 18px 60px rgba(0, 0, 0, 0.4);
}

.buffering-text {
  min-height: 24px;
  padding: 6px 12px;
  border-radius: 6px;
  background: rgba(0, 0, 0, 0.58);
  color: #d9e3ef;
}

.time-slider {
  display: grid;
  grid-template-columns: 64px 1fr 64px;
  gap: 12px;
  align-items: center;
}

.time-slider span {
  color: #e5edf8;
  font-size: 0.95rem;
  text-align: center;
}

.time-slider input,
.volume-slider input {
  width: 100%;
  accent-color: #00a4dc;
}

.control-row {
  position: relative;
  min-height: 6em;
  justify-content: space-between;
  gap: 16px;
}

.video-title {
  width: min(40vw, 520px);
  display: grid;
  gap: 4px;
  align-content: center;
}

.video-title strong,
.video-title span,
.video-title small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.video-title span,
.video-title small {
  color: rgba(231, 238, 248, 0.74);
}

.transport {
  position: absolute;
  inset: 0;
  justify-content: center;
  gap: 8px;
  pointer-events: none;
}

.transport > * {
  pointer-events: auto;
}

.actions {
  margin-left: auto;
  justify-content: flex-end;
  gap: 8px;
  z-index: 1;
}

.icon-button {
  width: 44px;
  height: 44px;
  border: 0;
  border-radius: 50%;
  display: inline-grid;
  place-items: center;
  background: transparent;
  color: #fff;
  font-size: 1.35rem;
}

.icon-button:hover,
.icon-button.primary,
.center-play:hover {
  background: rgba(0, 164, 220, 0.82);
}

.icon-button:disabled {
  opacity: 0.4;
}

.icon-button.large {
  font-size: 1.55rem;
}

.icon-button.inline {
  width: 34px;
  height: 34px;
  font-size: 1.1rem;
}

.volume-slider {
  width: 160px;
  display: grid;
  grid-template-columns: 34px 1fr;
  gap: 8px;
  align-items: center;
}

.jelly-panel {
  width: min(440px, 100%);
  max-height: min(56vh, 580px);
  overflow: auto;
  margin-left: auto;
  padding: 18px;
  border-radius: 8px;
  background: rgba(12, 16, 22, 0.92);
  box-shadow: 0 18px 80px rgba(0, 0, 0, 0.45);
  backdrop-filter: blur(18px);
}

.panel-head,
.info-grid,
.stream-list {
  display: grid;
  gap: 12px;
}

.panel-head {
  grid-template-columns: 1fr auto;
  align-items: center;
}

.panel-head h2 {
  font-size: 1.1rem;
}

.panel-section {
  display: grid;
  gap: 8px;
  margin-top: 16px;
}

.panel-section h3 {
  color: rgba(231, 238, 248, 0.72);
  font-size: 0.9rem;
  font-weight: 600;
}

.panel-section button {
  min-height: 56px;
  padding: 10px 12px;
  border: 0;
  border-radius: 6px;
  display: grid;
  gap: 4px;
  justify-items: start;
  background: rgba(255, 255, 255, 0.06);
  color: #fff;
  text-align: left;
}

.panel-section button.is-selected {
  background: rgba(0, 164, 220, 0.26);
  box-shadow: inset 3px 0 0 #00a4dc;
}

.panel-section span,
.info-grid span,
.stream-list span {
  color: rgba(231, 238, 248, 0.7);
}

.info-grid {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.info-grid > div,
.stream-list > div {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.stream-list {
  margin-top: 16px;
}

.jelly-player-loading {
  display: grid;
  place-items: center;
  gap: 16px;
  text-align: center;
}

.jelly-player-loading__spinner {
  width: 46px;
  height: 46px;
  border: 4px solid rgba(255, 255, 255, 0.2);
  border-top-color: #00a4dc;
  border-radius: 50%;
  animation: spin 0.9s linear infinite;
}

.jelly-player-error {
  width: min(560px, calc(100vw - 32px));
  display: grid;
  gap: 16px;
}

.jelly-player-error h1 {
  font-size: 1.25rem;
  line-height: 1.4;
}

.jelly-player-error button {
  min-height: 42px;
  width: fit-content;
  padding: 0 18px;
  border: 0;
  border-radius: 6px;
  background: #00a4dc;
  color: #fff;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 980px) {
  .control-row {
    display: grid;
    min-height: auto;
    gap: 12px;
  }

  .transport {
    position: static;
    order: 1;
  }

  .video-title {
    width: 100%;
    order: 0;
  }

  .actions {
    margin-left: 0;
    justify-content: flex-start;
    flex-wrap: wrap;
    order: 2;
  }
}

@media (max-width: 640px) {
  .jelly-osd__top,
  .jelly-osd__bottom {
    padding: 6px;
  }

  .time-slider {
    grid-template-columns: 52px 1fr 52px;
    gap: 8px;
  }

  .volume-slider {
    width: 130px;
  }

  .jelly-panel {
    width: 100%;
    max-height: 48vh;
  }

  .info-grid {
    grid-template-columns: 1fr;
  }
}
</style>
