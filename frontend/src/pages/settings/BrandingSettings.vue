<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import type { BrandingConfiguration } from '../../api/emby';
import { api, isAdmin } from '../../store/app';

const loading = ref(true);
const saving = ref(false);
const error = ref('');
const saved = ref('');

const form = reactive<BrandingConfiguration>({
  LoginDisclaimer: '',
  CustomCss: '',
  SplashscreenEnabled: false
});

onMounted(async () => {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  try {
    Object.assign(form, await api.brandingConfiguration());
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
    Object.assign(form, await api.updateBrandingConfiguration(form));
    saved.value = 'Branding 已保存';
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
      <p class="text-muted text-sm">当前账户不能修改品牌化配置。</p>
    </div>

    <form v-else class="space-y-4" @submit.prevent="save">
      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">登录页文案</h3>
        </template>
        <UFormField label="登录免责声明" hint="将显示在登录页底部，支持多行文本">
          <UTextarea v-model="form.LoginDisclaimer" :rows="4" class="w-full" />
        </UFormField>
      </UCard>

      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">启动闪屏</h3>
            <USwitch v-model="form.SplashscreenEnabled" label="启用启动闪屏" />
          </div>
        </template>
        <p class="text-muted text-xs">
          启用后，Emby 客户端在连接到服务器时会显示品牌闪屏。
        </p>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">自定义 CSS</h3>
        </template>
        <UFormField label="注入到 Web 客户端的 CSS" hint="通过 /Branding/Css 下发给所有兼容客户端">
          <UTextarea
            v-model="form.CustomCss"
            :rows="14"
            class="w-full font-mono text-xs"
            placeholder="/* 例如 */\n.cardBox { border-radius: 14px; }"
          />
        </UFormField>
        <template #footer>
          <div class="flex justify-end">
            <UButton type="submit" :loading="saving" icon="i-lucide-save">保存 Branding</UButton>
          </div>
        </template>
      </UCard>
    </form>
  </SettingsLayout>
</template>
