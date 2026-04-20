<script setup lang="ts">
import Hls, { ErrorTypes, Events, type ErrorData } from 'hls.js';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { BaseItemDto } from '../../api/emby';
import { api, fileSize, streamLabel, streamText } from '../../store/app';
import { itemRoute } from '../../utils/navigation';

type PlayerPanel = 'sources' | 'subtitles' | 'info' | null;

interface SubtitleOption {
  key: string;
  label: string;
  src: string;
  srclang: string;
  streamIndex: number;
  isDefault: boolean;
}

type PlaybackEngine = 'hls' | 'mpegts' | 'native';

interface MpegtsRuntime {
  isSupported?: () => boolean;
  getFeatureList?: () => {
    msePlayback?: boolean;
    mseLivePlayback?: boolean;
    networkStreamIO?: boolean;
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

const playerShellRef = ref<HTMLElement | null>(null);
const videoRef = ref<HTMLVideoElement | null>(null);
const loading = ref(false);
const error = ref('');
const item = ref<BaseItemDto | null>(null);
const currentSourceIndex = ref(0);
const playSessionId = ref('');
const overlayVisible = ref(true);
const paused = ref(true);
const waiting = ref(false);
const duration = ref(0);
const currentTime = ref(0);
const volume = ref(1);
const muted = ref(false);
const isFullscreen = ref(false);
const panelMode = ref<PlayerPanel>(null);
const selectedSubtitleKey = ref('off');

let overlayTimer = 0;
let hasStarted = false;
let hasStopped = false;
let lastProgressSecond = -10;
let pendingSeekSeconds = 0;
let rememberedVolume = 1;
let hls: Hls | null = null;
let mpegtsPlayer: MpegtsPlayer | null = null;

const currentSource = computed(() => item.value?.MediaSources?.[currentSourceIndex.value]);
const sourceUrl = computed(() => {
  if (currentSource.value) {
    const directSourceUrl = api.streamUrlForSource(currentSource.value);
    if (directSourceUrl) {
      return directSourceUrl;
    }
  }

  return item.value ? api.streamUrl(item.value) : '';
});
const posterImage = computed(() =>
  item.value ? api.backdropUrl(item.value) || api.itemImageUrl(item.value) : ''
);
const currentStreams = computed(() => currentSource.value?.MediaStreams || item.value?.MediaStreams || []);
const videoStream = computed(() => currentStreams.value.find((stream) => stream.Type === 'Video') || null);
const audioStreams = computed(() => currentStreams.value.filter((stream) => stream.Type === 'Audio'));
const subtitleStreams = computed(() => currentStreams.value.filter((stream) => stream.Type === 'Subtitle'));
const subtitleOptions = computed<SubtitleOption[]>(() =>
  currentStreams.value
    .filter((stream) => stream.Type === 'Subtitle' && stream.DeliveryUrl)
    .map((stream) => ({
      key: String(stream.Index),
      label: stream.DisplayTitle || `字幕 ${stream.Index}`,
      src: api.subtitleUrl(stream.DeliveryUrl),
      srclang: (stream.Language || 'und').toLowerCase(),
      streamIndex: stream.Index,
      isDefault: Boolean(stream.IsDefault)
    }))
);
const progressValue = computed(() =>
  duration.value > 0 ? Math.min(100, (currentTime.value / duration.value) * 100) : 0
);
const topMeta = computed(() => {
  if (!item.value) {
    return '';
  }

  if (item.value.Type === 'Episode') {
    return [
      item.value.SeriesName,
      formatSeasonEpisode(item.value.ParentIndexNumber, item.value.IndexNumber)
    ]
      .filter(Boolean)
      .join(' · ');
  }

  return [item.value.ProductionYear, item.value.Type].filter(Boolean).join(' · ');
});
const detailMeta = computed(() => {
  if (!item.value) {
    return '';
  }

  const parts = [
    currentSource.value?.Container?.toUpperCase(),
    videoStream.value?.Width && videoStream.value?.Height
      ? `${videoStream.value.Width}x${videoStream.value.Height}`
      : '',
    currentSource.value?.Size ? fileSize(currentSource.value.Size) : ''
  ].filter(Boolean);

  return parts.join(' · ');
});
const endTimeText = computed(() => {
  if (!duration.value || currentTime.value >= duration.value) {
    return '';
  }

  const remainingSeconds = Math.max(0, duration.value - currentTime.value);
  const endTime = new Date(Date.now() + remainingSeconds * 1000);
  return `预计 ${formatClock(endTime)} 结束`;
});
const selectedSubtitleLabel = computed(() => {
  if (selectedSubtitleKey.value === 'off') {
    return '关闭';
  }

  return (
    subtitleOptions.value.find((track) => track.key === selectedSubtitleKey.value)?.label ||
    '未选择'
  );
});
const currentSourceLabel = computed(() =>
  currentSource.value ? sourceLabel(currentSource.value, currentSourceIndex.value) : '未选择'
);
const supportsPictureInPicture = computed(
  () => typeof document !== 'undefined' && 'pictureInPictureEnabled' in document && document.pictureInPictureEnabled
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
    const player = videoRef.value;
    if (!player || !sourceUrl.value) {
      return;
    }

    pendingSeekSeconds = player.currentTime;
    currentTime.value = pendingSeekSeconds;
    waiting.value = true;
    panelMode.value = null;
    await attachPlaybackSource();

    try {
      await player.play();
    } catch {
      paused.value = true;
    }
  }
);

watch(
  subtitleOptions,
  async (tracks) => {
    if (!tracks.length) {
      selectedSubtitleKey.value = 'off';
      await nextTick();
      applySubtitleSelection();
      return;
    }

    if (tracks.some((track) => track.key === selectedSubtitleKey.value)) {
      await nextTick();
      applySubtitleSelection();
      return;
    }

    const defaultTrack =
      tracks.find((track) => track.streamIndex === currentSource.value?.DefaultSubtitleStreamIndex) ||
      tracks.find((track) => track.isDefault) ||
      null;

    selectedSubtitleKey.value = defaultTrack?.key || 'off';
    await nextTick();
    applySubtitleSelection();
  },
  { immediate: true }
);

watch(
  () => selectedSubtitleKey.value,
  async () => {
    await nextTick();
    applySubtitleSelection();
  }
);

watch([paused, waiting, panelMode], () => {
  const keepVisible = paused.value || waiting.value || Boolean(panelMode.value);
  overlayVisible.value = true;
  window.clearTimeout(overlayTimer);

  if (!keepVisible) {
    overlayTimer = window.setTimeout(() => {
      overlayVisible.value = false;
    }, 5000);
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

async function loadPlayback(nextItemId: string) {
  loading.value = true;
  error.value = '';
  waiting.value = true;
  panelMode.value = null;
  overlayVisible.value = true;

  try {
    await stopPlayback();
    hasStarted = false;
    hasStopped = false;
    lastProgressSecond = -10;
    pendingSeekSeconds = 0;

    const [loadedItem, playback] = await Promise.all([api.item(nextItemId), api.playbackInfo(nextItemId)]);

    item.value = {
      ...loadedItem,
      MediaSources: playback.MediaSources,
      MediaStreams: playback.MediaSources[0]?.MediaStreams || loadedItem.MediaStreams
    };
    currentSourceIndex.value = 0;
    playSessionId.value = playback.PlaySessionId;
    duration.value = 0;
    currentTime.value = 0;
    paused.value = true;
    pendingSeekSeconds = ticksToSeconds(loadedItem.UserData?.PlaybackPositionTicks);
    touchOverlay();

    await nextTick();
    syncVolumeState();
    if (videoRef.value) {
      await attachPlaybackSource();
      try {
        await videoRef.value.play();
      } catch {
        paused.value = true;
      }
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

function ticksToSeconds(ticks?: number) {
  return ticks ? ticks / 10_000_000 : 0;
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
  waiting.value = false;

  try {
    await api.playbackStopped({
      ItemId: item.value.Id,
      PlaySessionId: playSessionId.value,
      MediaSourceId: currentSource.value?.Id,
      PositionTicks: toTicks(videoRef.value.currentTime),
      IsPaused: paused.value,
      PlayedToCompletion: forceCompleted || playedToCompletion()
    });
  } catch {
    // 忽略停止上报失败，避免阻塞页面离开。
  }
}

function handlePlay() {
  paused.value = false;
  waiting.value = false;
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
        PositionTicks: toTicks(videoRef.value?.currentTime || 0)
      })
      .catch(() => undefined);
  } else {
    void reportProgress(false).catch(() => undefined);
  }
}

function handlePause() {
  paused.value = true;
  touchOverlay();
  void reportProgress(true).catch(() => undefined);
}

function handleEnded() {
  paused.value = true;
  currentTime.value = duration.value;
  void stopPlayback(true);
}

function handleTimeUpdate() {
  const player = videoRef.value;
  if (!player) {
    return;
  }

  currentTime.value = player.currentTime;
  duration.value = Number.isFinite(player.duration) ? player.duration : 0;

  const second = Math.floor(player.currentTime);
  if (second - lastProgressSecond >= 10) {
    lastProgressSecond = second;
    void reportProgress(false).catch(() => undefined);
  }

  validateVideoFrame();
}

function handleLoadedMetadata() {
  const player = videoRef.value;
  if (!player) {
    return;
  }

  duration.value = Number.isFinite(player.duration) ? player.duration : 0;
  syncVolumeState();

  if (pendingSeekSeconds > 0) {
    try {
      player.currentTime = Math.min(
        pendingSeekSeconds,
        Number.isFinite(player.duration) && player.duration > 0 ? player.duration : pendingSeekSeconds
      );
      currentTime.value = player.currentTime;
    } catch {
      // 某些浏览器在媒体尚未准备好时不允许立即 seek。
    } finally {
      pendingSeekSeconds = 0;
    }
  }

  applySubtitleSelection();
}

function handleDurationChange() {
  const player = videoRef.value;
  if (!player) {
    return;
  }

  duration.value = Number.isFinite(player.duration) ? player.duration : 0;
}

function handleMediaError(_event?: Event) {
  const player = videoRef.value;
  const mediaError = player?.error;
  const container = currentSource.value?.Container?.toUpperCase() || 'UNKNOWN';
  const codec = videoStream.value?.Codec ? ` / ${videoStream.value.Codec}` : '';

  if (!mediaError) {
    return;
  }

  if (mediaError.code === MediaError.MEDIA_ERR_NETWORK) {
    error.value = '播放网络连接失败，请查看容器日志里的 STRM 代理请求和上游响应状态。';
    return;
  }

  if (mediaError.code === MediaError.MEDIA_ERR_DECODE) {
    error.value = `浏览器无法解码当前视频流（${container}${codec}），这通常是容器或视频编码不被 Web 播放器支持。`;
    return;
  }

  if (mediaError.code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED) {
    error.value = `浏览器不支持当前播放源（${container}${codec}）。MP4/WebM/HLS/FLV/TS 会优先直连或 MSE 播放，MKV/AVI/WMV 等仍取决于浏览器自身能力。`;
  }
}

function validateVideoFrame(_event?: Event) {
  const player = videoRef.value;
  if (!player || error.value || item.value?.MediaType !== 'Video' || !videoStream.value) {
    return;
  }

  if (player.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || player.currentTime < 1) {
    return;
  }

  if (player.videoWidth > 0 || player.videoHeight > 0) {
    return;
  }

  const container = currentSource.value?.Container?.toUpperCase() || 'UNKNOWN';
  const codec = videoStream.value.Codec ? ` / ${videoStream.value.Codec}` : '';
  error.value = `已经收到音频但没有视频画面，当前视频轨道可能不被浏览器解码器支持（${container}${codec}）。`;
}

async function attachPlaybackSource() {
  const player = videoRef.value;
  const url = sourceUrl.value;
  if (!player || !url) {
    return;
  }

  destroyStreamingPlayers();
  error.value = '';
  player.removeAttribute('src');
  player.load();

  const engine = await detectPlaybackEngine(url);
  if (engine === 'hls' && Hls.isSupported() && !canPlayNativeHls(player)) {
    await attachHlsSource(player, url);
    return;
  }

  if (engine === 'mpegts') {
    const handledByMse = await attachMpegtsSource(player, url);
    if (handledByMse) {
      return;
    }
  }

  player.src = url;
  player.load();
}

async function attachHlsSource(player: HTMLVideoElement, url: string) {
  const instance = new Hls({
    testBandwidth: false,
    manifestLoadingTimeOut: 20000
  });
  hls = instance;
  instance.on(Events.ERROR, onHlsError);

  await new Promise<void>((resolve) => {
    let settled = false;
    const done = () => {
      if (settled) {
        return;
      }

      settled = true;
      window.clearTimeout(timer);
      instance.off(Events.MANIFEST_PARSED, done);
      resolve();
    };
    const timer = window.setTimeout(done, 3000);

    instance.on(Events.MANIFEST_PARSED, done);
    instance.on(Events.MEDIA_ATTACHED, () => {
      instance.loadSource(url);
    });
    instance.attachMedia(player);
  });
}

async function attachMpegtsSource(player: HTMLVideoElement, url: string) {
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
      lazyLoad: false
    }
  );
  mpegtsPlayer.attachMediaElement(player);
  mpegtsPlayer.load();
  void Promise.resolve(mpegtsPlayer.play?.()).catch(() => undefined);
  return true;
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

function destroyStreamingPlayers() {
  destroyHlsPlayer();
  destroyMpegtsPlayer();
}

function destroyMpegtsPlayer() {
  if (!mpegtsPlayer) {
    return;
  }

  try {
    mpegtsPlayer.unload?.();
    mpegtsPlayer.detachMediaElement?.();
    mpegtsPlayer.destroy();
  } finally {
    mpegtsPlayer = null;
  }
}

function destroyHlsPlayer() {
  if (!hls) {
    return;
  }

  hls.off(Events.ERROR, onHlsError);
  hls.destroy();
  hls = null;
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

  error.value = 'HLS 播放失败，请检查 STRM 远程地址是否可访问';
}

function handleWaiting() {
  waiting.value = true;
  overlayVisible.value = true;
}

function handlePlaying() {
  waiting.value = false;
}

function seek(event: Event) {
  const player = videoRef.value;
  if (!player || !duration.value) {
    return;
  }

  const target = event.target as HTMLInputElement;
  const ratio = Number(target.value) / 100;
  player.currentTime = duration.value * ratio;
  currentTime.value = player.currentTime;
  touchOverlay();
}

function skip(seconds: number) {
  const player = videoRef.value;
  if (!player) {
    return;
  }

  const targetTime = Math.max(0, Math.min(player.duration || Infinity, player.currentTime + seconds));
  player.currentTime = targetTime;
  currentTime.value = targetTime;
  touchOverlay();
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
    void player.play().catch(() => undefined);
  } else {
    player.pause();
  }
}

function handleVolumeInput(event: Event) {
  const player = videoRef.value;
  if (!player) {
    return;
  }

  const target = event.target as HTMLInputElement;
  const nextVolume = Math.max(0, Math.min(1, Number(target.value)));
  player.volume = nextVolume;
  player.muted = nextVolume === 0;
  if (nextVolume > 0) {
    rememberedVolume = nextVolume;
  }
  syncVolumeState();
}

function toggleMuted() {
  const player = videoRef.value;
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

function adjustVolume(delta: number) {
  const player = videoRef.value;
  if (!player) {
    return;
  }

  const nextVolume = Math.max(0, Math.min(1, player.volume + delta));
  player.volume = nextVolume;
  player.muted = nextVolume === 0;
  if (nextVolume > 0) {
    rememberedVolume = nextVolume;
  }
  syncVolumeState();
}

async function toggleFullscreen() {
  const root = playerShellRef.value;
  if (!root) {
    return;
  }

  if (document.fullscreenElement) {
    await document.exitFullscreen();
  } else {
    await root.requestFullscreen();
  }
}

async function togglePictureInPicture() {
  const player = videoRef.value;
  if (!player || !supportsPictureInPicture.value) {
    return;
  }

  const pictureInPictureDocument = document as Document & {
    pictureInPictureElement?: Element | null;
    exitPictureInPicture?: () => Promise<void>;
  };
  const pipVideo = player as HTMLVideoElement & {
    requestPictureInPicture?: () => Promise<void>;
  };

  try {
    if (pictureInPictureDocument.pictureInPictureElement && pictureInPictureDocument.exitPictureInPicture) {
      await pictureInPictureDocument.exitPictureInPicture();
    } else if (pipVideo.requestPictureInPicture) {
      await pipVideo.requestPictureInPicture();
    }
  } catch {
    // 画中画不作为关键能力，失败时保持播放器可用即可。
  }
}

function togglePanel(mode: Exclude<PlayerPanel, null>) {
  panelMode.value = panelMode.value === mode ? null : mode;
  overlayVisible.value = true;
}

function touchOverlay() {
  overlayVisible.value = true;
  window.clearTimeout(overlayTimer);

  if (!paused.value && !waiting.value && !panelMode.value) {
    overlayTimer = window.setTimeout(() => {
      overlayVisible.value = false;
    }, 5000);
  }
}

function syncVolumeState() {
  const player = videoRef.value;
  if (!player) {
    return;
  }

  volume.value = player.muted ? 0 : player.volume;
  muted.value = player.muted || player.volume === 0;
}

function syncFullscreenState() {
  isFullscreen.value = Boolean(document.fullscreenElement);
}

function applySubtitleSelection() {
  const player = videoRef.value;
  if (!player) {
    return;
  }

  Array.from(player.textTracks).forEach((track, index) => {
    const target = subtitleOptions.value[index];
    track.mode = target && target.key === selectedSubtitleKey.value ? 'showing' : 'disabled';
  });
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
      adjustVolume(0.05);
      break;
    case 'ArrowDown':
      event.preventDefault();
      adjustVolume(-0.05);
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
    case 'c':
    case 'C':
      event.preventDefault();
      selectedSubtitleKey.value = selectedSubtitleKey.value === 'off' && subtitleOptions.value.length
        ? subtitleOptions.value[0].key
        : 'off';
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

function formatSeasonEpisode(seasonNumber?: number, episodeNumber?: number) {
  const season = seasonNumber ? `S${String(seasonNumber).padStart(2, '0')}` : '';
  const episode = episodeNumber ? `E${String(episodeNumber).padStart(2, '0')}` : '';
  return `${season}${episode}`;
}

function formatClock(value: Date) {
  return `${String(value.getHours()).padStart(2, '0')}:${String(value.getMinutes()).padStart(2, '0')}`;
}

function sourceLabel(source: NonNullable<BaseItemDto['MediaSources']>[number], index: number) {
  return source.Container ? `${source.Container.toUpperCase()} 源 ${index + 1}` : `播放源 ${index + 1}`;
}

async function detectPlaybackEngine(url: string): Promise<PlaybackEngine> {
  const container = normalizedContainer();
  const path = currentSource.value?.Path || '';

  if (looksLikeHls(container, url, path)) {
    return 'hls';
  }

  if (looksLikeMpegts(container, url, path)) {
    return 'mpegts';
  }

  if (shouldProbeStreamType(container, url, path)) {
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
  return (
    ['flv', 'ts', 'm2ts', 'mts'].includes(container) ||
    hasAnyExtension([url, path], ['flv', 'ts', 'm2ts', 'mts'])
  );
}

function hasAnyExtension(values: string[], extensions: string[]) {
  return values.some((value) => {
    const cleanValue = value.split('#')[0].split('?')[0].toLowerCase();
    return extensions.some((extension) => cleanValue.endsWith(`.${extension}`));
  });
}

function shouldProbeStreamType(container: string, url: string, path: string) {
  if (!currentSource.value?.IsRemote) {
    return false;
  }

  if (!container || container === 'strm') {
    return true;
  }

  return !hasAnyExtension([url, path], [
    'm3u8',
    'flv',
    'ts',
    'm2ts',
    'mts',
    'mp4',
    'm4v',
    'mov',
    'webm',
    'ogv',
    'ogg'
  ]);
}

async function probeStreamContentType(url: string) {
  try {
    const response = await fetch(url, { method: 'HEAD' });
    if (!response.ok) {
      return '';
    }

    return response.headers.get('content-type') || '';
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
    ref="playerShellRef"
    class="jelly-video-player"
    @mousemove="touchOverlay"
    @touchstart.passive="touchOverlay"
  >
    <video
      ref="videoRef"
      class="jelly-video-player__surface"
      :poster="posterImage"
      autoplay
      playsinline
      preload="metadata"
      @click="togglePlayback"
      @play="handlePlay"
      @pause="handlePause"
      @ended="handleEnded"
      @timeupdate="handleTimeUpdate"
      @loadedmetadata="handleLoadedMetadata"
      @loadeddata="validateVideoFrame"
      @durationchange="handleDurationChange"
      @waiting="handleWaiting"
      @playing="handlePlaying"
      @error="handleMediaError"
    >
      <track
        v-for="track in subtitleOptions"
        :key="`${currentSource?.Id}-${track.key}`"
        kind="subtitles"
        :label="track.label"
        :src="track.src"
        :srclang="track.srclang"
      />
    </video>

    <div class="jelly-video-player__overlay" :class="{ 'is-hidden': !overlayVisible }">
      <div class="jelly-video-player__top">
        <div class="jelly-video-player__nav">
          <button class="jelly-video-player__ghost" type="button" @click="closePlayer">关闭</button>
        </div>
        <div class="jelly-video-player__heading">
          <p>{{ topMeta || '在线播放' }}</p>
          <h1>{{ item.Name }}</h1>
          <span v-if="detailMeta">{{ detailMeta }}</span>
        </div>
      </div>

      <div class="jelly-video-player__center">
        <button class="jelly-video-player__hero-button" type="button" @click="togglePlayback">
          {{ paused ? '播放' : '暂停' }}
        </button>
        <p v-if="waiting" class="jelly-video-player__status">正在缓冲媒体流…</p>
      </div>

      <div class="jelly-video-player__bottom">
        <div class="jelly-video-player__timeline">
          <span>{{ timeText(currentTime) }}</span>
          <input
            type="range"
            min="0"
            max="100"
            step="0.1"
            :value="progressValue"
            @input="seek"
          />
          <span>{{ timeText(duration) }}</span>
        </div>

        <div class="jelly-video-player__controls">
          <div class="jelly-video-player__title">
            <strong>{{ item.SeriesName || item.Name }}</strong>
            <span>{{ endTimeText || item.Overview || '继续播放当前媒体' }}</span>
          </div>

          <div class="jelly-video-player__transport">
            <button type="button" @click="skip(-10)">后退 10 秒</button>
            <button type="button" class="is-primary" @click="togglePlayback">
              {{ paused ? '播放' : '暂停' }}
            </button>
            <button type="button" @click="skip(10)">快进 10 秒</button>
          </div>

          <div class="jelly-video-player__actions">
            <label class="jelly-video-player__volume">
              <span>{{ muted ? '静音' : '音量' }}</span>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                :value="volume"
                @input="handleVolumeInput"
              />
            </label>
            <button type="button" @click="toggleMuted">{{ muted ? '取消静音' : '静音' }}</button>
            <button type="button" :class="{ 'is-active': panelMode === 'subtitles' }" @click="togglePanel('subtitles')">
              字幕
            </button>
            <button type="button" :class="{ 'is-active': panelMode === 'sources' }" @click="togglePanel('sources')">
              播放源
            </button>
            <button type="button" :class="{ 'is-active': panelMode === 'info' }" @click="togglePanel('info')">
              媒体信息
            </button>
            <button v-if="supportsPictureInPicture" type="button" @click="togglePictureInPicture">
              画中画
            </button>
            <button type="button" @click="toggleFullscreen">
              {{ isFullscreen ? '退出全屏' : '全屏' }}
            </button>
          </div>
        </div>

        <div v-if="panelMode" class="jelly-video-player__panel">
          <template v-if="panelMode === 'subtitles'">
            <div class="jelly-video-player__panel-header">
              <h2>字幕</h2>
              <span>{{ selectedSubtitleLabel }}</span>
            </div>
            <div class="jelly-video-player__option-list">
              <button
                type="button"
                :class="{ 'is-active': selectedSubtitleKey === 'off' }"
                @click="selectedSubtitleKey = 'off'"
              >
                关闭字幕
              </button>
              <button
                v-for="track in subtitleOptions"
                :key="track.key"
                type="button"
                :class="{ 'is-active': selectedSubtitleKey === track.key }"
                @click="selectedSubtitleKey = track.key"
              >
                <strong>{{ track.label }}</strong>
                <span>{{ track.srclang.toUpperCase() }}</span>
              </button>
            </div>
          </template>

          <template v-else-if="panelMode === 'sources'">
            <div class="jelly-video-player__panel-header">
              <h2>播放源</h2>
              <span>{{ item.MediaSources?.length || 0 }} 个版本</span>
            </div>
            <div class="jelly-video-player__option-list">
              <button
                v-for="(source, index) in item.MediaSources"
                :key="source.Id"
                type="button"
                :class="{ 'is-active': currentSourceIndex === index }"
                @click="currentSourceIndex = index"
              >
                <strong>{{ sourceLabel(source, index) }}</strong>
                <span>{{ source.Size ? fileSize(source.Size) : '直连播放' }}</span>
              </button>
            </div>
          </template>

          <template v-else>
            <div class="jelly-video-player__panel-header">
              <h2>媒体信息</h2>
              <span>{{ currentStreams.length }} 条流</span>
            </div>
            <div class="jelly-video-player__info-grid">
              <div>
                <strong>当前源</strong>
                <span>{{ currentSourceLabel }}</span>
              </div>
              <div>
                <strong>字幕状态</strong>
                <span>{{ selectedSubtitleLabel }}</span>
              </div>
              <div>
                <strong>音频轨道</strong>
                <span>{{ audioStreams.length }} 条</span>
              </div>
              <div>
                <strong>字幕轨道</strong>
                <span>{{ subtitleStreams.length }} 条</span>
              </div>
            </div>
            <div class="streams compact">
              <div v-for="stream in currentStreams" :key="`${stream.Type}-${stream.Index}`">
                <strong>{{ streamLabel(stream.Type) }}</strong>
                <span>{{ streamText(stream) || '默认轨道' }}</span>
              </div>
            </div>
          </template>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.jelly-video-player {
  position: relative;
  width: 100%;
  min-height: 100vh;
  background: #000;
  overflow: hidden;
}

.jelly-video-player__surface {
  width: 100%;
  height: 100vh;
  object-fit: contain;
  background: #000;
}

.jelly-video-player__overlay {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  transition: opacity 180ms ease;
}

.jelly-video-player__overlay.is-hidden {
  opacity: 0;
  pointer-events: none;
}

.jelly-video-player__top,
.jelly-video-player__bottom {
  width: min(1720px, 100%);
  margin: 0 auto;
  padding: 20px 24px;
}

.jelly-video-player__top {
  display: grid;
  gap: 20px;
  background: linear-gradient(180deg, rgba(6, 8, 12, 0.86) 0%, rgba(6, 8, 12, 0) 100%);
}

.jelly-video-player__bottom {
  display: grid;
  gap: 16px;
  background: linear-gradient(0deg, rgba(6, 8, 12, 0.92) 0%, rgba(6, 8, 12, 0) 100%);
}

.jelly-video-player__nav {
  display: flex;
  justify-content: flex-start;
}

.jelly-video-player__heading {
  display: grid;
  gap: 8px;
  max-width: min(960px, 100%);
}

.jelly-video-player__heading p,
.jelly-video-player__heading span,
.jelly-video-player__title span,
.jelly-video-player__status {
  color: #c8d2df;
}

.jelly-video-player__heading h1 {
  font-size: clamp(2rem, 5vw, 4rem);
  line-height: 1.05;
}

.jelly-video-player__center {
  display: grid;
  justify-items: center;
  align-content: center;
  gap: 12px;
  padding: 0 24px;
  pointer-events: none;
}

.jelly-video-player__hero-button,
.jelly-video-player__top button,
.jelly-video-player__transport button,
.jelly-video-player__actions button,
.jelly-video-player__option-list button {
  pointer-events: auto;
}

.jelly-video-player__hero-button {
  min-width: 120px;
  padding: 14px 24px;
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 8px;
  background: rgba(13, 18, 27, 0.8);
  color: #f7f8fb;
  backdrop-filter: blur(14px);
}

.jelly-video-player__ghost {
  min-height: 42px;
  padding: 0 18px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 8px;
  background: rgba(13, 18, 27, 0.6);
  color: #f7f8fb;
}

.jelly-video-player__timeline {
  display: grid;
  grid-template-columns: 62px 1fr 62px;
  gap: 12px;
  align-items: center;
}

.jelly-video-player__timeline span {
  color: #d8e1ee;
  font-size: 0.94rem;
  text-align: center;
}

.jelly-video-player__timeline input,
.jelly-video-player__volume input {
  width: 100%;
  accent-color: #00a4dc;
}

.jelly-video-player__controls {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  gap: 20px;
  align-items: center;
}

.jelly-video-player__title {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.jelly-video-player__title strong,
.jelly-video-player__title span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.jelly-video-player__transport,
.jelly-video-player__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: center;
}

.jelly-video-player__transport {
  justify-content: center;
}

.jelly-video-player__actions {
  justify-content: flex-end;
}

.jelly-video-player__transport button,
.jelly-video-player__actions button,
.jelly-video-player__option-list button {
  min-height: 42px;
  padding: 0 16px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 8px;
  background: rgba(13, 18, 27, 0.72);
  color: #f7f8fb;
}

.jelly-video-player__transport button.is-primary,
.jelly-video-player__actions button.is-active,
.jelly-video-player__option-list button.is-active {
  border-color: rgba(0, 164, 220, 0.62);
  background: rgba(0, 164, 220, 0.2);
}

.jelly-video-player__volume {
  display: grid;
  grid-template-columns: auto minmax(108px, 160px);
  gap: 10px;
  align-items: center;
  min-height: 42px;
  padding: 0 12px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 8px;
  background: rgba(13, 18, 27, 0.72);
}

.jelly-video-player__panel {
  justify-self: end;
  width: min(420px, 100%);
  display: grid;
  gap: 14px;
  padding: 16px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 8px;
  background: rgba(10, 12, 16, 0.88);
  backdrop-filter: blur(18px);
}

.jelly-video-player__panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.jelly-video-player__panel-header span,
.jelly-video-player__option-list span,
.jelly-video-player__info-grid span {
  color: #a8b2c1;
}

.jelly-video-player__option-list {
  display: grid;
  gap: 10px;
}

.jelly-video-player__option-list button {
  display: grid;
  gap: 4px;
  justify-items: start;
  text-align: left;
}

.jelly-video-player__info-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.jelly-video-player__info-grid > div {
  display: grid;
  gap: 4px;
}

@media (max-width: 1080px) {
  .jelly-video-player__controls {
    grid-template-columns: 1fr;
  }

  .jelly-video-player__title,
  .jelly-video-player__transport,
  .jelly-video-player__actions {
    justify-content: flex-start;
  }

  .jelly-video-player__panel {
    justify-self: stretch;
    width: 100%;
  }
}

@media (max-width: 720px) {
  .jelly-video-player__top,
  .jelly-video-player__bottom {
    padding: 16px;
  }

  .jelly-video-player__timeline {
    grid-template-columns: 52px 1fr 52px;
    gap: 8px;
  }

  .jelly-video-player__volume {
    grid-template-columns: 1fr;
    justify-items: start;
    padding: 10px 12px;
  }

  .jelly-video-player__info-grid {
    grid-template-columns: 1fr;
  }
}
</style>
