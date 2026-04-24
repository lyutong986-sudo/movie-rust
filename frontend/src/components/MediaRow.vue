<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import MediaCard from './MediaCard.vue';
import type { BaseItemDto } from '../api/emby';

const props = withDefaults(
  defineProps<{
    title: string;
    hint?: string;
    items: BaseItemDto[];
    thumb?: boolean;
    icon?: string;
  }>(),
  {
    thumb: false,
    icon: undefined,
    hint: ''
  }
);

const emit = defineEmits<{
  select: [item: BaseItemDto];
  play: [item: BaseItemDto];
}>();

const scroller = ref<HTMLElement | null>(null);
const canLeft = ref(false);
const canRight = ref(false);

function updateArrows() {
  const el = scroller.value;
  if (!el) return;
  canLeft.value = el.scrollLeft > 4;
  canRight.value = el.scrollLeft + el.clientWidth < el.scrollWidth - 4;
}

function scrollBy(dir: 1 | -1) {
  const el = scroller.value;
  if (!el) return;
  el.scrollBy({ left: dir * Math.max(320, el.clientWidth * 0.8), behavior: 'smooth' });
}

onMounted(() => {
  updateArrows();
  scroller.value?.addEventListener('scroll', updateArrows, { passive: true });
  window.addEventListener('resize', updateArrows);
});

onBeforeUnmount(() => {
  scroller.value?.removeEventListener('scroll', updateArrows);
  window.removeEventListener('resize', updateArrows);
});

const cardWidth = computed(() =>
  props.thumb ? 'w-52 sm:w-60 md:w-64' : 'w-28 sm:w-32 md:w-36 lg:w-40'
);
</script>

<template>
  <section class="space-y-3">
    <div class="flex items-baseline justify-between gap-3">
      <div class="flex items-center gap-2 min-w-0">
        <UIcon v-if="icon" :name="icon" class="text-primary size-4" />
        <h2 class="text-highlighted truncate text-lg font-semibold">{{ title }}</h2>
      </div>
      <div class="flex items-center gap-2">
        <span v-if="hint" class="text-muted hidden text-sm sm:inline">{{ hint }}</span>
        <span v-else class="text-muted text-sm">{{ items.length }} 项</span>
        <div class="hidden gap-1 sm:flex">
          <UButton
            color="neutral"
            variant="soft"
            size="xs"
            icon="i-lucide-chevron-left"
            :disabled="!canLeft"
            aria-label="向左"
            @click="scrollBy(-1)"
          />
          <UButton
            color="neutral"
            variant="soft"
            size="xs"
            icon="i-lucide-chevron-right"
            :disabled="!canRight"
            aria-label="向右"
            @click="scrollBy(1)"
          />
        </div>
      </div>
    </div>

    <div
      ref="scroller"
      class="flex snap-x snap-mandatory gap-4 overflow-x-auto scroll-smooth pb-2
             [-ms-overflow-style:none] [scrollbar-width:thin]
             [&::-webkit-scrollbar]:h-1.5
             [&::-webkit-scrollbar-thumb]:rounded-full
             [&::-webkit-scrollbar-thumb]:bg-default"
    >
      <div
        v-for="item in items"
        :key="item.Id"
        class="shrink-0 snap-start"
        :class="cardWidth"
      >
        <MediaCard
          :item="item"
          :thumb="thumb"
          @play="emit('play', $event)"
          @select="emit('select', $event)"
        />
      </div>
    </div>
  </section>
</template>
