<script setup lang="ts">
import { computed } from 'vue';
import type { BaseItemDto } from '../api/emby';

const props = defineProps<{ item: BaseItemDto; compact?: boolean }>();

// 角标根据 MediaStreams 推断，尽可能贴近 Jellyfin/Plex 的视觉约定：
//   4K = width ≥ 3800 或 height ≥ 2100
//   HDR / DV = VideoRange 或 codec hint
//   ATMOS = 音轨标题或 codec 命中 atmos
const badges = computed(() => {
  const item = props.item;
  const videoStream = item.MediaStreams?.find((s) => s.Type === 'Video')
    || item.MediaSources?.[0]?.MediaStreams?.find((s) => s.Type === 'Video');
  const audioStreams = (item.MediaStreams || item.MediaSources?.[0]?.MediaStreams || []).filter(
    (s) => s.Type === 'Audio'
  );

  const list: Array<{ label: string; tone: string }> = [];

  const width = videoStream?.Width || item.Width || 0;
  const height = videoStream?.Height || item.Height || 0;
  if (width >= 3800 || height >= 2100) {
    list.push({ label: '4K', tone: 'bg-sky-500/90 text-white' });
  } else if (width >= 1800 || height >= 1000 || item.IsHD) {
    list.push({ label: 'HD', tone: 'bg-neutral-700/80 text-white' });
  }

  const vr = (videoStream as unknown as { VideoRange?: string; VideoRangeType?: string } | undefined);
  const vrStr = `${vr?.VideoRange || ''} ${vr?.VideoRangeType || ''} ${item.VideoRange || ''}`.toUpperCase();
  if (/DOVI|DOLBY\s*VISION|DV/.test(vrStr)) {
    list.push({ label: 'DV', tone: 'bg-indigo-500/90 text-white' });
  } else if (/HDR/.test(vrStr)) {
    list.push({ label: 'HDR', tone: 'bg-amber-500/90 text-white' });
  }

  const isAtmos = audioStreams.some((s) => {
    const blob = `${s.DisplayTitle || ''} ${s.Codec || ''}`.toUpperCase();
    return blob.includes('ATMOS');
  });
  if (isAtmos) {
    list.push({ label: 'ATMOS', tone: 'bg-emerald-500/90 text-white' });
  } else {
    const chMax = audioStreams.reduce((m, s) => Math.max(m, s.Channels || 0), 0);
    if (chMax >= 6) {
      list.push({ label: `${chMax}.1`, tone: 'bg-neutral-700/80 text-white' });
    }
  }

  return list;
});
</script>

<template>
  <div v-if="badges.length" class="flex flex-wrap items-center gap-1">
    <span
      v-for="b in badges"
      :key="b.label"
      class="rounded-sm px-1.5 font-mono font-semibold tracking-wider shadow ring-1 ring-black/10 backdrop-blur-sm"
      :class="[b.tone, compact ? 'text-[9px] leading-4' : 'text-[10px] leading-5']"
    >
      {{ b.label }}
    </span>
  </div>
</template>
