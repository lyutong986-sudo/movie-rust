<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { BaseItemDto } from '../api/emby';
import {
  api,
  enqueue,
  isInWatchLater,
  itemSubtitle,
  toggleFavorite,
  togglePlayed,
  toggleWatchLater
} from '../store/app';
import MediaQualityBadges from './MediaQualityBadges.vue';

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

const logoUrl = computed(() => api.logoUrl(props.item));
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

// 右键 / 更多菜单
const menuItems = computed(() => {
  const list: Array<Array<{ label: string; icon: string; onSelect?: () => void }>> = [];
  const actions: Array<{ label: string; icon: string; onSelect?: () => void }> = [];
  if (playable.value) {
    actions.push({ label: '播放', icon: 'i-lucide-play', onSelect: () => emit('play', props.item) });
    actions.push({
      label: '加入队列',
      icon: 'i-lucide-list-plus',
      onSelect: () => enqueue(props.item, 'last')
    });
    actions.push({
      label: '作为下一首',
      icon: 'i-lucide-play-circle',
      onSelect: () => enqueue(props.item, 'next')
    });
  }
  actions.push({
    label: props.item.UserData?.IsFavorite ? '取消收藏' : '收藏',
    icon: props.item.UserData?.IsFavorite ? 'i-lucide-heart-off' : 'i-lucide-heart',
    onSelect: () => void toggleFavorite(props.item)
  });
  actions.push({
    label: props.item.UserData?.Played ? '标记未观看' : '标记已观看',
    icon: props.item.UserData?.Played ? 'i-lucide-eye-off' : 'i-lucide-eye',
    onSelect: () => void togglePlayed(props.item)
  });
  actions.push({
    label: isInWatchLater(props.item.Id) ? '移出稍后观看' : '添加到稍后观看',
    icon: 'i-lucide-clock',
    onSelect: () => toggleWatchLater(props.item)
  });
  list.push(actions);
  list.push([
    {
      label: '查看详情',
      icon: 'i-lucide-info',
      onSelect: () => emit('select', props.item)
    }
  ]);
  return list;
});

const showMenu = ref(false);

function openMenu(e: MouseEvent) {
  e.preventDefault();
  showMenu.value = true;
}
</script>

<template>
  <article
    class="group relative flex cursor-pointer flex-col gap-2 transition"
    @click="emit('select', props.item)"
    @contextmenu="openMenu"
  >
    <div
      class="bg-elevated ring-default group-hover:ring-primary/60 relative overflow-hidden rounded-lg ring-1 transition-all group-hover:-translate-y-0.5 group-hover:shadow-xl"
      :class="props.thumb ? 'aspect-video' : 'aspect-[2/3]'"
    >
      <img
        v-if="showImage"
        :src="imageUrl"
        :alt="props.item.Name"
        loading="lazy"
        decoding="async"
        class="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.04]"
        @error="imageError = true"
      />
      <div
        v-else
        class="from-primary/20 to-primary/5 text-primary flex h-full w-full items-center justify-center bg-gradient-to-br text-2xl font-bold"
      >
        {{ fallbackLabel }}
      </div>

      <!-- Logo clearart 悬浮覆盖 -->
      <div
        v-if="logoUrl"
        class="pointer-events-none absolute inset-x-2 bottom-8 hidden sm:block"
      >
        <img
          :src="logoUrl"
          :alt="props.item.Name"
          class="max-h-10 w-auto drop-shadow-[0_1px_4px_rgba(0,0,0,0.8)] opacity-0 transition-opacity duration-300 group-hover:opacity-100"
        />
      </div>

      <!-- 质量角标 -->
      <div class="absolute left-2 top-2 flex flex-col items-start gap-1">
        <MediaQualityBadges :item="props.item" compact />
      </div>

      <!-- 进度条 -->
      <div v-if="progress > 0" class="absolute inset-x-0 bottom-0 h-1 bg-black/40">
        <div class="bg-primary h-full" :style="{ width: `${progress}%` }" />
      </div>

      <!-- 悬浮播放 -->
      <button
        v-if="playable"
        type="button"
        title="播放"
        class="absolute inset-0 flex items-center justify-center bg-gradient-to-t from-black/70 via-black/20 to-transparent opacity-0 transition-opacity group-hover:opacity-100"
        @click.stop="emit('play', props.item)"
      >
        <span
          class="bg-primary text-primary-contrast flex h-12 w-12 items-center justify-center rounded-full shadow-lg ring-4 ring-black/20"
        >
          <UIcon name="i-lucide-play" class="size-6" />
        </span>
      </button>

      <!-- 右上角状态 -->
      <span
        v-if="props.item.UserData?.Played"
        class="bg-primary text-primary-contrast absolute right-2 top-2 inline-flex h-6 w-6 items-center justify-center rounded-full text-xs"
      >
        <UIcon name="i-lucide-check" class="size-4" />
      </span>
      <span
        v-else-if="props.item.UserData?.IsFavorite"
        class="bg-error absolute right-2 top-2 inline-flex h-6 w-6 items-center justify-center rounded-full text-xs text-white"
      >
        <UIcon name="i-lucide-heart" class="size-3.5" />
      </span>

      <!-- 更多按钮 -->
      <UDropdownMenu v-model:open="showMenu" :items="menuItems">
        <UButton
          icon="i-lucide-more-horizontal"
          color="neutral"
          variant="solid"
          size="xs"
          class="absolute bottom-2 right-2 opacity-0 transition-opacity group-hover:opacity-100"
          aria-label="更多"
          @click.stop
        />
      </UDropdownMenu>
    </div>

    <div class="min-w-0 space-y-0.5 px-0.5">
      <h3
        class="text-highlighted group-hover:text-primary truncate text-sm font-medium transition-colors"
        :title="title"
      >
        {{ title }}
      </h3>
      <p class="text-muted truncate text-xs" :title="secondary">{{ secondary }}</p>
    </div>
  </article>
</template>
