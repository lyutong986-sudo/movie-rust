<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import SettingsNav from '../../components/SettingsNav.vue';
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
  <section class="settings-shell">
    <SettingsNav />

    <div class="settings-content">
      <div v-if="!isAdmin" class="empty">
        <p>转码</p>
        <h2>需要管理员权限</h2>
        <p>当前账户不能修改服务端转码配置。</p>
      </div>

      <form v-else class="settings-page settings-form transcoding-settings" @submit.prevent="saveEncodingSettings">
        <div v-if="loading" class="empty">
          <p>正在读取转码设置...</p>
        </div>

        <template v-else>
          <div class="stat-grid">
            <article>
              <small>转码状态</small>
              <strong>{{ form.EnableTranscoding ? '已启用' : '已关闭' }}</strong>
              <span>控制 HLS/HTTP 转码入口是否可用</span>
            </article>
            <article>
              <small>硬件加速</small>
              <strong>{{ form.HardwareAccelerationType || '无' }}</strong>
              <span>对齐 Emby 的硬件加速类型选择</span>
            </article>
            <article>
              <small>ffmpeg</small>
              <strong>{{ form.EncoderLocationType === 'Custom' ? '自定义' : '系统版本' }}</strong>
              <span>当前转码器程序来源</span>
            </article>
          </div>

          <label class="check-row">
            <input v-model="form.EnableTranscoding" type="checkbox" />
            <span>启用视频转码</span>
          </label>

          <label>
            硬件加速类型
            <select v-model="form.HardwareAccelerationType">
              <option value="">无</option>
              <option value="qsv">Intel Quick Sync</option>
              <option value="h264_omx">OpenMAX OMX</option>
              <option value="nvenc">Nvidia NVENC</option>
              <option value="vaapi">Video Acceleration API (VA API)</option>
            </select>
            <span>启用前请确认系统驱动和 ffmpeg encoder 可用。</span>
          </label>

          <label v-if="showVaapiDevice">
            VAAPI 设备
            <input v-model.trim="form.VaapiDevice" type="text" placeholder="/dev/dri/renderD128" />
            <span>通常为 Linux 上的 VAAPI 渲染设备路径。</span>
          </label>

          <label>
            转码线程数
            <select v-model.number="form.EncodingThreadCount">
              <option :value="-1">自动</option>
              <option v-for="count in 8" :key="count" :value="count">{{ count }}</option>
              <option :value="0">最大</option>
            </select>
            <span>自动会保守使用部分 CPU，最大会使用所有可用核心。</span>
          </label>

          <label class="check-row">
            <input v-model="form.EnableThrottling" type="checkbox" :disabled="!form.EnableTranscoding" />
            <span>启用转码限速</span>
          </label>

          <label>
            ffmpeg 版本
            <select v-model="form.EncoderLocationType">
              <option value="System">使用系统安装版本</option>
              <option value="Custom">使用自定义版本</option>
            </select>
          </label>

          <label v-if="isCustomEncoder">
            ffmpeg 路径
            <div class="path-control">
              <input v-model.trim="form.EncoderAppPath" type="text" autocomplete="off" required />
              <button class="secondary icon-button" type="button" title="浏览" @click="openBrowser('encoder')">⌕</button>
            </div>
            <span>选择 ffmpeg 或 ffmpeg.exe 的完整路径。</span>
          </label>

          <label>
            转码临时目录
            <div class="path-control">
              <input v-model.trim="form.TranscodingTempPath" type="text" autocomplete="off" />
              <button class="secondary icon-button" type="button" title="浏览" @click="openBrowser('temp')">⌕</button>
            </div>
            <span>用于保存 HLS 播放列表、分片和临时转码输出。</span>
          </label>

          <label>
            音频下混增益
            <input v-model.number="form.DownMixAudioBoost" type="number" min="0.5" max="3" step="0.1" required />
            <span>当多声道音频下混到立体声时应用的音量比例。</span>
          </label>

          <label>
            H264 编码预设
            <select v-model="form.H264Preset">
              <option value="">自动</option>
              <option value="veryslow">veryslow</option>
              <option value="slower">slower</option>
              <option value="slow">slow</option>
              <option value="medium">medium</option>
              <option value="fast">fast</option>
              <option value="faster">faster</option>
              <option value="veryfast">veryfast</option>
              <option value="superfast">superfast</option>
              <option value="ultrafast">ultrafast</option>
            </select>
            <span>越慢通常压缩率越高，实时播放建议选择 fast 或更快。</span>
          </label>

          <label>
            H264 CRF
            <input v-model.number="form.H264Crf" type="number" min="0" max="51" step="1" />
            <span>数值越低质量越高，23 是常见默认值。</span>
          </label>

          <label>
            最大并发转码数
            <input v-model.number="form.MaxTranscodeSessions" type="number" min="1" max="64" step="1" />
            <span>服务端会按转码任务生命周期控制并发。</span>
          </label>

          <p v-if="error" class="form-error">{{ error }}</p>
          <p v-if="saved" class="form-success">{{ saved }}</p>

          <div class="button-row">
            <button type="submit" :disabled="saving">{{ saving ? '保存中...' : '保存' }}</button>
            <button class="secondary" type="button" :disabled="loading || saving" @click="loadEncodingSettings">重新读取</button>
          </div>
        </template>
      </form>
    </div>

    <section v-if="browser.open" class="folder-picker-backdrop" @click.self="browser.open = false">
      <div class="folder-picker">
        <header class="folder-picker-head">
          <div>
            <small>{{ browser.title }}</small>
            <h3>{{ browser.currentPath || '选择驱动器' }}</h3>
          </div>
          <button class="close" type="button" aria-label="关闭" @click="browser.open = false">×</button>
        </header>

        <div class="folder-picker-toolbar">
          <button class="secondary" type="button" :disabled="browser.loading" @click="loadDrives">驱动器</button>
          <button class="secondary" type="button" :disabled="browser.loading || !browser.currentPath" @click="goUp">上一级</button>
          <button type="button" :disabled="browser.loading || !canUseCurrentPath" @click="useCurrentPath">
            使用当前位置
          </button>
        </div>

        <p v-if="browser.error" class="form-error">{{ browser.error }}</p>
        <div class="folder-list" :aria-busy="browser.loading">
          <button
            v-for="entry in browser.entries"
            :key="entry.Path"
            class="folder-entry"
            type="button"
            :disabled="browser.loading"
            @click="browse(entry)"
          >
            <span>{{ entry.Type === 'File' ? '文件' : '目录' }}</span>
            <strong>{{ entry.Name || entry.Path }}</strong>
            <small>{{ entry.Path }}</small>
          </button>
          <div v-if="browser.loading" class="folder-empty">正在读取目录...</div>
          <div v-else-if="!browser.entries.length" class="folder-empty">此目录没有可选择的项目</div>
        </div>
      </div>
    </section>
  </section>
</template>
