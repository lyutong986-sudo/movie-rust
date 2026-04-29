<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { SubtitleDownloadConfiguration } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(true);
const saving = ref(false);
const error = ref('');
const saved = ref('');

const languagesText = ref('');

const form = reactive<SubtitleDownloadConfiguration>({
  DownloadSubtitlesForMovies: false,
  DownloadSubtitlesForEpisodes: false,
  DownloadLanguages: [],
  RequirePerfectMatch: true,
  SkipIfAudioTrackPresent: false,
  SkipIfGraphicalSubsPresent: true,
  OpenSubtitlesUsername: '',
  OpenSubtitlesPassword: '',
  OpenSubtitlesApiKey: ''
});

const languageOptions = [
  { label: '简体中文 (zho/chi)', value: 'zho' },
  { label: '英文 (eng)', value: 'eng' },
  { label: '日语 (jpn)', value: 'jpn' },
  { label: '韩语 (kor)', value: 'kor' },
  { label: '繁体中文 (zht)', value: 'zht' },
  { label: '法语 (fre/fra)', value: 'fre' },
  { label: '德语 (ger/deu)', value: 'ger' },
  { label: '西班牙语 (spa)', value: 'spa' }
];

const isLanguageSelected = computed(() => {
  const selected = new Set(form.DownloadLanguages.map((code) => code.toLowerCase()));
  return (code: string) => selected.has(code.toLowerCase());
});

function toggleLanguage(code: string) {
  const lower = code.toLowerCase();
  const set = new Set(form.DownloadLanguages.map((item) => item.toLowerCase()));
  if (set.has(lower)) {
    set.delete(lower);
  } else {
    set.add(lower);
  }
  form.DownloadLanguages = Array.from(set);
  languagesText.value = form.DownloadLanguages.join(', ');
}

function applyCustomLanguages() {
  form.DownloadLanguages = languagesText.value
    .split(/[,;，；\s]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

onMounted(async () => {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  try {
    Object.assign(form, await api.subtitleDownloadConfiguration());
    languagesText.value = form.DownloadLanguages.join(', ');
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
});

async function save() {
  error.value = '';
  saved.value = '';
  saving.value = true;
  try {
    applyCustomLanguages();
    Object.assign(form, await api.updateSubtitleDownloadConfiguration(form));
    languagesText.value = form.DownloadLanguages.join(', ');
    saved.value = '字幕下载策略已保存';
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能修改字幕下载策略。</p>
    </div>

    <form v-else class="space-y-4" @submit.prevent="save">
      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">为以下类型下载字幕</h3>
        </template>
        <div class="grid gap-3 sm:grid-cols-2">
          <USwitch v-model="form.DownloadSubtitlesForMovies" label="电影" />
          <USwitch v-model="form.DownloadSubtitlesForEpisodes" label="剧集分集" />
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">下载语言</h3>
        </template>
        <div class="grid gap-2 sm:grid-cols-2">
          <label
            v-for="option in languageOptions"
            :key="option.value"
            class="flex items-center gap-2 rounded-md border border-default p-2 text-sm"
          >
            <UCheckbox
              :model-value="isLanguageSelected(option.value)"
              @update:model-value="() => toggleLanguage(option.value)"
            />
            {{ option.label }}
          </label>
        </div>
        <UFormField class="mt-4" label="自定义语言代码" hint="多个代码用逗号分隔，例如 zho, eng, jpn">
          <UInput v-model="languagesText" class="w-full" @change="applyCustomLanguages" />
        </UFormField>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">匹配策略</h3>
        </template>
        <div class="grid gap-3 sm:grid-cols-2">
          <USwitch v-model="form.RequirePerfectMatch" label="要求完美匹配（哈希）" />
          <USwitch v-model="form.SkipIfAudioTrackPresent" label="当存在匹配音轨时跳过" />
          <USwitch v-model="form.SkipIfGraphicalSubsPresent" label="当存在图形字幕时跳过" />
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">OpenSubtitles 账号</h3>
        </template>
        <p class="text-muted mb-3 text-xs">搜索字幕无需登录。如需下载字幕，请填写 OpenSubtitles 账号密码。</p>
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="用户名">
            <UInput v-model.trim="form.OpenSubtitlesUsername" placeholder="OpenSubtitles 用户名" class="w-full" />
          </UFormField>
          <UFormField label="密码">
            <UInput v-model="form.OpenSubtitlesPassword" type="password" class="w-full" />
          </UFormField>
        </div>
        <UFormField class="mt-4" label="API Key（可选）" hint="留空将使用内置 API Key">
          <UInput v-model.trim="form.OpenSubtitlesApiKey" placeholder="留空使用内置 Key" class="w-full" />
        </UFormField>
        <template #footer>
          <div class="flex justify-end">
            <UButton type="submit" :loading="saving" icon="i-lucide-save">保存字幕下载策略</UButton>
          </div>
        </template>
      </UCard>
    </form>
  </SettingsLayout>
</template>
