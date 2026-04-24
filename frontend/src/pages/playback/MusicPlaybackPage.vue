<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type { BaseItemDto } from '../../api/emby';
import { api } from '../../store/app';

const route = useRoute();
const router = useRouter();

const audioRef = ref<HTMLAudioElement | null>(null);
const loading = ref(false);
const error = ref('');
const queue = ref<BaseItemDto[]>([]);
const currentIndex = ref(0);
const playSessionId = ref('');
const duration = ref(0);
const currentTime = ref(0);
const paused = ref(true);

let hasStarted = false;
let hasStopped = false;
let lastProgressSecond = -10;

const itemId = computed(() => String(route.query.itemId || ''));
const currentTrack = computed(() => queue.value[currentIndex.value] || null);
const coverImage = computed(() =>
  currentTrack.value
    ? api.itemImageUrl(currentTrack.value) || api.backdropUrl(currentTrack.value)
    : ''
);
const progressValue = computed(() =>
  duration.value ? Math.min(100, (currentTime.value / duration.value) * 100) : 0
);
const sourceUrl = computed(() => (currentTrack.value ? api.streamUrl(currentTrack.value) : ''));

watch(
  () => route.query.itemId,
  async (value) => {
    if (typeof value === 'string' && value) {
      await loadQueue(value);
    }
  },
  { immediate: true }
);

onBeforeUnmount(async () => {
  await stopPlayback();
});

async function loadQueue(startId: string) {
  loading.value = true;
  error.value = '';
  try {
    const firstItem = await api.item(startId);
    let items = [firstItem];
    let startIndex = 0;

    if (firstItem.Type === 'Audio' && firstItem.ParentId) {
      const siblings = await api.items(firstItem.ParentId, '', false, {
        includeTypes: ['Audio'],
        sortBy: 'IndexNumber',
        sortOrder: 'Ascending',
        limit: 300
      });
      if (siblings.Items.length) {
        items = siblings.Items;
        startIndex = Math.max(
          0,
          siblings.Items.findIndex((i) => i.Id === firstItem.Id)
        );
      }
    } else if (firstItem.IsFolder || firstItem.Type === 'MusicAlbum') {
      const children = await api.items(firstItem.Id, '', true, {
        includeTypes: ['Audio'],
        sortBy: 'SortName',
        sortOrder: 'Ascending',
        limit: 300
      });
      if (children.Items.length) {
        items = children.Items;
        startIndex = 0;
      }
    }

    queue.value = items;
    await prepareTrack(startIndex);
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : String(loadError);
    queue.value = [];
  } finally {
    loading.value = false;
  }
}

async function prepareTrack(index: number) {
  if (!queue.value.length) return;
  await stopPlayback();

  const target = queue.value[index];
  const [fullTrack, playback] = await Promise.all([
    api.item(target.Id),
    api.playbackInfo(target.Id)
  ]);

  queue.value[index] = {
    ...fullTrack,
    MediaSources: playback.MediaSources,
    MediaStreams: playback.MediaSources[0]?.MediaStreams || fullTrack.MediaStreams
  };

  currentIndex.value = index;
  playSessionId.value = playback.PlaySessionId;
  currentTime.value = 0;
  duration.value = 0;
  paused.value = true;
  hasStarted = false;
  hasStopped = false;
  lastProgressSecond = -10;

  if (itemId.value !== queue.value[index].Id) {
    await router.replace({
      path: '/playback/music',
      query: { itemId: queue.value[index].Id }
    });
  }

  await nextTick();
  if (audioRef.value) {
    audioRef.value.load();
    try {
      await audioRef.value.play();
    } catch {
      // Ignore autoplay failures.
    }
  }
}

function toTicks(seconds: number) {
  return Math.max(0, Math.round(seconds * 10_000_000));
}

async function reportProgress(isPaused = false) {
  if (!currentTrack.value || !playSessionId.value || !audioRef.value) return;
  await api.playbackProgress({
    ItemId: currentTrack.value.Id,
    PlaySessionId: playSessionId.value,
    MediaSourceId: currentTrack.value.MediaSources?.[0]?.Id,
    PositionTicks: toTicks(audioRef.value.currentTime),
    IsPaused: isPaused,
    PlayedToCompletion: false
  });
}

async function stopPlayback(forceCompleted = false) {
  if (!currentTrack.value || !playSessionId.value || hasStopped || !audioRef.value) return;
  hasStopped = true;
  await api.playbackStopped({
    ItemId: currentTrack.value.Id,
    PlaySessionId: playSessionId.value,
    MediaSourceId: currentTrack.value.MediaSources?.[0]?.Id,
    PositionTicks: toTicks(audioRef.value.currentTime),
    IsPaused: paused.value,
    PlayedToCompletion: forceCompleted
  });
}

async function handlePlay() {
  paused.value = false;
  if (!currentTrack.value) return;
  if (!hasStarted) {
    hasStarted = true;
    await api.playbackStarted({
      ItemId: currentTrack.value.Id,
      PlaySessionId: playSessionId.value,
      MediaSourceId: currentTrack.value.MediaSources?.[0]?.Id,
      PositionTicks: toTicks(audioRef.value?.currentTime || 0)
    });
  } else {
    await reportProgress(false);
  }
}

async function handlePause() {
  paused.value = true;
  await reportProgress(true);
}

function handleTimeUpdate() {
  if (!audioRef.value) return;
  currentTime.value = audioRef.value.currentTime;
  duration.value = Number.isFinite(audioRef.value.duration) ? audioRef.value.duration : 0;

  const second = Math.floor(currentTime.value);
  if (second - lastProgressSecond >= 10) {
    lastProgressSecond = second;
    void reportProgress(false);
  }
}

async function handleEnded() {
  paused.value = true;
  if (currentIndex.value < queue.value.length - 1) {
    await prepareTrack(currentIndex.value + 1);
    return;
  }
  await stopPlayback(true);
}

async function previousTrack() {
  if (currentIndex.value > 0) {
    await prepareTrack(currentIndex.value - 1);
  }
}

async function nextTrack() {
  if (currentIndex.value < queue.value.length - 1) {
    await prepareTrack(currentIndex.value + 1);
  }
}

function togglePlayback() {
  if (!audioRef.value) return;
  if (audioRef.value.paused) {
    void audioRef.value.play();
  } else {
    audioRef.value.pause();
  }
}

function seek(event: Event) {
  if (!audioRef.value) return;
  const target = event.target as HTMLInputElement;
  const ratio = Number(target.value) / 100;
  audioRef.value.currentTime = duration.value * ratio;
}

function timeText(value: number) {
  const totalSeconds = Math.max(0, Math.floor(value));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}
</script>

<template>
  <div
    v-if="loading"
    class="fixed inset-0 z-50 flex flex-col items-center justify-center gap-3 bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 text-white"
  >
    <UProgress animation="carousel" class="w-48" />
    <p class="text-sm opacity-80">正在准备音乐播放器…</p>
  </div>

  <div
    v-else-if="error || !currentTrack"
    class="fixed inset-0 z-50 flex flex-col items-center justify-center gap-3 bg-slate-950 text-white"
  >
    <UIcon name="i-lucide-circle-alert" class="size-12 opacity-80" />
    <h2 class="text-xl font-semibold">{{ error || '没有找到音频内容' }}</h2>
    <UButton color="neutral" variant="subtle" icon="i-lucide-arrow-left" @click="router.back()">返回</UButton>
  </div>

  <div
    v-else
    class="relative h-screen w-screen overflow-hidden bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 text-white"
  >
    <img
      v-if="coverImage"
      :src="coverImage"
      :alt="currentTrack.Name"
      class="pointer-events-none absolute inset-0 h-full w-full scale-110 object-cover opacity-30 blur-3xl"
    />
    <audio
      ref="audioRef"
      :src="sourceUrl"
      autoplay
      @play="handlePlay"
      @pause="handlePause"
      @timeupdate="handleTimeUpdate"
      @ended="handleEnded"
    />

    <div class="relative flex h-full flex-col lg:grid lg:grid-cols-[1.2fr_1fr] lg:gap-0">
      <div class="flex flex-col items-center justify-center gap-6 p-6 sm:p-10">
        <div class="aspect-square w-full max-w-md overflow-hidden rounded-2xl bg-white/5 shadow-2xl ring-1 ring-white/10">
          <img
            v-if="coverImage"
            :src="coverImage"
            :alt="currentTrack.Name"
            class="h-full w-full object-cover"
          />
          <div v-else class="flex h-full w-full items-center justify-center text-6xl font-bold text-white/50">
            {{ currentTrack.Name.slice(0, 1).toUpperCase() }}
          </div>
        </div>

        <div class="w-full max-w-md text-center">
          <p class="text-xs uppercase tracking-[0.3em] text-white/50">正在播放</p>
          <h2 class="mt-2 text-2xl font-semibold">{{ currentTrack.Name }}</h2>
          <p class="text-muted mt-1 text-sm text-white/60">
            {{ currentTrack.SeriesName || currentTrack.SeasonName || currentTrack.Type }}
          </p>
        </div>

        <div class="w-full max-w-md">
          <input
            type="range"
            min="0"
            max="100"
            step="0.1"
            :value="progressValue"
            class="h-1 w-full cursor-pointer appearance-none rounded-full bg-white/10 accent-[var(--ui-primary)]"
            @input="seek"
          />
          <div class="mt-1 flex justify-between text-xs text-white/60 tabular-nums">
            <span>{{ timeText(currentTime) }}</span>
            <span>{{ timeText(duration) }}</span>
          </div>
        </div>

        <div class="flex items-center gap-3">
          <UButton
            color="neutral"
            variant="soft"
            size="lg"
            icon="i-lucide-skip-back"
            :disabled="currentIndex === 0"
            @click="previousTrack"
          />
          <UButton
            color="primary"
            variant="solid"
            size="xl"
            :icon="paused ? 'i-lucide-play' : 'i-lucide-pause'"
            @click="togglePlayback"
          />
          <UButton
            color="neutral"
            variant="soft"
            size="lg"
            icon="i-lucide-skip-forward"
            :disabled="currentIndex >= queue.length - 1"
            @click="nextTrack"
          />
          <UButton
            color="neutral"
            variant="ghost"
            size="lg"
            icon="i-lucide-x"
            @click="router.back()"
          />
        </div>
      </div>

      <aside class="flex flex-col overflow-hidden bg-black/30 p-6 backdrop-blur-xl ring-1 ring-white/10">
        <div class="mb-3 flex items-center justify-between">
          <h3 class="text-sm font-semibold">播放队列</h3>
          <span class="text-xs text-white/60">{{ queue.length }} 首</span>
        </div>
        <div class="flex-1 space-y-1 overflow-y-auto pr-2">
          <button
            v-for="(track, index) in queue"
            :key="track.Id"
            type="button"
            class="flex w-full items-start gap-3 rounded-lg p-2 text-start transition"
            :class="
              index === currentIndex
                ? 'bg-white/15 ring-1 ring-white/30'
                : 'hover:bg-white/10'
            "
            @click="prepareTrack(index)"
          >
            <div
              class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-white/10 text-xs text-white/80"
            >
              <UIcon v-if="index === currentIndex && !paused" name="i-lucide-audio-lines" class="size-4 text-primary" />
              <span v-else>{{ index + 1 }}</span>
            </div>
            <div class="min-w-0 flex-1">
              <p class="truncate text-sm">{{ track.Name }}</p>
              <p class="truncate text-xs text-white/50">
                {{ track.ProductionYear || track.Type }}
              </p>
            </div>
          </button>
        </div>
      </aside>
    </div>
  </div>
</template>
