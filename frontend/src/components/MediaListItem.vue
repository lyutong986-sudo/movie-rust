<script setup lang="ts">
import { computed, ref } from 'vue';
import type { BaseItemDto } from '../api/emby';
import { api } from '../store/app';
import MediaQualityBadges from './MediaQualityBadges.vue';

const props = withDefaults(
  defineProps<{
    item: BaseItemDto;
    detailed?: boolean;
  }>(),
  { detailed: false }
);

const emit = defineEmits<{
  select: [item: BaseItemDto];
}>();

const imageError = ref(false);

const imageUrl = computed(() => api.itemImageUrl(props.item) || api.backdropUrl(props.item));

const runtime = computed(() => {
  const ticks = props.item.RunTimeTicks;
  if (!ticks) return '';
  const totalMinutes = Math.round(ticks / 600_000_000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours > 0) return `${hours}小时${minutes}分钟`;
  return `${minutes}分钟`;
});

const genres = computed(() => (props.item.Genres || []).join(' / '));

const rating = computed(() => {
  const r = props.item.CommunityRating;
  return r ? r.toFixed(1) : '';
});

const resolution = computed(() => {
  const vs =
    props.item.MediaStreams?.find((s) => s.Type === 'Video') ||
    props.item.MediaSources?.[0]?.MediaStreams?.find((s) => s.Type === 'Video');
  if (!vs) return '';
  if (vs.Width && vs.Height) return `${vs.Width}×${vs.Height}`;
  return '';
});

const progress = computed(() => {
  const ticks = props.item.UserData?.PlaybackPositionTicks ?? 0;
  const total = props.item.RunTimeTicks ?? 0;
  if (!ticks || !total) return 0;
  return Math.min(100, Math.round((ticks / total) * 100));
});
</script>

<template>
  <article
    class="border-default hover:bg-elevated group flex cursor-pointer items-center gap-3 rounded-lg border px-3 py-2 transition"
    @click="emit('select', item)"
    @contextmenu.prevent
  >
    <div
      class="bg-elevated relative flex-shrink-0 overflow-hidden rounded"
      :class="detailed ? 'h-28 w-20' : 'h-18 w-12'"
    >
      <img
        v-if="imageUrl && !imageError"
        :src="imageUrl"
        :alt="item.Name"
        loading="lazy"
        decoding="async"
        class="h-full w-full object-cover"
        @error="imageError = true"
      />
      <div
        v-else
        class="from-primary/20 to-primary/5 text-primary flex h-full w-full items-center justify-center bg-gradient-to-br text-lg font-bold"
      >
        {{ (item.Name || '').slice(0, 1).toUpperCase() || '?' }}
      </div>
      <div v-if="progress > 0" class="absolute inset-x-0 bottom-0 h-0.5 bg-black/40">
        <div class="bg-primary h-full" :style="{ width: `${progress}%` }" />
      </div>
    </div>

    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2">
        <h3
          class="text-highlighted group-hover:text-primary truncate text-sm font-medium transition-colors"
          :title="item.Name"
        >
          {{ item.Name }}
        </h3>
        <UIcon v-if="item.UserData?.Played" name="i-lucide-check-circle" class="size-3.5 flex-shrink-0 text-green-500" title="已观看" />
        <UIcon v-else-if="item.UserData?.IsFavorite" name="i-lucide-heart" class="size-3.5 flex-shrink-0 text-red-500" title="收藏" />
        <span v-if="item.ProductionYear" class="text-muted flex-shrink-0 text-xs">
          {{ item.ProductionYear }}
        </span>
        <MediaQualityBadges :item="item" compact class="flex-shrink-0" />
      </div>

      <template v-if="detailed">
        <p
          v-if="item.Overview"
          class="text-muted mt-1 line-clamp-2 text-xs leading-relaxed"
          :title="item.Overview"
        >
          {{ item.Overview }}
        </p>
        <div class="text-muted mt-1 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs">
          <span v-if="runtime">{{ runtime }}</span>
          <span v-if="genres">{{ genres }}</span>
          <span v-if="rating" class="flex items-center gap-0.5">
            <UIcon name="i-lucide-star" class="text-amber-400 size-3" />
            {{ rating }}
          </span>
          <span v-if="resolution">{{ resolution }}</span>
        </div>
      </template>
    </div>
  </article>
</template>
