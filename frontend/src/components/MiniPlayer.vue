<script setup lang="ts">
import { computed, ref } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import {
  api,
  continueWatching,
  playQueue,
  playQueueIndex,
  playbackItem,
  playbackPaused,
  playbackCurrentTime,
  playbackDuration,
  playbackVolume,
  playbackMuted,
  hasPlaybackCallbacks,
  miniPlayerSeek,
  miniPlayerTogglePause,
  miniPlayerSetVolume,
  miniPlayerStop,
  miniPlayerPrev,
  miniPlayerNext
} from '../store/app';
import { playbackRoute } from '../utils/navigation';
import type { BaseItemDto } from '../api/emby';

const route = useRoute();
const router = useRouter();
const volumeOpen = ref(false);

const isActivePlayback = computed(() => playbackItem.value && hasPlaybackCallbacks.value);

const current = computed<BaseItemDto | null>(() => {
  if (route.meta.layout === 'fullpage') return null;
  if (isActivePlayback.value) return playbackItem.value;
  return playQueue.value[playQueueIndex.value] || continueWatching.value[0] || null;
});

const cover = computed(() => {
  const item = current.value;
  if (!item) return '';
  return api.itemImageUrl(item);
});

const subtitle = computed(() => {
  const item = current.value;
  if (!item) return '';
  if (item.SeriesName) {
    let text = item.SeriesName;
    if (item.ParentIndexNumber && item.IndexNumber) {
      text += ` · S${String(item.ParentIndexNumber).padStart(2, '0')}E${String(item.IndexNumber).padStart(2, '0')}`;
    }
    return text;
  }
  if (item.ProductionYear) return String(item.ProductionYear);
  return item.Type || '';
});

const progressPercent = computed(() => {
  if (!isActivePlayback.value) {
    const item = current.value;
    if (item?.UserData?.PlaybackPositionTicks && item.RunTimeTicks) {
      return (item.UserData.PlaybackPositionTicks / item.RunTimeTicks) * 100;
    }
    return 0;
  }
  if (!playbackDuration.value) return 0;
  return (playbackCurrentTime.value / playbackDuration.value) * 100;
});

const hasPrev = computed(() => playQueueIndex.value > 0 && playQueue.value.length > 1);
const hasNext = computed(() =>
  playQueueIndex.value + 1 < playQueue.value.length
);

const volumeIcon = computed(() => {
  if (playbackMuted.value || playbackVolume.value === 0) return 'i-lucide-volume-x';
  if (playbackVolume.value < 0.5) return 'i-lucide-volume-1';
  return 'i-lucide-volume-2';
});

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

function seekFromClick(e: MouseEvent) {
  if (!isActivePlayback.value || !playbackDuration.value) return;
  const bar = e.currentTarget as HTMLElement;
  const rect = bar.getBoundingClientRect();
  const percent = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
  miniPlayerSeek(percent * playbackDuration.value);
}

function open() {
  if (current.value) void router.push(playbackRoute(current.value));
}

function onVolumeInput(e: Event) {
  const v = Number((e.target as HTMLInputElement).value);
  miniPlayerSetVolume(v);
}
</script>

<template>
  <Transition
    enter-active-class="transition duration-300"
    leave-active-class="transition duration-300"
    enter-from-class="opacity-0 translate-y-full"
    leave-to-class="opacity-0 translate-y-full"
  >
    <div
      v-if="current"
      class="pointer-events-auto fixed inset-x-0 bottom-0 z-50 hidden md:block"
    >
      <div class="bg-[#181818]/95 backdrop-blur-md border-t border-white/10 shadow-2xl">
        <!-- 顶部进度条 -->
        <div
          class="group relative h-1 cursor-pointer bg-white/10 transition-[height] hover:h-1.5"
          @click="seekFromClick"
        >
          <div
            class="bg-primary absolute inset-y-0 left-0 transition-[width] duration-100"
            :style="{ width: `${progressPercent}%` }"
          />
          <div
            class="bg-primary absolute top-1/2 size-3 -translate-y-1/2 rounded-full opacity-0 shadow transition-opacity group-hover:opacity-100"
            :style="{ left: `calc(${progressPercent}% - 6px)` }"
          />
        </div>

        <!-- 主体内容 -->
        <div class="flex h-16 items-center gap-3 px-4">
          <!-- 左侧：封面 + 信息 -->
          <button
            type="button"
            class="flex min-w-0 flex-1 items-center gap-3 text-left"
            @click="open"
          >
            <div class="bg-white/5 relative h-12 w-9 shrink-0 overflow-hidden rounded">
              <img
                v-if="cover"
                :src="cover"
                :alt="current.Name"
                class="h-full w-full object-cover"
              />
            </div>
            <div class="min-w-0">
              <p class="truncate text-sm font-medium text-white">{{ current.Name }}</p>
              <p v-if="subtitle" class="truncate text-xs text-white/50">{{ subtitle }}</p>
            </div>
          </button>

          <!-- 中央：控件 -->
          <div class="flex items-center gap-1">
            <template v-if="isActivePlayback">
              <UButton
                v-if="hasPrev"
                color="neutral"
                variant="ghost"
                icon="i-lucide-skip-back"
                size="sm"
                class="!text-white/70 hover:!text-white"
                @click="miniPlayerPrev"
              />
              <UButton
                color="neutral"
                variant="ghost"
                :icon="playbackPaused ? 'i-lucide-play' : 'i-lucide-pause'"
                size="md"
                class="!text-white"
                @click="miniPlayerTogglePause"
              />
              <UButton
                v-if="hasNext"
                color="neutral"
                variant="ghost"
                icon="i-lucide-skip-forward"
                size="sm"
                class="!text-white/70 hover:!text-white"
                @click="miniPlayerNext"
              />

              <span class="mx-2 hidden text-xs tabular-nums text-white/50 lg:inline">
                {{ fmt(playbackCurrentTime) }} / {{ fmt(playbackDuration) }}
              </span>
            </template>

            <template v-else>
              <UButton
                color="neutral"
                variant="ghost"
                icon="i-lucide-play"
                size="md"
                class="!text-white"
                @click="open"
              />
            </template>
          </div>

          <!-- 右侧：音量 + 关闭 -->
          <div class="flex shrink-0 items-center gap-1">
            <template v-if="isActivePlayback">
              <div
                class="relative flex items-center"
                @mouseenter="volumeOpen = true"
                @mouseleave="volumeOpen = false"
              >
                <UButton
                  color="neutral"
                  variant="ghost"
                  :icon="volumeIcon"
                  size="sm"
                  class="!text-white/70 hover:!text-white"
                  @click="miniPlayerSetVolume(playbackMuted ? 1 : 0)"
                />
                <Transition
                  enter-active-class="transition duration-150"
                  leave-active-class="transition duration-150"
                  enter-from-class="opacity-0 w-0"
                  leave-to-class="opacity-0 w-0"
                >
                  <input
                    v-show="volumeOpen"
                    type="range"
                    class="mini-volume-slider ml-1 h-1 w-20 cursor-pointer appearance-none rounded bg-white/20"
                    min="0"
                    max="1"
                    step="0.01"
                    :value="playbackMuted ? 0 : playbackVolume"
                    @input="onVolumeInput"
                  />
                </Transition>
              </div>

              <UButton
                color="neutral"
                variant="ghost"
                icon="i-lucide-x"
                size="sm"
                class="!text-white/40 hover:!text-white"
                @click="miniPlayerStop"
              />
            </template>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.mini-volume-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 10px;
  height: 10px;
  background: white;
  border-radius: 50%;
  cursor: pointer;
}
.mini-volume-slider::-moz-range-thumb {
  width: 10px;
  height: 10px;
  background: white;
  border-radius: 50%;
  border: none;
  cursor: pointer;
}
</style>
