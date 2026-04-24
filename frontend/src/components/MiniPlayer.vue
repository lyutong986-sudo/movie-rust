<script setup lang="ts">
import { computed } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { api, continueWatching, playQueue, playQueueIndex } from '../store/app';
import { playbackRoute } from '../utils/navigation';
import type { BaseItemDto } from '../api/emby';

const route = useRoute();
const router = useRouter();

// 迷你播放器：播放队列首项 or 继续观看首项，提供快速恢复播放入口。
// 仅在主应用（非全屏播放、非登录）显示。
const current = computed<BaseItemDto | null>(() => {
  if (route.meta.layout === 'fullpage') return null;
  return playQueue.value[playQueueIndex.value] || continueWatching.value[0] || null;
});

const cover = computed(() => {
  const item = current.value;
  if (!item) return '';
  return api.backdropUrl(item) || api.itemImageUrl(item);
});

function open() {
  if (current.value) void router.push(playbackRoute(current.value));
}
</script>

<template>
  <Transition
    enter-active-class="transition duration-300"
    leave-active-class="transition duration-300"
    enter-from-class="opacity-0 translate-y-4"
    leave-to-class="opacity-0 translate-y-4"
  >
    <div
      v-if="current"
      class="pointer-events-auto fixed bottom-4 right-4 z-40 hidden w-80 overflow-hidden rounded-xl shadow-xl ring-1 ring-default md:block"
    >
      <button
        type="button"
        class="bg-elevated/90 hover:bg-elevated flex w-full items-center gap-3 p-2 text-left backdrop-blur transition"
        @click="open"
      >
        <div class="bg-default relative h-14 w-24 shrink-0 overflow-hidden rounded-md">
          <img
            v-if="cover"
            :src="cover"
            :alt="current.Name"
            class="h-full w-full object-cover"
          />
          <div class="absolute inset-0 flex items-center justify-center bg-black/30">
            <UIcon name="i-lucide-play" class="size-6 text-white" />
          </div>
        </div>
        <div class="min-w-0 flex-1">
          <p class="text-muted text-[10px] uppercase tracking-wider">继续播放</p>
          <p class="text-highlighted truncate text-sm font-medium">{{ current.Name }}</p>
          <p v-if="current.SeriesName" class="text-muted truncate text-xs">
            {{ current.SeriesName }}
            <template v-if="current.ParentIndexNumber && current.IndexNumber">
              · S{{ String(current.ParentIndexNumber).padStart(2, '0') }}E{{ String(current.IndexNumber).padStart(2, '0') }}
            </template>
          </p>
        </div>
      </button>
    </div>
  </Transition>
</template>
