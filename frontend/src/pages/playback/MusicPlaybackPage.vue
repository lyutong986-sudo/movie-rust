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
  currentTrack.value ? api.itemImageUrl(currentTrack.value) || api.backdropUrl(currentTrack.value) : ''
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
          siblings.Items.findIndex((item) => item.Id === firstItem.Id)
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
  if (!queue.value.length) {
    return;
  }

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
  if (!currentTrack.value || !playSessionId.value || !audioRef.value) {
    return;
  }

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
  if (!currentTrack.value || !playSessionId.value || hasStopped || !audioRef.value) {
    return;
  }

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
  if (!currentTrack.value) {
    return;
  }

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
  if (!audioRef.value) {
    return;
  }

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
  if (!audioRef.value) {
    return;
  }

  if (audioRef.value.paused) {
    void audioRef.value.play();
  } else {
    audioRef.value.pause();
  }
}

function seek(event: Event) {
  if (!audioRef.value) {
    return;
  }

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
  <section v-if="loading" class="player-loading">
    <p>正在准备音乐播放器</p>
  </section>

  <section v-else-if="error || !currentTrack" class="player-loading">
    <div>
      <p>播放失败</p>
      <h2>{{ error || '没有找到音频内容' }}</h2>
      <button type="button" @click="router.back()">返回</button>
    </div>
  </section>

  <section v-else class="player-shell music-player-shell">
    <audio
      ref="audioRef"
      :src="sourceUrl"
      autoplay
      @play="handlePlay"
      @pause="handlePause"
      @timeupdate="handleTimeUpdate"
      @ended="handleEnded"
    />

    <div class="music-stage">
      <div class="music-cover">
        <img v-if="coverImage" :src="coverImage" :alt="currentTrack.Name" />
        <div v-else class="poster-fallback">{{ currentTrack.Name.slice(0, 1).toUpperCase() }}</div>
      </div>

      <div class="music-panel">
        <p>正在播放</p>
        <h2>{{ currentTrack.Name }}</h2>
        <span>{{ currentTrack.SeriesName || currentTrack.SeasonName || currentTrack.Type }}</span>

        <div class="button-row">
          <button class="secondary" type="button" @click="router.back()">返回</button>
        </div>

        <div class="music-progress">
          <span>{{ timeText(currentTime) }}</span>
          <input type="range" min="0" max="100" step="0.1" :value="progressValue" @input="seek" />
          <span>{{ timeText(duration) }}</span>
        </div>

        <div class="button-row">
          <button class="secondary" type="button" :disabled="currentIndex === 0" @click="previousTrack">
            上一首
          </button>
          <button type="button" @click="togglePlayback">{{ paused ? '播放' : '暂停' }}</button>
          <button
            class="secondary"
            type="button"
            :disabled="currentIndex >= queue.length - 1"
            @click="nextTrack"
          >
            下一首
          </button>
        </div>

        <section class="queue-panel">
          <div class="section-heading">
            <h3>播放队列</h3>
            <span>{{ queue.length }} 首</span>
          </div>
          <div class="track-list">
            <button
              v-for="(track, index) in queue"
              :key="track.Id"
              type="button"
              :class="{ active: index === currentIndex }"
              @click="prepareTrack(index)"
            >
              <strong>{{ track.Name }}</strong>
              <span>{{ track.ProductionYear || track.Type }}</span>
            </button>
          </div>
        </section>
      </div>
    </div>
  </section>
</template>
