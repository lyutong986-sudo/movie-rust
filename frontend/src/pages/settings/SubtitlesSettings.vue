<script setup lang="ts">
import { reactive, watch } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';

const STORAGE_KEY = 'movie-rust-subtitle-settings';
interface SubtitleClientSettings {
  enabled: boolean;
  fontFamily: string;
  fontSize: number;
  position: number;
  backdrop: boolean;
  stroke: boolean;
}

const fontOptions = [
  { label: '默认', value: 'Inter, Microsoft YaHei, sans-serif' },
  { label: '微软雅黑', value: "'Microsoft YaHei', sans-serif" },
  { label: '苹方', value: "'PingFang SC', sans-serif" },
  { label: 'Arial', value: 'Arial, sans-serif' }
];

const settings = reactive(
  readSubtitleSettings() || {
    enabled: true,
    fontFamily: 'Inter, Microsoft YaHei, sans-serif',
    fontSize: 1.4,
    position: 8,
    backdrop: true,
    stroke: true
  }
);

watch(
  settings,
  () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  },
  { deep: true }
);

function readSubtitleSettings() {
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) {
    return null;
  }
  try {
    return JSON.parse(raw) as SubtitleClientSettings;
  } catch {
    return null;
  }
}
</script>

<template>
  <SettingsLayout>
    <div class="space-y-4">
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">字幕样式</h3>
            <USwitch v-model="settings.enabled" label="启用字幕" />
          </div>
        </template>

        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="字体">
            <USelect
              v-model="settings.fontFamily"
              :items="fontOptions"
              :disabled="!settings.enabled"
              class="w-full"
            />
          </UFormField>
          <div />
          <UFormField :label="`字号：${settings.fontSize}rem`">
            <input
              v-model.number="settings.fontSize"
              :disabled="!settings.enabled"
              type="range"
              min="1"
              max="3"
              step="0.1"
              class="w-full accent-[var(--ui-primary)]"
            />
          </UFormField>
          <UFormField :label="`距离底部：${settings.position}%`">
            <input
              v-model.number="settings.position"
              :disabled="!settings.enabled"
              type="range"
              min="0"
              max="20"
              step="1"
              class="w-full accent-[var(--ui-primary)]"
            />
          </UFormField>
        </div>

        <div class="mt-4 flex gap-6">
          <USwitch v-model="settings.backdrop" :disabled="!settings.enabled" label="背景底板" />
          <USwitch v-model="settings.stroke" :disabled="!settings.enabled" label="文字描边" />
        </div>
      </UCard>

      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">真实场景预览</h3>
            <span class="text-muted text-xs">模拟黑色底片 + 亮色底片 + 单行 / 双行</span>
          </div>
        </template>
        <div class="grid gap-3 md:grid-cols-2">
          <div
            class="relative aspect-video overflow-hidden rounded-lg"
            style="
              background-image: linear-gradient(135deg, #0f172a, #1e293b 60%, #0f172a),
                radial-gradient(circle at 20% 20%, #334155 0%, transparent 60%);
            "
          >
            <div
              class="absolute left-1/2 -translate-x-1/2 rounded px-4 py-1 text-center leading-snug text-white transition"
              :style="{
                fontFamily: settings.fontFamily,
                fontSize: `${settings.fontSize}rem`,
                bottom: `${settings.position}%`,
                textShadow: settings.stroke
                  ? '0 0 4px rgba(0,0,0,0.9), 0 0 10px rgba(0,0,0,0.8)'
                  : 'none',
                background: settings.backdrop ? 'rgba(0,0,0,0.45)' : 'transparent',
                opacity: settings.enabled ? '1' : '0.35'
              }"
            >
              这是字幕预览，风格参考 Jellyfin。<br />
              可以在这里检查两行文本的换行和间距。
            </div>
          </div>
          <div
            class="relative aspect-video overflow-hidden rounded-lg"
            style="
              background-image: linear-gradient(135deg, #f8fafc, #e2e8f0 60%, #cbd5e1),
                radial-gradient(circle at 70% 60%, #a5b4fc 0%, transparent 60%);
            "
          >
            <div
              class="absolute left-1/2 -translate-x-1/2 rounded px-4 py-1 text-center leading-snug text-white transition"
              :style="{
                fontFamily: settings.fontFamily,
                fontSize: `${settings.fontSize}rem`,
                bottom: `${settings.position}%`,
                textShadow: settings.stroke
                  ? '0 0 4px rgba(0,0,0,0.9), 0 0 10px rgba(0,0,0,0.8)'
                  : 'none',
                background: settings.backdrop ? 'rgba(0,0,0,0.45)' : 'transparent',
                opacity: settings.enabled ? '1' : '0.35'
              }"
            >
              明亮底片下的文字描边与底板效果。
            </div>
          </div>
        </div>
      </UCard>
    </div>
  </SettingsLayout>
</template>
