<script setup lang="ts">
import { ref, watch, onBeforeUnmount, computed } from 'vue';
import type { BaseItemDto } from '../api/emby';
import { api } from '../store/app';

const props = withDefaults(
  defineProps<{
    nextEpisode: BaseItemDto;
    visible: boolean;
    countdownSeconds: number;
  }>(),
  { countdownSeconds: 30 }
);

const emit = defineEmits<{
  'play-next': [];
  dismiss: [];
  'update:visible': [value: boolean];
}>();

const remaining = ref(props.countdownSeconds);
let intervalId: ReturnType<typeof setInterval> | null = null;

const progress = computed(() =>
  props.countdownSeconds > 0
    ? (props.countdownSeconds - remaining.value) / props.countdownSeconds
    : 1
);

const circumference = 2 * Math.PI * 18;
const strokeOffset = computed(() => circumference * (1 - progress.value));

const posterUrl = computed(() => api.itemImageUrl(props.nextEpisode));

const episodeLabel = computed(() => {
  const ep = props.nextEpisode;
  const season = ep.ParentIndexNumber;
  const episode = ep.IndexNumber;
  const parts: string[] = [];
  if (ep.SeriesName) parts.push(ep.SeriesName);
  if (season != null && episode != null) parts.push(`S${season}E${episode}`);
  if (ep.Name) parts.push(ep.Name);
  return parts.join(' - ');
});

function startCountdown() {
  stopCountdown();
  remaining.value = props.countdownSeconds;
  intervalId = setInterval(() => {
    remaining.value -= 1;
    if (remaining.value <= 0) {
      stopCountdown();
      emit('play-next');
    }
  }, 1000);
}

function stopCountdown() {
  if (intervalId !== null) {
    clearInterval(intervalId);
    intervalId = null;
  }
}

watch(
  () => props.visible,
  (v) => {
    if (v) {
      startCountdown();
    } else {
      stopCountdown();
    }
  },
  { immediate: true }
);

onBeforeUnmount(() => {
  stopCountdown();
});

function onPlayNext() {
  stopCountdown();
  emit('play-next');
}

function onDismiss() {
  stopCountdown();
  emit('update:visible', false);
  emit('dismiss');
}
</script>

<template>
  <transition name="upnext-slide">
    <div v-if="visible" class="upnext-dialog">
      <div class="upnext-header">
        <span class="upnext-title">接下来</span>
        <div class="upnext-countdown">
          <svg class="upnext-ring" viewBox="0 0 40 40">
            <circle
              cx="20" cy="20" r="18"
              fill="none"
              stroke="rgba(255,255,255,0.15)"
              stroke-width="3"
            />
            <circle
              cx="20" cy="20" r="18"
              fill="none"
              stroke="white"
              stroke-width="3"
              stroke-linecap="round"
              :stroke-dasharray="circumference"
              :stroke-dashoffset="strokeOffset"
              class="upnext-ring-progress"
            />
          </svg>
          <span class="upnext-countdown-text">{{ remaining }}</span>
        </div>
      </div>

      <div class="upnext-body">
        <img
          v-if="posterUrl"
          :src="posterUrl"
          :alt="nextEpisode.Name"
          class="upnext-poster"
        />
        <div class="upnext-info">
          <p class="upnext-ep-label">{{ episodeLabel }}</p>
        </div>
      </div>

      <div class="upnext-actions">
        <button type="button" class="upnext-btn upnext-btn-primary" @click="onPlayNext">
          立即播放
        </button>
        <button type="button" class="upnext-btn upnext-btn-secondary" @click="onDismiss">
          取消
        </button>
      </div>
    </div>
  </transition>
</template>

<style scoped>
.upnext-dialog {
  position: absolute;
  bottom: 7rem;
  right: 2rem;
  z-index: 30;
  display: flex;
  flex-direction: column;
  gap: 0.625rem;
  width: 20rem;
  padding: 0.875rem;
  background: rgba(0, 0, 0, 0.82);
  backdrop-filter: blur(12px);
  border-radius: 0.75rem;
  border: 1px solid rgba(255, 255, 255, 0.12);
  color: white;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
}

.upnext-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.upnext-title {
  font-size: 0.75rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: rgba(255, 255, 255, 0.6);
}

.upnext-countdown {
  position: relative;
  width: 2.25rem;
  height: 2.25rem;
  display: flex;
  align-items: center;
  justify-content: center;
}

.upnext-ring {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  transform: rotate(-90deg);
}

.upnext-ring-progress {
  transition: stroke-dashoffset 0.3s linear;
}

.upnext-countdown-text {
  font-size: 0.75rem;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.upnext-body {
  display: flex;
  gap: 0.75rem;
  align-items: center;
}

.upnext-poster {
  width: 5.5rem;
  height: 3.25rem;
  object-fit: cover;
  border-radius: 0.375rem;
  flex-shrink: 0;
  background: rgba(255, 255, 255, 0.05);
}

.upnext-info {
  min-width: 0;
  flex: 1;
}

.upnext-ep-label {
  font-size: 0.8125rem;
  font-weight: 500;
  line-height: 1.35;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.upnext-actions {
  display: flex;
  gap: 0.5rem;
}

.upnext-btn {
  flex: 1;
  padding: 0.4rem 0.75rem;
  border: none;
  border-radius: 0.375rem;
  font-size: 0.8125rem;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s, opacity 0.15s;
}

.upnext-btn-primary {
  background: white;
  color: black;
}
.upnext-btn-primary:hover {
  background: rgba(255, 255, 255, 0.88);
}

.upnext-btn-secondary {
  background: rgba(255, 255, 255, 0.12);
  color: white;
}
.upnext-btn-secondary:hover {
  background: rgba(255, 255, 255, 0.2);
}

.upnext-slide-enter-active {
  transition: transform 0.35s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.3s ease;
}
.upnext-slide-leave-active {
  transition: transform 0.25s ease-in, opacity 0.2s ease;
}
.upnext-slide-enter-from {
  transform: translateX(2rem);
  opacity: 0;
}
.upnext-slide-leave-to {
  transform: translateX(2rem);
  opacity: 0;
}
</style>
