<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { state } from '../store/app';

const visible = ref(false);
const progress = ref(0);
let timer = 0;
let hideTimer = 0;

function tick() {
  if (progress.value < 85) {
    progress.value += Math.max(1, (95 - progress.value) * 0.08);
  }
}

function start() {
  window.clearInterval(timer);
  window.clearTimeout(hideTimer);
  visible.value = true;
  progress.value = 8;
  timer = window.setInterval(tick, 180);
}

function finish() {
  window.clearInterval(timer);
  progress.value = 100;
  hideTimer = window.setTimeout(() => {
    visible.value = false;
    progress.value = 0;
  }, 280);
}

watch(
  () => state.busy,
  (busy) => {
    if (busy) {
      start();
    } else if (visible.value) {
      finish();
    }
  }
);

onBeforeUnmount(() => {
  window.clearInterval(timer);
  window.clearTimeout(hideTimer);
});

const style = computed(() => ({
  width: `${progress.value}%`,
  opacity: visible.value ? 1 : 0
}));
</script>

<template>
  <div
    aria-hidden="true"
    class="pointer-events-none fixed inset-x-0 top-0 z-[100] h-0.5 bg-transparent"
  >
    <div
      class="bg-primary h-full shadow-[0_0_10px_rgba(0,0,0,0.2)] transition-[width,opacity] duration-200 ease-out"
      :style="style"
    />
  </div>
</template>
