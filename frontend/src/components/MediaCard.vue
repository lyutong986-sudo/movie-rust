<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { BaseItemDto } from '../api/emby';
import { api, itemSubtitle } from '../store/app';

const props = defineProps<{
  item: BaseItemDto;
  subtitle?: string;
  thumb?: boolean;
}>();

const emit = defineEmits<{
  play: [item: BaseItemDto];
  select: [item: BaseItemDto];
}>();

const imageError = ref(false);

watch(
  () => props.item.Id,
  () => {
    imageError.value = false;
  }
);

const imageUrl = computed(() => {
  if (props.thumb) {
    return api.backdropUrl(props.item) || api.itemImageUrl(props.item);
  }

  return api.itemImageUrl(props.item) || api.backdropUrl(props.item);
});

const showImage = computed(() => Boolean(imageUrl.value) && !imageError.value);

const title = computed(() =>
  props.item.Type === 'Episode' && props.item.SeriesName ? props.item.SeriesName : props.item.Name
);
const secondary = computed(() => props.subtitle || itemSubtitle(props.item));
const playable = computed(() => !props.item.IsFolder && Boolean(props.item.MediaSources?.length));
const fallbackLabel = computed(() => {
  if (props.item.IsFolder) {
    return '目录';
  }

  return (props.item.Name || '').slice(0, 1).toUpperCase() || '?';
});

const progress = computed(() => {
  const ticks = props.item.UserData?.PlaybackPositionTicks ?? 0;
  const runtime = props.item.RunTimeTicks ?? 0;
  if (!ticks || !runtime) return 0;
  return Math.max(0, Math.min(100, (ticks / runtime) * 100));
});
</script>

<template>
  <article
    class="group relative flex cursor-pointer flex-col gap-2 transition"
    @click="emit('select', props.item)"
  >
    <div
      class="relative overflow-hidden rounded-lg bg-elevated ring-1 ring-default transition-all group-hover:ring-primary/60 group-hover:shadow-lg"
      :class="props.thumb ? 'aspect-video' : 'aspect-[2/3]'"
    >
      <img
        v-if="showImage"
        :src="imageUrl"
        :alt="props.item.Name"
        loading="lazy"
        decoding="async"
        class="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
        @error="imageError = true"
      />
      <div
        v-else
        class="flex h-full w-full items-center justify-center bg-gradient-to-br from-primary/20 to-primary/5 text-2xl font-bold text-primary"
      >
        {{ fallbackLabel }}
      </div>

      <!-- 进度条 -->
      <div
        v-if="progress > 0"
        class="absolute inset-x-0 bottom-0 h-1 bg-black/40"
      >
        <div class="h-full bg-primary" :style="{ width: `${progress}%` }" />
      </div>

      <!-- 悬浮播放按钮 -->
      <button
        v-if="playable"
        type="button"
        title="播放"
        class="absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 transition-opacity group-hover:opacity-100"
        @click.stop="emit('play', props.item)"
      >
        <span
          class="flex h-12 w-12 items-center justify-center rounded-full bg-primary text-primary-contrast shadow-lg"
        >
          <UIcon name="i-lucide-play" class="size-6" />
        </span>
      </button>

      <!-- 状态角标 -->
      <span
        v-if="props.item.UserData?.Played"
        class="absolute right-2 top-2 inline-flex h-6 w-6 items-center justify-center rounded-full bg-primary text-primary-contrast text-xs"
      >
        <UIcon name="i-lucide-check" class="size-4" />
      </span>
      <span
        v-else-if="props.item.UserData?.IsFavorite"
        class="absolute right-2 top-2 inline-flex h-6 w-6 items-center justify-center rounded-full bg-error text-white text-xs"
      >
        <UIcon name="i-lucide-heart" class="size-3.5" />
      </span>
    </div>

    <div class="min-w-0 space-y-0.5 px-0.5">
      <h3
        class="text-highlighted truncate text-sm font-medium transition-colors group-hover:text-primary"
        :title="title"
      >
        {{ title }}
      </h3>
      <p class="text-muted truncate text-xs" :title="secondary">{{ secondary }}</p>
    </div>
  </article>
</template>
