<script setup lang="ts">
import { ref } from 'vue';
import SettingsLayout from '../../layouts/SettingsLayout.vue';
import { api, isAdmin } from '../../store/app';

const revealed = ref(false);

async function copyToken() {
  if (!api.token) return;
  try {
    await navigator.clipboard.writeText(api.token);
  } catch {
    // ignore clipboard failures
  }
}
</script>

<template>
  <SettingsLayout>
    <div
      v-if="!isAdmin"
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default p-10 text-center"
    >
      <UIcon name="i-lucide-lock" class="size-10 text-muted" />
      <h3 class="text-highlighted text-lg font-semibold">需要管理员权限</h3>
      <p class="text-muted text-sm">当前账户不能查看令牌兼容说明。</p>
    </div>

    <div v-else class="space-y-4">
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h3 class="text-highlighted text-sm font-semibold">当前访问令牌</h3>
            <UBadge variant="subtle" color="primary">X-Emby-Token</UBadge>
          </div>
        </template>
        <p class="text-muted text-sm">
          当前版本优先兼容 Emby/Jellyfin 登录令牌，而不是独立 API Key 管理页。
        </p>
        <div class="mt-3 flex items-center gap-2">
          <div
            class="flex-1 overflow-x-auto rounded-md border border-default bg-elevated/40 px-3 py-2 font-mono text-xs"
          >
            <template v-if="api.token">
              {{ revealed ? api.token : '•'.repeat(Math.min(api.token.length, 32)) }}
            </template>
            <span v-else class="text-muted">当前没有令牌</span>
          </div>
          <UButton
            :icon="revealed ? 'i-lucide-eye-off' : 'i-lucide-eye'"
            color="neutral"
            variant="subtle"
            :disabled="!api.token"
            @click="revealed = !revealed"
          />
          <UButton
            icon="i-lucide-copy"
            color="neutral"
            variant="subtle"
            :disabled="!api.token"
            @click="copyToken"
          />
        </div>
      </UCard>

      <div class="grid gap-3 md:grid-cols-2">
        <UCard>
          <div class="flex items-start gap-3">
            <UIcon name="i-lucide-shield-check" class="text-primary size-5" />
            <div>
              <h4 class="text-highlighted text-sm font-semibold">兼容方式</h4>
              <p class="text-muted mt-1 text-xs">
                服务端已支持 <code>X-Emby-Token</code>、<code>X-Emby-Authorization</code>、
                <code>Authorization</code> 和 <code>api_key</code>。
              </p>
            </div>
          </div>
        </UCard>
        <UCard>
          <div class="flex items-start gap-3">
            <UIcon name="i-lucide-git-pull-request-arrow" class="text-primary size-5" />
            <div>
              <h4 class="text-highlighted text-sm font-semibold">后续计划</h4>
              <p class="text-muted mt-1 text-xs">
                下一步可以继续补独立 API Key 生成、撤销与用途标记，使它更贴近 Jellyfin 后台逻辑。
              </p>
            </div>
          </div>
        </UCard>
      </div>
    </div>
  </SettingsLayout>
</template>
