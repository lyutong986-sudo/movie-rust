<script setup lang="ts">
import { onMounted } from 'vue';
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

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">并发线程</h3>
        </template>
        <div class="grid gap-4 sm:grid-cols-3">
          <UFormField label="影片扫描入库线程数" hint="扫描媒体库时使用的并行任务数">
            <UInput v-model.number="state.libraryScanThreadCount" type="number" :min="1" :max="32" class="w-full" />
          </UFormField>
          <UFormField label="STRM URL 读取线程数" hint="解析 STRM 链接的并发数">
            <UInput v-model.number="state.strmAnalysisThreadCount" type="number" :min="1" :max="64" class="w-full" />
          </UFormField>
          <UFormField label="TMDB 元数据线程数" hint="抓取第三方元数据的并发数">
            <UInput v-model.number="state.tmdbMetadataThreadCount" type="number" :min="1" :max="32" class="w-full" />
          </UFormField>
        </div>
        <UFormField class="mt-4" label="TMDB API Key" hint="在 themoviedb.org 注册获取 API v3 密钥">
          <UInput v-model.trim="state.tmdbApiKey" placeholder="TMDB API Key (v3)" class="w-full" />
        </UFormField>
        <template #footer>
          <div class="flex justify-end">
            <UButton type="submit" :loading="state.busy" icon="i-lucide-save">保存服务器设置</UButton>
          </div>
        </template>
      </UCard>
    </form>
  </SettingsLayout>
</template>
