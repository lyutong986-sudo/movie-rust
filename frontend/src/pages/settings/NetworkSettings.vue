<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import { api, isAdmin, loadAdminData, saveServerSettings, state, systemInfo } from '../../store/app';
import type { NetworkConfiguration } from '../../api/emby';

const baseUrl = computed(() => api.baseUrl || window.location.origin);
const loading = ref(true);
const saving = ref(false);
const error = ref('');
const saved = ref('');

const form = reactive<NetworkConfiguration>({
  LocalAddress: '',
  HttpServerPortNumber: 8096,
  HttpsPortNumber: 8920,
  PublicHttpPort: 8096,
  PublicHttpsPort: 8920,
  CertificatePath: '',
  EnableHttps: false,
  ExternalDomain: '',
  EnableUPnP: false
});

onMounted(async () => {
  if (!isAdmin.value) {
    loading.value = false;
    return;
  }
  try {
    await loadAdminData();
    Object.assign(form, await api.networkConfiguration());
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
});

async function saveNetwork() {
  saving.value = true;
  error.value = '';
  saved.value = '';
  try {
    Object.assign(form, await api.updateNetworkConfiguration(form));
    state.allowRemoteAccess = state.allowRemoteAccess;
    state.enableUPNP = form.EnableUPnP;
    await saveServerSettings();
    saved.value = '网络配置已保存';
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
      <p class="text-muted text-sm">当前账户不能修改网络配置。</p>
    </div>

    <form v-else class="space-y-4" @submit.prevent="saveNetwork">
      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">远程访问</h3>
        </template>
        <div class="grid gap-3 sm:grid-cols-2">
          <USwitch v-model="state.allowRemoteAccess" label="允许远程访问" />
          <USwitch v-model="form.EnableUPnP" label="自动端口映射 (UPnP)" />
        </div>
      </UCard>

      <UAlert v-if="error" color="error" icon="i-lucide-triangle-alert" :description="error" />
      <UAlert v-else-if="saved" color="success" icon="i-lucide-check" :description="saved" />

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">端口与绑定</h3>
        </template>
        <div class="grid gap-4 sm:grid-cols-2">
          <UFormField label="绑定本地 IP" hint="留空即监听全部地址">
            <UInput v-model.trim="form.LocalAddress" placeholder="例如 0.0.0.0 或 127.0.0.1" class="w-full" />
          </UFormField>
          <UFormField label="外部域名 / DDNS" hint="公网访问时的域名">
            <UInput v-model.trim="form.ExternalDomain" placeholder="https://movie.example.com" class="w-full" />
          </UFormField>
          <UFormField label="HTTP 端口">
            <UInput v-model.number="form.HttpServerPortNumber" type="number" :min="1" :max="65535" class="w-full" />
          </UFormField>
          <UFormField label="HTTPS 端口">
            <UInput v-model.number="form.HttpsPortNumber" type="number" :min="1" :max="65535" class="w-full" />
          </UFormField>
          <UFormField label="对外 HTTP 端口">
            <UInput v-model.number="form.PublicHttpPort" type="number" :min="1" :max="65535" class="w-full" />
          </UFormField>
          <UFormField label="对外 HTTPS 端口">
            <UInput v-model.number="form.PublicHttpsPort" type="number" :min="1" :max="65535" class="w-full" />
          </UFormField>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">HTTPS</h3>
            <USwitch v-model="form.EnableHttps" label="启用 HTTPS" />
          </div>
        </template>
        <div class="grid gap-4">
          <UFormField label="证书路径" hint="支持 PFX / PEM 证书文件">
            <UInput v-model.trim="form.CertificatePath" placeholder="/etc/ssl/movie-rust.pfx" class="w-full" />
          </UFormField>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">接入地址</h3>
        </template>
        <div class="grid gap-3 sm:grid-cols-3">
          <div class="rounded-lg border border-default bg-elevated/20 p-3">
            <p class="text-muted text-xs">主入口</p>
            <p class="text-highlighted mt-1 break-all font-mono text-sm">
              {{ systemInfo?.LocalAddress || baseUrl }}
            </p>
            <p class="text-muted mt-1 text-xs">标准根路径</p>
          </div>
          <div class="rounded-lg border border-default bg-elevated/20 p-3">
            <p class="text-muted text-xs">Emby 兼容入口</p>
            <p class="text-highlighted mt-1 break-all font-mono text-sm">{{ baseUrl }}/emby</p>
            <p class="text-muted mt-1 text-xs">适配 Emby 客户端 / 本地播放器</p>
          </div>
          <div class="rounded-lg border border-default bg-elevated/20 p-3">
            <p class="text-muted text-xs">MediaBrowser 兼容入口</p>
            <p class="text-highlighted mt-1 break-all font-mono text-sm">{{ baseUrl }}/mediabrowser</p>
            <p class="text-muted mt-1 text-xs">兼容老式客户端路径</p>
          </div>
        </div>
        <template #footer>
          <div class="flex justify-end">
            <UButton type="submit" :loading="saving" icon="i-lucide-save">保存网络设置</UButton>
          </div>
        </template>
      </UCard>
    </form>
  </SettingsLayout>
</template>
