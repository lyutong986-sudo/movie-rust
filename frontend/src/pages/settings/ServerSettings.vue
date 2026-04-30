<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import { isAdmin, loadAdminData, saveServerSettings, state, systemInfo } from '../../store/app';

const uiCultureOptions = [
  { label: '简体中文', value: 'zh-CN' },
  { label: 'English', value: 'en-US' }
];
const metaLangOptions = [
  { label: '中文', value: 'zh' },
  { label: 'English', value: 'en' },
  { label: '日本語', value: 'ja' },
  { label: '한국어', value: 'ko' }
];
const countryOptions = [
  { label: '中国', value: 'CN' },
  { label: 'United States', value: 'US' },
  { label: '日本', value: 'JP' },
  { label: '韩国', value: 'KR' }
];

const performanceTierOptions = [
  { label: '低 — 轻量服务器 / NAS', value: 'low' },
  { label: '中 — 通用服务器', value: 'medium' },
  { label: '高 — 高性能服务器', value: 'high' },
  { label: '超高 — 多核工作站', value: 'ultra' },
  { label: '极限 — 铂金 CPU / 大规模片库', value: 'extreme' }
];

const tierPresets: Record<string, { scan: number; strm: number; tmdb: number; db: number; img: number; bg: number }> = {
  low: { scan: 1, strm: 4, tmdb: 2, db: 10, img: 4, bg: 2 },
  medium: { scan: 2, strm: 8, tmdb: 4, db: 20, img: 8, bg: 4 },
  high: { scan: 8, strm: 32, tmdb: 16, db: 50, img: 24, bg: 12 },
  ultra: { scan: 16, strm: 64, tmdb: 32, db: 100, img: 48, bg: 24 },
  extreme: { scan: 32, strm: 128, tmdb: 64, db: 200, img: 96, bg: 48 }
};

const newTmdbKey = ref('');
const newFanartKey = ref('');
const newSubtitleKey = ref('');

const currentTierPreset = computed(() => tierPresets[state.performanceTier] || tierPresets.medium);

function applyTierPreset() {
  const p = currentTierPreset.value;
  state.libraryScanThreadCount = p.scan;
  state.strmAnalysisThreadCount = p.strm;
  state.tmdbMetadataThreadCount = p.tmdb;
  state.dbMaxConnections = p.db;
  state.imageDownloadThreads = p.img;
  state.backgroundTaskThreads = p.bg;
}

function addTmdbKey() {
  const key = newTmdbKey.value.trim();
  if (key && !state.tmdbApiKeys.includes(key)) {
    state.tmdbApiKeys.push(key);
    newTmdbKey.value = '';
  }
}
function addFanartKey() {
  const key = newFanartKey.value.trim();
  if (key && !state.fanartApiKeys.includes(key)) {
    state.fanartApiKeys.push(key);
    newFanartKey.value = '';
  }
}
function addSubtitleKey() {
  const key = newSubtitleKey.value.trim();
  if (key && !state.subtitleApiKeys.includes(key)) {
    state.subtitleApiKeys.push(key);
    newSubtitleKey.value = '';
  }
}
function removeKey(list: string[], idx: number) {
  list.splice(idx, 1);
}

onMounted(async () => {
  if (isAdmin.value) {
    await loadAdminData();
  }
});
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能修改服务器配置。</p>
    </div>

    <form v-else class="space-y-4" @submit.prevent="saveServerSettings">
      <div class="grid gap-3 sm:grid-cols-3">
        <UCard variant="soft">
          <p class="text-muted text-xs">版本</p>
          <p class="text-highlighted mt-1 text-base font-semibold">{{ systemInfo?.Version || '0.1.0' }}</p>
          <p class="text-muted text-xs">{{ systemInfo?.ProductName || 'Movie Rust' }}</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">系统</p>
          <p class="text-highlighted mt-1 text-base font-semibold">{{ systemInfo?.OperatingSystem || 'Unknown' }}</p>
          <p class="text-muted text-xs">与 Jellyfin/Emby 兼容接口共存</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">引导状态</p>
          <p class="text-highlighted mt-1 text-base font-semibold">
            {{ state.startupWizardCompleted ? '已完成' : '未完成' }}
          </p>
          <p class="text-muted text-xs">管理员账户通过首次启动向导创建</p>
        </UCard>
      </div>

      <UAlert v-if="state.error" color="error" icon="i-lucide-triangle-alert" :description="state.error" />
      <UAlert v-else-if="state.message" color="success" icon="i-lucide-check" :description="state.message" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">基础信息</h3>
        </template>
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="服务器名称">
            <UInput v-model="state.serverName" class="w-full" />
          </UFormField>
          <UFormField label="界面语言">
            <USelect v-model="state.uiCulture" :items="uiCultureOptions" class="w-full" />
          </UFormField>
          <UFormField label="元数据语言">
            <USelect v-model="state.metadataLanguage" :items="metaLangOptions" class="w-full" />
          </UFormField>
          <UFormField label="元数据国家/地区">
            <USelect v-model="state.metadataCountry" :items="countryOptions" class="w-full" />
          </UFormField>
        </div>
      </UCard>

      <!-- 性能档位 -->
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">性能档位</h3>
            <UBadge variant="subtle" color="primary">{{ state.performanceTier }}</UBadge>
          </div>
        </template>
        <UFormField label="性能预设" hint="选择后自动填入推荐参数，也可手动微调各项数值">
          <div class="flex gap-2">
            <USelect v-model="state.performanceTier" :items="performanceTierOptions" class="flex-1" />
            <UButton variant="soft" icon="i-lucide-wand-2" @click="applyTierPreset">应用预设</UButton>
          </div>
        </UFormField>
        <div class="mt-4 grid gap-4 sm:grid-cols-3">
          <UFormField label="影片扫描入库线程" hint="扫描媒体库的并行任务数">
            <UInput v-model.number="state.libraryScanThreadCount" type="number" :min="1" class="w-full" />
          </UFormField>
          <UFormField label="STRM URL 读取线程" hint="解析 STRM 链接的并发数">
            <UInput v-model.number="state.strmAnalysisThreadCount" type="number" :min="1" class="w-full" />
          </UFormField>
          <UFormField label="TMDB 元数据线程" hint="抓取第三方元数据的并发数">
            <UInput v-model.number="state.tmdbMetadataThreadCount" type="number" :min="1" class="w-full" />
          </UFormField>
          <UFormField label="数据库连接池" hint="PostgreSQL 最大连接数">
            <UInput v-model.number="state.dbMaxConnections" type="number" :min="5" class="w-full" />
          </UFormField>
          <UFormField label="图片下载线程" hint="并行下载封面/背景图">
            <UInput v-model.number="state.imageDownloadThreads" type="number" :min="1" class="w-full" />
          </UFormField>
          <UFormField label="后台任务线程" hint="计划任务、NFO 生成等后台工作">
            <UInput v-model.number="state.backgroundTaskThreads" type="number" :min="1" class="w-full" />
          </UFormField>
        </div>
        <p class="text-muted mt-3 text-xs">
          当前预设参考值：扫描 {{ currentTierPreset.scan }} / STRM {{ currentTierPreset.strm }} / TMDB {{ currentTierPreset.tmdb }} / DB {{ currentTierPreset.db }} / 图片 {{ currentTierPreset.img }} / 后台 {{ currentTierPreset.bg }}
        </p>
      </UCard>

      <!-- API Keys 管理 -->
      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">API 密钥管理</h3>
          <p class="text-muted text-xs mt-1">支持多 Key 轮询，突破单 Key 速率限制（TMDB: 40次/分钟/Key）</p>
        </template>

        <!-- TMDB Keys -->
        <div class="space-y-3">
          <UFormField label="TMDB API Keys" hint="主 Key + 额外 Key 列表轮询使用">
            <UInput v-model.trim="state.tmdbApiKey" placeholder="主 TMDB API Key (v3)" class="w-full" />
          </UFormField>
          <div v-if="state.tmdbApiKeys.length" class="flex flex-wrap gap-2">
            <UBadge v-for="(key, idx) in state.tmdbApiKeys" :key="idx" variant="subtle" color="info" class="gap-1">
              <span class="font-mono text-xs">{{ key.slice(0, 8) }}...{{ key.slice(-4) }}</span>
              <UButton variant="link" size="xs" icon="i-lucide-x" color="error" class="ml-1 !p-0" @click="removeKey(state.tmdbApiKeys, idx)" />
            </UBadge>
          </div>
          <div class="flex gap-2">
            <UInput v-model.trim="newTmdbKey" placeholder="添加额外 TMDB Key" class="flex-1" @keyup.enter="addTmdbKey" />
            <UButton variant="soft" icon="i-lucide-plus" @click="addTmdbKey">添加</UButton>
          </div>
        </div>

        <USeparator class="my-4" />

        <!-- Fanart Keys -->
        <div class="space-y-3">
          <UFormField label="Fanart.tv API Keys" hint="用于获取高质量艺术图 (Logo/Banner/Disc)">
            <div v-if="state.fanartApiKeys.length" class="flex flex-wrap gap-2 mb-2">
              <UBadge v-for="(key, idx) in state.fanartApiKeys" :key="idx" variant="subtle" color="success" class="gap-1">
                <span class="font-mono text-xs">{{ key.slice(0, 8) }}...{{ key.slice(-4) }}</span>
                <UButton variant="link" size="xs" icon="i-lucide-x" color="error" class="ml-1 !p-0" @click="removeKey(state.fanartApiKeys, idx)" />
              </UBadge>
            </div>
            <div class="flex gap-2">
              <UInput v-model.trim="newFanartKey" placeholder="Fanart.tv Personal API Key" class="flex-1" @keyup.enter="addFanartKey" />
              <UButton variant="soft" icon="i-lucide-plus" @click="addFanartKey">添加</UButton>
            </div>
          </UFormField>
        </div>

        <USeparator class="my-4" />

        <!-- Subtitle Keys -->
        <div class="space-y-3">
          <UFormField label="字幕下载 API Keys" hint="OpenSubtitles/SubScene 等字幕服务 Key">
            <div v-if="state.subtitleApiKeys.length" class="flex flex-wrap gap-2 mb-2">
              <UBadge v-for="(key, idx) in state.subtitleApiKeys" :key="idx" variant="subtle" color="warning" class="gap-1">
                <span class="font-mono text-xs">{{ key.slice(0, 8) }}...{{ key.slice(-4) }}</span>
                <UButton variant="link" size="xs" icon="i-lucide-x" color="error" class="ml-1 !p-0" @click="removeKey(state.subtitleApiKeys, idx)" />
              </UBadge>
            </div>
            <div class="flex gap-2">
              <UInput v-model.trim="newSubtitleKey" placeholder="字幕服务 API Key" class="flex-1" @keyup.enter="addSubtitleKey" />
              <UButton variant="soft" icon="i-lucide-plus" @click="addSubtitleKey">添加</UButton>
            </div>
          </UFormField>
        </div>

        <template #footer>
          <div class="flex justify-end">
            <UButton type="submit" :loading="state.busy" icon="i-lucide-save">保存服务器设置</UButton>
          </div>
        </template>
      </UCard>
    </form>
  </SettingsLayout>
</template>
