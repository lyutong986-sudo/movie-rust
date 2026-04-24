<script setup lang="ts">
import { computed, onMounted } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import { api, isAdmin, loadAdminData, saveServerSettings, state, systemInfo } from '../../store/app';

const baseUrl = computed(() => api.baseUrl || window.location.origin);

onMounted(async () => {
  if (isAdmin.value) {
    await loadAdminData();
  }
});
</script>

<template>
  <SettingsLayout>
    <div
      v-if="!isAdmin"
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
    >
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能修改远程访问设置。</p>
    </div>

    <form v-else class="space-y-4" @submit.prevent="saveServerSettings">
      <UCard>
        <template #header>
          <h3 class="text-highlighted text-sm font-semibold">远程访问</h3>
        </template>
        <div class="grid gap-3 sm:grid-cols-2">
          <USwitch v-model="state.allowRemoteAccess" label="允许远程访问" />
          <USwitch v-model="state.enableUPNP" label="自动端口映射 (UPnP)" />
        </div>
      </UCard>

      <UAlert v-if="state.error" color="error" icon="i-lucide-triangle-alert" :description="state.error" />
      <UAlert v-else-if="state.message" color="success" icon="i-lucide-check" :description="state.message" />

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
            <UButton type="submit" :loading="state.busy" icon="i-lucide-save">保存网络设置</UButton>
          </div>
        </template>
      </UCard>
    </form>
  </SettingsLayout>
</template>
