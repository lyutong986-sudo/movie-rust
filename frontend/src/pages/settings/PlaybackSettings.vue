<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { PlaybackConfiguration } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(true);
const saving = ref(false);
const error = ref('');
const saved = ref('');

const form = reactive<PlaybackConfiguration>({
  MinResumePct: 5,
  MaxResumePct: 90,
  MinResumeDurationSeconds: 300,
  MinAudiobookResume: 5,
  MaxAudiobookResume: 95
});

onMounted(async () => {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  try {
    Object.assign(form, await api.playbackConfiguration());
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
    Object.assign(form, await api.updatePlaybackConfiguration(form));
    saved.value = '续播阈值已保存';
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
      <p class="text-muted text-sm">当前账户不能修改服务端播放配置。</p>
    </div>

    <form v-else class="space-y-4" @submit.prevent="save">
      <div class="grid gap-3 sm:grid-cols-3">
        <UCard variant="soft">
          <p class="text-muted text-xs">续播阈值下限</p>
          <p class="text-highlighted mt-1 text-base font-semibold">
            {{ form.MinResumePct }}%
          </p>
          <p class="text-muted text-xs">少于此百分比将不保留续播位置</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">续播阈值上限</p>
          <p class="text-highlighted mt-1 text-base font-semibold">
            {{ form.MaxResumePct }}%
          </p>
          <p class="text-muted text-xs">超过此百分比将视为已播放</p>
        </UCard>
        <UCard variant="soft">
          <p class="text-muted text-xs">最短续播时长</p>
          <p class="text-highlighted mt-1 text-base font-semibold">
            {{ Math.round(form.MinResumeDurationSeconds / 60) }} 分钟
          </p>
          <p class="text-muted text-xs">低于此时长不保留续播</p>
        </UCard>
      </div>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">视频续播阈值</h3>
        </template>
        <div class="grid gap-4 sm:grid-cols-3">
          <UFormField label="最低续播百分比" hint="低于此百分比将不保留播放进度">
            <UInput v-model.number="form.MinResumePct" type="number" :min="0" :max="100" class="w-full" />
          </UFormField>
          <UFormField label="最高续播百分比" hint="超过此百分比视为播放完成">
            <UInput v-model.number="form.MaxResumePct" type="number" :min="1" :max="100" class="w-full" />
          </UFormField>
          <UFormField label="最短续播时长（秒）" hint="少于此时长不保留续播">
            <UInput v-model.number="form.MinResumeDurationSeconds" type="number" :min="0" class="w-full" />
          </UFormField>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">有声读物续播阈值</h3>
        </template>
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="最低续播百分比">
            <UInput v-model.number="form.MinAudiobookResume" type="number" :min="0" :max="100" class="w-full" />
          </UFormField>
          <UFormField label="最高续播百分比">
            <UInput v-model.number="form.MaxAudiobookResume" type="number" :min="1" :max="100" class="w-full" />
          </UFormField>
        </div>
        <template #footer>
          <div class="flex justify-end">
            <UButton type="submit" :loading="saving" icon="i-lucide-save">保存播放配置</UButton>
          </div>
        </template>
      </UCard>
    </form>
  </SettingsLayout>
</template>
