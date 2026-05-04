<script setup lang="ts">
/**
 * PB52：翻译兜底（Youdao 大模型翻译）配置页。
 *
 * 后台 `system_settings` 表存的是 snake_case JSON；这里直接镜像后端字段名，
 * 不做大小写转换。字段较多，按「连接」「字段范围」「触发位」三组拆分到 3 个
 * UCard，避免单卡过长。
 *
 * 注意：`app_key` / `app_secret` 后端读取时会被脱敏成 `****` 占位（16 / 32 个
 * 星号）。前端 PUT 时如果字段保持脱敏占位，后端会识别成「不变更」继续沿用
 * 已存储值；用户改密钥时直接清空再粘贴新值即可。
 */
import { onMounted, reactive, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { TranslationSettings, TranslationTestResult } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(true);
const saving = ref(false);
const testing = ref(false);
const error = ref('');
const saved = ref('');

const form = reactive<TranslationSettings>({
  enabled: false,
  provider: 'youdao',
  app_key: '',
  app_secret: '',
  target_lang: 'zh-CHS',
  from_lang: 'auto',
  translate_name: true,
  translate_overview: true,
  translate_episode: true,
  translate_season_name: false,
  translate_person_overview: false,
  trigger_manual_refresh: true,
  trigger_scheduled_task: true,
  trigger_remote_sync: true
});

const testInput = reactive({
  text: 'Wonderblock Go gets everything and everyone moving in Wonderland! It\'s noisy, exciting fun.'
});
const testResult = ref<TranslationTestResult | null>(null);
const testError = ref('');

const TARGET_LANG_OPTIONS = [
  { value: 'zh-CHS', label: '简体中文 (zh-CHS)' },
  { value: 'zh-CHT', label: '繁体中文 (zh-CHT)' },
  { value: 'en', label: 'English (en)' },
  { value: 'ja', label: '日本語 (ja)' },
  { value: 'ko', label: '한국어 (ko)' }
];

const FROM_LANG_OPTIONS = [
  { value: 'auto', label: '自动识别 (auto)' },
  { value: 'en', label: 'English (en)' },
  { value: 'ja', label: '日本語 (ja)' },
  { value: 'ko', label: '한국어 (ko)' },
  { value: 'zh-CHS', label: '简体中文 (zh-CHS)' }
];

const PROVIDER_OPTIONS = [
  { value: 'youdao', label: '有道大模型翻译 (Youdao)' }
];

async function load() {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    Object.assign(form, await api.getTranslationSettings());
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
}

async function save() {
  saving.value = true;
  error.value = '';
  saved.value = '';
  try {
    const next = await api.updateTranslationSettings({ ...form });
    Object.assign(form, next);
    saved.value = '翻译兜底配置已保存';
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    saving.value = false;
  }
}

async function runTest() {
  testing.value = true;
  testError.value = '';
  testResult.value = null;
  try {
    testResult.value = await api.testTranslation({
      text: testInput.text,
      from: form.from_lang,
      to: form.target_lang
    });
  } catch (err) {
    testError.value = err instanceof Error ? err.message : String(err);
  } finally {
    testing.value = false;
  }
}

onMounted(load);
</script>

<template>
  <SettingsLayout>
    <div
      v-if="!isAdmin"
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
    >
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">仅管理员可配置翻译兜底服务。</p>
    </div>

    <form v-else class="space-y-4" @submit.prevent="save">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p class="text-muted text-xs tracking-wide">PB52 · 元数据语言兜底</p>
          <h2 class="text-highlighted text-xl font-semibold">翻译兜底（Youdao）</h2>
          <p class="text-muted mt-1 max-w-3xl text-sm leading-relaxed">
            TMDB / 远端 Emby 偶尔只返回英文（典型如 BBC CBeebies 出品的剧集 Episode overview），
            开启后 Movie Rust 会在<strong>元数据写库前</strong>检测目标语言；
            非目标语言的 <code class="text-xs">name</code> / <code class="text-xs">overview</code>
            将通过 <a class="text-primary hover:underline" href="https://ai.youdao.com/doc.s#guide" target="_blank" rel="noopener">有道大模型翻译</a>
            兜底。结果写入 <code class="text-xs">translation_cache</code>，同一句英文不会重复计费。
          </p>
        </div>
        <div class="flex gap-2">
          <UButton color="neutral" variant="subtle" icon="i-lucide-refresh-cw" :loading="loading" @click="load">刷新</UButton>
          <UButton type="submit" color="primary" icon="i-lucide-save" :loading="saving" :disabled="loading">保存</UButton>
        </div>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">连接与认证</h3>
            <USwitch v-model="form.enabled" label="启用翻译兜底" />
          </div>
        </template>
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="服务商">
            <USelect v-model="form.provider" :items="PROVIDER_OPTIONS" value-attribute="value" class="w-full" />
          </UFormField>
          <UFormField label="目标语言">
            <USelect v-model="form.target_lang" :items="TARGET_LANG_OPTIONS" value-attribute="value" class="w-full" />
          </UFormField>
          <UFormField label="源语言（无法识别时使用）">
            <USelect v-model="form.from_lang" :items="FROM_LANG_OPTIONS" value-attribute="value" class="w-full" />
          </UFormField>
          <UFormField label="应用ID" hint="有道智云控制台『应用 ID（appKey）』">
            <UInput
              v-model="form.app_key"
              placeholder="****************"
              class="w-full"
              autocomplete="off"
              spellcheck="false"
            />
          </UFormField>
          <UFormField label="应用秘钥" hint="保留占位 `****` 不变 = 沿用已存储密钥；改值请清空后重新粘贴。">
            <UInput
              v-model="form.app_secret"
              type="password"
              placeholder="********************************"
              class="w-full"
              autocomplete="off"
              spellcheck="false"
            />
          </UFormField>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">字段范围</h3>
        </template>
        <p class="text-muted mb-3 text-xs">
          字段开关控制「在写库前需要做语言检测的字段类型」。粗检测命中目标语言时不会调用 API，所以勾选多但实际计费由远端返回的语言决定。
        </p>
        <div class="grid gap-3 sm:grid-cols-2">
          <USwitch v-model="form.translate_name" label="Movie / Series 名称" />
          <USwitch v-model="form.translate_overview" label="Movie / Series / Season 简介" />
          <USwitch v-model="form.translate_episode" label="Episode 名称 + 简介" />
          <USwitch v-model="form.translate_season_name" label="Season 名称（一般为『第 X 季』，默认关）" />
          <USwitch v-model="form.translate_person_overview" label="People 传记 / 角色描述" />
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">触发位</h3>
        </template>
        <p class="text-muted mb-3 text-xs">
          翻译兜底总是运行在「写库之后」——这里勾选哪些触发位会在写库后调一次翻译。关掉某个触发位不影响其它触发位。
        </p>
        <div class="grid gap-3 sm:grid-cols-3">
          <USwitch v-model="form.trigger_manual_refresh" label="手动『刷新元数据』" />
          <USwitch v-model="form.trigger_scheduled_task" label="计划任务『元数据刷新』/『翻译兜底』" />
          <USwitch v-model="form.trigger_remote_sync" label="远端 Emby 同步" />
        </div>
      </UCard>

      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">连通性测试</h3>
            <UButton
              color="neutral"
              variant="outline"
              icon="i-lucide-flask-conical"
              :loading="testing"
              :disabled="!form.enabled || !form.app_key.trim()"
              @click="runTest"
            >
              翻译一次
            </UButton>
          </div>
        </template>
        <UFormField label="测试文本">
          <UTextarea v-model="testInput.text" :rows="3" class="w-full" />
        </UFormField>
        <UAlert
          v-if="testError"
          color="error"
          icon="i-lucide-triangle-alert"
          class="mt-3"
          :description="testError"
        />
        <div v-if="testResult" class="mt-3 rounded-md border border-default p-3 text-sm">
          <div class="text-muted text-xs">
            源 → {{ testResult.target_lang }} · 用时 {{ testResult.elapsed_ms }} ms · provider {{ testResult.provider }}
          </div>
          <div class="text-muted mt-2 text-xs uppercase tracking-wide">原文</div>
          <div class="text-default whitespace-pre-wrap break-words">{{ testResult.source_text }}</div>
          <div class="text-muted mt-2 text-xs uppercase tracking-wide">译文</div>
          <div class="text-highlighted whitespace-pre-wrap break-words font-medium">{{ testResult.translated_text }}</div>
        </div>
      </UCard>
    </form>
  </SettingsLayout>
</template>
