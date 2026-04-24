<script setup lang="ts">
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import MediaCard from '../components/MediaCard.vue';
import EmptyState from '../components/EmptyState.vue';
import {
  clearQueue,
  dequeueAt,
  playQueue,
  playQueueIndex,
  toggleWatchLater,
  watchLater
} from '../store/app';
import type { BaseItemDto } from '../api/emby';
import { itemRoute, playbackRoute } from '../utils/navigation';

const router = useRouter();

const hasQueue = computed(() => playQueue.value.length > 0);
const hasLater = computed(() => watchLater.value.length > 0);

async function openItem(item: BaseItemDto) {
  await router.push(itemRoute(item));
}

async function playItem(item: BaseItemDto, index?: number) {
  if (typeof index === 'number') {
    playQueueIndex.value = index;
  }
  await router.push(playbackRoute(item));
}
</script>

<template>
  <div class="space-y-8">
    <section class="space-y-3">
      <div class="flex items-baseline justify-between">
        <h2 class="text-highlighted text-lg font-semibold">播放队列</h2>
        <UButton
          v-if="hasQueue"
          size="xs"
          color="neutral"
          variant="soft"
          icon="i-lucide-trash-2"
          @click="clearQueue"
        >
          清空
        </UButton>
      </div>

      <div
        v-if="hasQueue"
        class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6"
      >
        <div
          v-for="(item, idx) in playQueue"
          :key="item.Id"
          class="relative"
          :class="{ 'ring-primary rounded-lg ring-2 ring-offset-2 ring-offset-transparent': idx === playQueueIndex }"
        >
          <MediaCard :item="item" @play="playItem(item, idx)" @select="openItem(item)" />
          <UButton
            class="absolute right-1.5 top-1.5"
            size="xs"
            color="neutral"
            variant="solid"
            icon="i-lucide-x"
            aria-label="从队列移除"
            @click="dequeueAt(idx)"
          />
        </div>
      </div>
      <EmptyState
        v-else
        compact
        icon="i-lucide-list-video"
        title="队列空空如也"
        description="在任一详情页或媒体卡的更多菜单中选择「加入队列」即可添加内容。"
        action-label="去浏览"
        action-icon="i-lucide-home"
        @action="router.push('/')"
      />
    </section>

    <section class="space-y-3">
      <div class="flex items-baseline justify-between">
        <h2 class="text-highlighted text-lg font-semibold">稍后观看</h2>
        <span class="text-muted text-sm">{{ watchLater.length }} 项</span>
      </div>
      <div
        v-if="hasLater"
        class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6"
      >
        <div v-for="item in watchLater" :key="item.Id" class="relative">
          <MediaCard :item="item" @play="playItem(item)" @select="openItem(item)" />
          <UButton
            class="absolute right-1.5 top-1.5"
            size="xs"
            color="neutral"
            variant="solid"
            icon="i-lucide-bookmark-minus"
            aria-label="从稍后观看移除"
            @click="toggleWatchLater(item)"
          />
        </div>
      </div>
      <EmptyState
        v-else
        compact
        icon="i-lucide-clock"
        title="暂无稍后观看"
        description="在媒体卡的更多菜单里选择「添加到稍后观看」即可收藏。"
      />
    </section>
  </div>
</template>
