<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { BaseItemDto } from '../api/emby';
import { api } from '../store/app';
import MediaQualityBadges from './MediaQualityBadges.vue';

const props = withDefaults(
  defineProps<{
    items: BaseItemDto[];
    intervalMs?: number;
    max?: number;
  }>(),
  {
    intervalMs: 7000,
    max: 5
  }
);

const emit = defineEmits<{
  play: [item: BaseItemDto];
  select: [item: BaseItemDto];
}>();

const slides = computed(() => props.items.slice(0, props.max));
const index = ref(0);
let timer = 0;
let paused = ref(false);

const current = computed(() => slides.value[index.value] || null);
const heroImage = computed(() => {
  if (!current.value) return '';
  return api.backdropUrl(current.value) || api.itemImageUrl(current.value);
});
const heroLogo = computed(() => (current.value ? api.logoUrl(current.value) : ''));

function goto(i: number) {
  if (!slides.value.length) return;
  index.value = (i + slides.value.length) % slides.value.length;
}

function next() {
  goto(index.value + 1);
}

function start() {
  stop();
  if (slides.value.length <= 1) return;
  timer = window.setInterval(() => {
    if (!paused.value) next();
  }, props.intervalMs);
}
function stop() {
  if (timer) window.clearInterval(timer);
  timer = 0;
}

watch(
  () => slides.value.length,
  () => {
    if (index.value >= slides.value.length) index.value = 0;
    start();
  }
);

onMounted(start);
onBeforeUnmount(stop);

function tagForItem(item: BaseItemDto) {
  if (item.UserData?.PlaybackPositionTicks && item.UserData.PlaybackPositionTicks > 0)
    return '继续观看';
  if (item.Type === 'Episode') return '最新剧集';
  if (item.Type === 'Movie') return '精选电影';
  return '最近添加';
}
</script>

<template>
  <section
    v-if="current"
    class="ring-default relative overflow-hidden rounded-2xl ring-1"
    @mouseenter="paused = true"
    @mouseleave="paused = false"
  >
    <div class="relative h-[320px] sm:h-[400px] lg:h-[460px]">
      <transition
        name="hero-fade"
        mode="out-in"
      >
        <img
          v-if="heroImage"
          :key="current.Id"
          :src="heroImage"
          :alt="current.Name"
          class="absolute inset-0 h-full w-full object-cover"
        />
        <div v-else class="from-primary/30 to-primary/5 absolute inset-0 bg-gradient-to-br" />
      </transition>

      <div
        class="absolute inset-0 bg-gradient-to-t from-black via-black/70 to-transparent"
      />
      <div
        class="absolute inset-y-0 left-0 w-full max-w-[60%] bg-gradient-to-r from-black/60 to-transparent"
      />

      <div class="absolute inset-x-0 bottom-0 flex max-w-3xl flex-col gap-3 p-6 sm:p-10">
        <UBadge color="primary" variant="subtle" class="w-fit">
          {{ tagForItem(current) }}
        </UBadge>
        <img
          v-if="heroLogo"
          :src="heroLogo"
          :alt="current.Name"
          class="max-h-20 w-auto drop-shadow-[0_2px_8px_rgba(0,0,0,0.8)]"
        />
        <h1 v-else class="text-3xl font-bold text-white sm:text-4xl lg:text-5xl">
          {{ current.Name }}
        </h1>

        <div class="flex flex-wrap items-center gap-2">
          <MediaQualityBadges :item="current" />
          <span v-if="current.ProductionYear" class="text-sm text-white/80">
            {{ current.ProductionYear }}
          </span>
          <span v-if="current.OfficialRating" class="rounded border border-white/40 px-1.5 text-xs text-white/80">
            {{ current.OfficialRating }}
          </span>
          <span v-if="current.CommunityRating" class="flex items-center gap-1 text-xs text-amber-300">
            <UIcon name="i-lucide-star" class="size-3" />
            {{ current.CommunityRating.toFixed(1) }}
          </span>
        </div>

        <p v-if="current.Overview" class="line-clamp-3 max-w-2xl text-white/80">
          {{ current.Overview }}
        </p>

        <div class="mt-2 flex flex-wrap gap-2">
          <UButton
            v-if="!current.IsFolder && current.MediaSources?.length"
            icon="i-lucide-play"
            size="lg"
            @click="emit('play', current)"
          >
            立即播放
          </UButton>
          <UButton
            color="neutral"
            variant="subtle"
            size="lg"
            icon="i-lucide-info"
            @click="emit('select', current)"
          >
            查看详情
          </UButton>
        </div>
      </div>

      <!-- 分页指示 -->
      <div
        v-if="slides.length > 1"
        class="absolute bottom-4 right-6 flex gap-1.5"
      >
        <button
          v-for="(_, i) in slides"
          :key="i"
          type="button"
          class="h-1.5 rounded-full transition-all"
          :class="[
            i === index ? 'w-8 bg-white' : 'w-2 bg-white/40 hover:bg-white/70'
          ]"
          :aria-label="`切换到第 ${i + 1} 张`"
          @click="goto(i)"
        />
      </div>
    </div>
  </section>
</template>

<style scoped>
.hero-fade-enter-active,
.hero-fade-leave-active {
  transition: opacity 0.7s ease;
}
.hero-fade-enter-from,
.hero-fade-leave-to {
  opacity: 0;
}
</style>
