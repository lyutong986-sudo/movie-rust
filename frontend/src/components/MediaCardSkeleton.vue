<script setup lang="ts">
withDefaults(defineProps<{ thumb?: boolean }>(), { thumb: false });
</script>

<template>
  <div class="flex flex-col gap-2">
    <div
      class="w-full animate-pulse overflow-hidden rounded-xl bg-elevated/50 ring-1 ring-default"
      :class="thumb ? 'aspect-video' : 'aspect-[2/3]'"
    />
    <div class="space-y-1.5">
      <div class="h-3.5 w-4/5 animate-pulse rounded bg-elevated/60" />
      <div class="h-3 w-2/5 animate-pulse rounded bg-elevated/40" />
    </div>
  </div>
</template>
