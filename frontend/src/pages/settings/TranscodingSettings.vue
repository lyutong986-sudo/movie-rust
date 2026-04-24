<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import { api, isAdmin } from '../../store/app';
import type { EncodingOptions, FileSystemEntryInfo } from '../../api/emby';

const loading = ref(true);
const saving = ref(false);
const error = ref('');
const saved = ref('');

const form = reactive<EncodingOptions>({
  EnableTranscoding: false,
  EnableThrottling: true,
  HardwareAccelerationType: '',
  VaapiDevice: '',
  EncodingThreadCount: -1,
  DownMixAudioBoost: 1,
  EncoderAppPath: 'ffmpeg',
  EncoderLocationType: 'System',
  TranscodingTempPath: '',
  H264Preset: '',
  H264Crf: 23,
  MaxTranscodeSessions: 4
});

const browser = reactive({
  open: false,
  target: 'temp' as 'temp' | 'encoder',
  title: '',
  includeFiles: false,
  loading: false,
  error: '',
  currentPath: '',
  entries: [] as FileSystemEntryInfo[]
});

const isCustomEncoder = computed(() => form.EncoderLocationType === 'Custom');
const showVaapiDevice = computed(() => form.HardwareAccelerationType === 'vaapi');
const canUseCurrentPath = computed(() => Boolean(browser.currentPath.trim()));

const hwaOptions = [
  { label: '无', value: '' },
  { label: 'Intel Quick Sync', value: 'qsv' },
  { label: 'OpenMAX OMX', value: 'h264_omx' },
  { label: 'Nvidia NVENC', value: 'nvenc' },
  { label: 'Video Acceleration API (VAAPI)', value: 'vaapi' }
];

const threadOptions = computed(() => {
  const items: { label: string; value: number }[] = [{ label: '自动', value: -1 }];
  for (let i = 1; i <= 8; i += 1) {
    items.push({ label: String(i), value: i });
  }
  items.push({ label: '最大', value: 0 });
  return items;
});

const encoderLocationOptions = [
  { label: '使用系统安装版本', value: 'System' },
  { label: '使用自定义版本', value: 'Custom' }
];

const presetOptions = [
  { label: '自动', value: '' },
  ...['veryslow', 'slower', 'slow', 'medium', 'fast', 'faster', 'veryfast', 'superfast', 'ultrafast'].map(
    (value) => ({ label: value, value })
  )
];

onMounted(loadEncodingSettings);

async function loadEncodingSettings() {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    Object.assign(form, await api.encodingConfiguration());
  } catch (err) {
    error.value = err instanceof Error ? err.message : '无法读取转码设置';
  } finally {
    loading.value = false;
  }
}

async function saveEncodingSettings() {
  saving.value = true;
  error.value = '';
  saved.value = '';
  try {
    const next = { ...form };
    if (!next.EnableTranscoding) {
      next.EnableThrottling = false;
    }
    const savedOptions = await api.updateEncodingConfiguration(next);
    if (next.EncoderLocationType === 'Custom' || next.EncoderAppPath) {
      Object.assign(
        form,
        await api.updateMediaEncoderPath({
          Path: next.EncoderAppPath,
          PathType: next.EncoderLocationType
        })
      );
    } else {
      Object.assign(form, savedOptions);
    }
    saved.value = '转码设置已保存';
  } catch (err) {
    error.value = err instanceof Error ? err.message : '保存转码设置失败';
  } finally {
    saving.value = false;
  }
}

async function openBrowser(target: 'temp' | 'encoder') {
  browser.open = true;
  browser.target = target;
  browser.title = target === 'encoder' ? '选择 ffmpeg 程序' : '选择转码临时目录';
  browser.includeFiles = target === 'encoder';
  browser.error = '';
  browser.currentPath = '';
  await loadDrives();
}

async function loadDrives() {
  browser.loading = true;
  browser.error = '';
  try {
    browser.entries = await api.environmentDrives();
    browser.currentPath = '';
  } catch (err) {
    browser.error = err instanceof Error ? err.message : '无法读取服务器目录';
  } finally {
    browser.loading = false;
  }
}

async function browse(entry: FileSystemEntryInfo) {
  if (entry.Type === 'File') {
    usePath(entry.Path);
    return;
  }
  browser.loading = true;
  browser.error = '';
  try {
    browser.entries = await api.directoryContents(entry.Path, browser.includeFiles, true);
    browser.currentPath = entry.Path;
  } catch (err) {
    browser.error = err instanceof Error ? err.message : '无法打开目录';
  } finally {
    browser.loading = false;
  }
}

async function goUp() {
  if (!browser.currentPath) {
    return;
  }
  browser.loading = true;
  browser.error = '';
  try {
    const parent = await api.parentPath(browser.currentPath);
    if (parent) {
      browser.entries = await api.directoryContents(parent, browser.includeFiles, true);
      browser.currentPath = parent;
    } else {
      await loadDrives();
    }
  } catch (err) {
    browser.error = err instanceof Error ? err.message : '无法返回上级目录';
  } finally {
    browser.loading = false;
  }
}

function useCurrentPath() {
  if (browser.currentPath) {
    usePath(browser.currentPath);
  }
}

function usePath(path: string) {
  if (browser.target === 'encoder') {
    form.EncoderAppPath = path;
    form.EncoderLocationType = 'Custom';
  } else {
    form.TranscodingTempPath = path;
  }
  browser.open = false;
}
</script>

<template>
  <SettingsLayout>
    <div v-if="!isAdmin" class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center">
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能修改服务端转码配置。</p>
    </div>

    <form v-else class="space-y-4" @submit.prevent="saveEncodingSettings">
      <div v-if="loading" class="flex min-h-[40vh] flex-col items-center justify-center gap-2">
        <UProgress animation="carousel" class="w-48" />
        <p class="text-muted text-sm">正在读取转码设置…</p>
      </div>

      <template v-else>
        <div class="grid gap-3 sm:grid-cols-3">
          <UCard variant="soft">
            <p class="text-muted text-xs">转码状态</p>
            <p class="text-highlighted mt-1 text-base font-semibold">
              {{ form.EnableTranscoding ? '已启用' : '已关闭' }}
            </p>
            <p class="text-muted text-xs">控制 HLS/HTTP 转码入口是否可用</p>
          </UCard>
          <UCard variant="soft">
            <p class="text-muted text-xs">硬件加速</p>
            <p class="text-highlighted mt-1 text-base font-semibold">
              {{ form.HardwareAccelerationType || '无' }}
            </p>
            <p class="text-muted text-xs">对齐 Emby 的硬件加速类型</p>
          </UCard>
          <UCard variant="soft">
            <p class="text-muted text-xs">ffmpeg</p>
            <p class="text-highlighted mt-1 text-base font-semibold">
              {{ form.EncoderLocationType === 'Custom' ? '自定义' : '系统版本' }}
            </p>
            <p class="text-muted text-xs">当前转码器来源</p>
          </UCard>
        </div>

        <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
        <UAlert v-if="saved" color="success" icon="i-lucide-check" :description="saved" />

        <UCard>
          <template #header>
            <div class="flex items-center justify-between">
              <h3 class="text-highlighted text-sm font-semibold">转码总开关</h3>
              <USwitch v-model="form.EnableTranscoding" label="启用视频转码" />
            </div>
          </template>
          <div class="grid gap-4 sm:grid-cols-2">
            <UFormField label="硬件加速类型" hint="启用前请确认系统驱动和 ffmpeg encoder 可用">
              <USelect v-model="form.HardwareAccelerationType" :items="hwaOptions" class="w-full" />
            </UFormField>
            <UFormField v-if="showVaapiDevice" label="VAAPI 设备" hint="通常为 Linux 上的 VAAPI 渲染设备路径">
              <UInput v-model.trim="form.VaapiDevice" placeholder="/dev/dri/renderD128" class="w-full" />
            </UFormField>
            <UFormField label="转码线程数" hint="自动会保守使用部分 CPU">
              <USelect v-model.number="form.EncodingThreadCount" :items="threadOptions" class="w-full" />
            </UFormField>
            <div class="flex items-end">
              <USwitch
                v-model="form.EnableThrottling"
                :disabled="!form.EnableTranscoding"
                label="启用转码限速"
              />
            </div>
          </div>
        </UCard>

        <UCard>
          <template #header>
            <h3 class="text-highlighted text-sm font-semibold">ffmpeg / 临时目录</h3>
          </template>
          <div class="grid gap-4 sm:grid-cols-2">
            <UFormField label="ffmpeg 版本">
              <USelect v-model="form.EncoderLocationType" :items="encoderLocationOptions" class="w-full" />
            </UFormField>
            <UFormField v-if="isCustomEncoder" label="ffmpeg 路径" hint="选择 ffmpeg 或 ffmpeg.exe 的完整路径">
              <div class="flex gap-2">
                <UInput v-model.trim="form.EncoderAppPath" class="flex-1" />
                <UButton
                  color="neutral"
                  variant="subtle"
                  icon="i-lucide-folder-search"
                  @click="openBrowser('encoder')"
                />
              </div>
            </UFormField>
            <UFormField label="转码临时目录" class="sm:col-span-2" hint="用于保存 HLS 播放列表、分片和临时转码输出">
              <div class="flex gap-2">
                <UInput v-model.trim="form.TranscodingTempPath" class="flex-1" />
                <UButton
                  color="neutral"
                  variant="subtle"
                  icon="i-lucide-folder-search"
                  @click="openBrowser('temp')"
                />
              </div>
            </UFormField>
          </div>
        </UCard>

        <UCard>
          <template #header>
            <h3 class="text-highlighted text-sm font-semibold">编码质量</h3>
          </template>
          <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <UFormField label="音频下混增益" hint="多声道下混到立体声的音量比例">
              <UInput v-model.number="form.DownMixAudioBoost" type="number" :min="0.5" :max="3" step="0.1" class="w-full" />
            </UFormField>
            <UFormField label="H264 编码预设" hint="实时播放建议 fast 或更快">
              <USelect v-model="form.H264Preset" :items="presetOptions" class="w-full" />
            </UFormField>
            <UFormField label="H264 CRF" hint="数值越低质量越高，23 为常见默认">
              <UInput v-model.number="form.H264Crf" type="number" :min="0" :max="51" class="w-full" />
            </UFormField>
            <UFormField label="最大并发转码数">
              <UInput v-model.number="form.MaxTranscodeSessions" type="number" :min="1" :max="64" class="w-full" />
            </UFormField>
          </div>
          <template #footer>
            <div class="flex justify-end gap-2">
              <UButton
                type="button"
                color="neutral"
                variant="subtle"
                :disabled="loading || saving"
                icon="i-lucide-refresh-ccw"
                @click="loadEncodingSettings"
              >
                重新读取
              </UButton>
              <UButton type="submit" :loading="saving" icon="i-lucide-save">保存</UButton>
            </div>
          </template>
        </UCard>
      </template>
    </form>

    <UModal v-model:open="browser.open" :title="browser.title" :ui="{ content: 'max-w-2xl' }">
      <template #body>
        <div class="space-y-3">
          <div class="flex items-center gap-2">
            <UButton color="neutral" variant="subtle" :disabled="browser.loading" @click="loadDrives">驱动器</UButton>
            <UButton
              color="neutral"
              variant="subtle"
              icon="i-lucide-arrow-up"
              :disabled="browser.loading || !browser.currentPath"
              @click="goUp"
            >
              上一级
            </UButton>
            <div class="flex-1 truncate rounded-md border border-default bg-elevated/40 px-3 py-1.5 font-mono text-xs">
              {{ browser.currentPath || '选择驱动器' }}
            </div>
            <UButton
              icon="i-lucide-check"
              :disabled="browser.loading || !canUseCurrentPath"
              @click="useCurrentPath"
            >
              使用当前位置
            </UButton>
          </div>

          <UAlert v-if="browser.error" color="error" :description="browser.error" icon="i-lucide-triangle-alert" />

          <div class="max-h-80 space-y-1 overflow-y-auto rounded-lg border border-default bg-elevated/20 p-2">
            <button
              v-for="entry in browser.entries"
              :key="entry.Path"
              class="flex w-full items-start gap-3 rounded-md p-2 text-start transition hover:bg-elevated/60 disabled:opacity-50"
              type="button"
              :disabled="browser.loading"
              @click="browse(entry)"
            >
              <UIcon
                :name="entry.Type === 'File' ? 'i-lucide-file' : 'i-lucide-folder'"
                class="mt-1 size-4 text-primary"
              />
              <div class="flex-1 overflow-hidden">
                <div class="text-highlighted truncate text-sm">{{ entry.Name || entry.Path }}</div>
                <div class="text-muted truncate font-mono text-[11px]">{{ entry.Path }}</div>
              </div>
            </button>
            <div v-if="browser.loading" class="text-muted p-4 text-center text-sm">正在读取目录…</div>
            <div v-else-if="!browser.entries.length" class="text-muted p-4 text-center text-sm">
              此目录没有可选择的项目
            </div>
          </div>
        </div>
      </template>
    </UModal>
  </SettingsLayout>
</template>
