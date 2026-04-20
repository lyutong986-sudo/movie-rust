<script setup lang="ts">
import { reactive, watch } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';

const STORAGE_KEY = 'movie-rust-subtitle-settings';
interface SubtitleClientSettings {
  enabled: boolean;
  fontFamily: string;
  fontSize: number;
  position: number;
  backdrop: boolean;
  stroke: boolean;
}

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
  <section class="settings-shell">
    <SettingsNav />

    <div class="settings-content">
      <div class="settings-page settings-form">
        <label class="check-row">
          <input v-model="settings.enabled" type="checkbox" />
          启用字幕
        </label>
        <label>
          字体
          <select v-model="settings.fontFamily" :disabled="!settings.enabled">
            <option value="Inter, Microsoft YaHei, sans-serif">默认</option>
            <option value="'Microsoft YaHei', sans-serif">微软雅黑</option>
            <option value="'PingFang SC', sans-serif">苹方</option>
            <option value="Arial, sans-serif">Arial</option>
          </select>
        </label>
        <label>
          字号
          <input v-model="settings.fontSize" :disabled="!settings.enabled" type="range" min="1" max="3" step="0.1" />
        </label>
        <label>
          距离底部
          <input v-model="settings.position" :disabled="!settings.enabled" type="range" min="0" max="20" step="1" />
        </label>
        <label class="check-row">
          <input v-model="settings.backdrop" :disabled="!settings.enabled" type="checkbox" />
          背景底板
        </label>
        <label class="check-row">
          <input v-model="settings.stroke" :disabled="!settings.enabled" type="checkbox" />
          文字描边
        </label>

        <div class="caption-preview">
          <div
            class="caption-preview-text"
            :style="{
              fontFamily: settings.fontFamily,
              fontSize: `${settings.fontSize}rem`,
              bottom: `${settings.position}%`,
              textShadow: settings.stroke ? '0 0 4px rgba(0,0,0,0.9), 0 0 10px rgba(0,0,0,0.8)' : 'none',
              background: settings.backdrop ? 'rgba(0,0,0,0.45)' : 'transparent',
              opacity: settings.enabled ? '1' : '0.35'
            }"
          >
            这是字幕预览，风格参考 Jellyfin 的客户端字幕设置页。
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
