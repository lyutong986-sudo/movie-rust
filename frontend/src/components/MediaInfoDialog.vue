<script setup lang="ts">
import { computed } from 'vue';
import type { BaseItemDto, MediaStreamDto } from '../api/emby';

const props = defineProps<{
  item: BaseItemDto;
  open: boolean;
}>();

const emit = defineEmits<{
  'update:open': [value: boolean];
}>();

const dialogOpen = computed({
  get: () => props.open,
  set: (v) => emit('update:open', v)
});

const source = computed(() => props.item.MediaSources?.[0]);
const streams = computed(() => source.value?.MediaStreams || props.item.MediaStreams || []);
const videoStreams = computed(() => streams.value.filter((s) => s.Type === 'Video'));
const audioStreams = computed(() => streams.value.filter((s) => s.Type === 'Audio'));
const subtitleStreams = computed(() => streams.value.filter((s) => s.Type === 'Subtitle'));

function formatSize(bytes?: number): string {
  if (!bytes) return '-';
  if (bytes >= 1073741824) return (bytes / 1073741824).toFixed(2) + ' GB';
  if (bytes >= 1048576) return (bytes / 1048576).toFixed(1) + ' MB';
  return (bytes / 1024).toFixed(0) + ' KB';
}

function formatBitrate(bps?: number): string {
  if (!bps) return '-';
  if (bps >= 1000000) return (bps / 1000000).toFixed(1) + ' Mbps';
  return (bps / 1000).toFixed(0) + ' kbps';
}

function formatChannels(channels?: number): string {
  if (!channels) return '-';
  if (channels === 8) return '7.1';
  if (channels === 6) return '5.1';
  if (channels === 2) return '立体声';
  if (channels === 1) return '单声道';
  return `${channels} 声道`;
}

function codecName(codec?: string): string {
  if (!codec) return '-';
  const map: Record<string, string> = {
    h264: 'H.264', hevc: 'HEVC', h265: 'HEVC', av1: 'AV1', vp9: 'VP9', mpeg4: 'MPEG-4',
    aac: 'AAC', ac3: 'AC3', eac3: 'EAC3', dts: 'DTS', flac: 'FLAC', truehd: 'TrueHD',
    opus: 'Opus', mp3: 'MP3', vorbis: 'Vorbis', pcm_s16le: 'PCM',
    srt: 'SRT', subrip: 'SRT', ass: 'ASS', ssa: 'SSA', webvtt: 'WebVTT', vtt: 'WebVTT',
    pgssub: 'PGS', hdmv_pgs_subtitle: 'PGS', dvdsub: 'DVD SUB', dvd_subtitle: 'DVD SUB'
  };
  return map[codec.toLowerCase()] || codec.toUpperCase();
}

function streamRow(stream: MediaStreamDto): Array<{ label: string; value: string }> {
  if (stream.Type === 'Video') {
    return [
      { label: '编解码器', value: codecName(stream.Codec) },
      { label: '分辨率', value: stream.Width && stream.Height ? `${stream.Width} × ${stream.Height}` : '-' },
      { label: '宽高比', value: stream.AspectRatio || '-' },
      { label: '码率', value: formatBitrate(stream.BitRate) },
      { label: '帧率', value: stream.RealFrameRate ? `${stream.RealFrameRate} fps` : '-' },
      { label: '位深', value: stream.BitDepth ? `${stream.BitDepth} bit` : '-' },
      { label: '色彩空间', value: [stream.ColorSpace, stream.ColorPrimaries].filter(Boolean).join(' / ') || '-' },
      { label: 'HDR', value: stream.VideoRange || stream.VideoRangeType || '-' },
      { label: '档次 / 级别', value: [stream.Profile, stream.Level != null ? `Level ${stream.Level}` : ''].filter(Boolean).join(' / ') || '-' }
    ];
  }
  if (stream.Type === 'Audio') {
    return [
      { label: '编解码器', value: codecName(stream.Codec) },
      { label: '声道', value: stream.ChannelLayout || formatChannels(stream.Channels) },
      { label: '语言', value: stream.Language || '-' },
      { label: '码率', value: formatBitrate(stream.BitRate) },
      { label: '采样率', value: stream.SampleRate ? `${stream.SampleRate} Hz` : '-' },
      { label: '标题', value: stream.Title || stream.DisplayTitle || '-' },
      { label: '标记', value: [stream.IsDefault && '默认', stream.IsForced && '强制'].filter(Boolean).join(' / ') || '-' }
    ];
  }
  return [
    { label: '编解码器', value: codecName(stream.Codec) },
    { label: '语言', value: stream.Language || '-' },
    { label: '标题', value: stream.Title || stream.DisplayTitle || '-' },
    { label: '标记', value: [stream.IsDefault && '默认', stream.IsForced && '强制', stream.IsExternal && '外挂'].filter(Boolean).join(' / ') || '-' }
  ];
}
</script>

<template>
  <UModal v-model:open="dialogOpen" :ui="{ content: 'max-w-3xl' }">
    <template #header>
      <h3 class="text-highlighted text-base font-semibold">媒体信息</h3>
    </template>
    <template #body>
      <div class="max-h-[70vh] space-y-6 overflow-y-auto">
        <!-- 文件信息 -->
        <section>
          <h4 class="text-highlighted mb-3 text-sm font-semibold">文件信息</h4>
          <dl class="bg-elevated/30 divide-default divide-y rounded-lg">
            <div class="flex items-baseline gap-4 px-4 py-2.5">
              <dt class="text-muted w-24 shrink-0 text-xs">容器格式</dt>
              <dd class="text-default text-sm">{{ source?.Container || item.Container || '-' }}</dd>
            </div>
            <div class="flex items-baseline gap-4 px-4 py-2.5">
              <dt class="text-muted w-24 shrink-0 text-xs">文件路径</dt>
              <dd class="text-default break-all font-mono text-xs">{{ source?.Path || item.Path || '-' }}</dd>
            </div>
            <div class="flex items-baseline gap-4 px-4 py-2.5">
              <dt class="text-muted w-24 shrink-0 text-xs">文件大小</dt>
              <dd class="text-default text-sm">{{ formatSize(source?.Size) }}</dd>
            </div>
            <div class="flex items-baseline gap-4 px-4 py-2.5">
              <dt class="text-muted w-24 shrink-0 text-xs">总码率</dt>
              <dd class="text-default text-sm">{{ formatBitrate(source?.Bitrate) }}</dd>
            </div>
          </dl>
        </section>

        <!-- 视频流 -->
        <section v-if="videoStreams.length">
          <h4 class="text-highlighted mb-3 text-sm font-semibold">
            视频流
            <span class="text-muted ml-1 font-normal">({{ videoStreams.length }})</span>
          </h4>
          <div class="space-y-3">
            <div
              v-for="(vs, idx) in videoStreams"
              :key="`v-${vs.Index}`"
              class="bg-elevated/30 overflow-hidden rounded-lg"
            >
              <div
                v-if="videoStreams.length > 1"
                class="bg-elevated/50 border-default border-b px-4 py-1.5 text-xs font-medium"
              >
                视频 #{{ idx + 1 }}
              </div>
              <dl class="divide-default divide-y">
                <div
                  v-for="field in streamRow(vs)"
                  :key="field.label"
                  class="flex items-baseline gap-4 px-4 py-2"
                >
                  <dt class="text-muted w-24 shrink-0 text-xs">{{ field.label }}</dt>
                  <dd class="text-default text-sm">{{ field.value }}</dd>
                </div>
              </dl>
            </div>
          </div>
        </section>

        <!-- 音频流 -->
        <section v-if="audioStreams.length">
          <h4 class="text-highlighted mb-3 text-sm font-semibold">
            音频流
            <span class="text-muted ml-1 font-normal">({{ audioStreams.length }})</span>
          </h4>
          <div class="space-y-3">
            <div
              v-for="(as_, idx) in audioStreams"
              :key="`a-${as_.Index}`"
              class="bg-elevated/30 overflow-hidden rounded-lg"
            >
              <div
                v-if="audioStreams.length > 1"
                class="bg-elevated/50 border-default border-b px-4 py-1.5 text-xs font-medium"
              >
                音频 #{{ idx + 1 }}
              </div>
              <dl class="divide-default divide-y">
                <div
                  v-for="field in streamRow(as_)"
                  :key="field.label"
                  class="flex items-baseline gap-4 px-4 py-2"
                >
                  <dt class="text-muted w-24 shrink-0 text-xs">{{ field.label }}</dt>
                  <dd class="text-default text-sm">{{ field.value }}</dd>
                </div>
              </dl>
            </div>
          </div>
        </section>

        <!-- 字幕流 -->
        <section v-if="subtitleStreams.length">
          <h4 class="text-highlighted mb-3 text-sm font-semibold">
            字幕流
            <span class="text-muted ml-1 font-normal">({{ subtitleStreams.length }})</span>
          </h4>
          <div class="space-y-3">
            <div
              v-for="(ss, idx) in subtitleStreams"
              :key="`s-${ss.Index}`"
              class="bg-elevated/30 overflow-hidden rounded-lg"
            >
              <div
                v-if="subtitleStreams.length > 1"
                class="bg-elevated/50 border-default border-b px-4 py-1.5 text-xs font-medium"
              >
                字幕 #{{ idx + 1 }}
              </div>
              <dl class="divide-default divide-y">
                <div
                  v-for="field in streamRow(ss)"
                  :key="field.label"
                  class="flex items-baseline gap-4 px-4 py-2"
                >
                  <dt class="text-muted w-24 shrink-0 text-xs">{{ field.label }}</dt>
                  <dd class="text-default text-sm">{{ field.value }}</dd>
                </div>
              </dl>
            </div>
          </div>
        </section>

        <!-- 无流信息 -->
        <div v-if="!streams.length" class="text-muted py-8 text-center text-sm">
          <UIcon name="i-lucide-file-video" class="mx-auto mb-2 size-8 opacity-50" />
          <p>无可用的媒体流信息</p>
        </div>
      </div>
    </template>
  </UModal>
</template>
