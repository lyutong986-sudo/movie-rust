<script setup lang="ts">
import { computed } from 'vue';
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

const imageUrl = computed(() => {
  if (props.thumb) {
    return api.backdropUrl(props.item) || api.itemImageUrl(props.item);
  }

  return api.itemImageUrl(props.item) || api.backdropUrl(props.item);
});
const title = computed(() =>
  props.item.Type === 'Episode' && props.item.SeriesName ? props.item.SeriesName : props.item.Name
);
const secondary = computed(() => props.subtitle || itemSubtitle(props.item));
const playable = computed(() => !props.item.IsFolder && Boolean(props.item.MediaSources?.length));
const fallbackLabel = computed(() => {
  if (props.item.IsFolder) {
    return '目录';
  }

  return props.item.Name.slice(0, 1).toUpperCase();
});
</script>

<template>
  <article class="poster-card" @click="emit('select', props.item)">
    <div class="poster-art" :class="{ thumb: props.thumb }">
      <img v-if="imageUrl" :src="imageUrl" :alt="props.item.Name" loading="lazy" />
      <div v-else class="poster-fallback" :class="{ folder: props.item.IsFolder }">
        {{ fallbackLabel }}
      </div>
      <button
        v-if="playable"
        class="play-fab"
        type="button"
        title="播放"
        @click.stop="emit('play', props.item)"
      >
        ▶
      </button>
      <span v-if="props.item.UserData?.Played" class="watched">✓</span>
      <span v-else-if="props.item.UserData?.IsFavorite" class="favorite">♥</span>
    </div>
    <h3 :title="title">{{ title }}</h3>
    <p :title="secondary">{{ secondary }}</p>
  </article>
</template>
