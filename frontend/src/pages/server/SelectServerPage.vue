<script setup lang="ts">
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import { currentServerUrl, removeServer, servers, switchServer } from '../../store/app';

const router = useRouter();
const serverList = computed(() => servers.value);

async function select(url: string) {
  await switchServer(url);
  await router.replace('/server/login');
}

function remove(url: string) {
  removeServer(url);
}
</script>

<template>
  <div class="space-y-6">
    <header>
      <p class="text-muted text-xs">Movie Rust</p>
      <h2 class="text-highlighted text-xl font-semibold">选择服务器</h2>
    </header>

    <div v-if="serverList.length" class="flex flex-col gap-3">
      <article
        v-for="server in serverList"
        :key="server.Url"
        class="flex flex-col gap-3 rounded-xl border border-default bg-elevated/30 p-4 sm:flex-row sm:items-center sm:justify-between"
      >
        <div class="min-w-0">
          <strong class="text-highlighted text-sm">{{ server.Name }}</strong>
          <p class="text-muted truncate font-mono text-xs">{{ server.Url }}</p>
          <p class="text-muted text-xs">
            {{ server.ProductName || 'Movie Rust' }} {{ server.Version || '' }}
          </p>
        </div>
        <div class="flex flex-wrap gap-2">
          <UButton
            :variant="server.Url === currentServerUrl ? 'soft' : 'solid'"
            :color="server.Url === currentServerUrl ? 'primary' : 'primary'"
            icon="i-lucide-plug"
            @click="select(server.Url)"
          >
            {{ server.Url === currentServerUrl ? '当前服务器' : '连接' }}
          </UButton>
          <UButton
            v-if="serverList.length > 1"
            color="neutral"
            variant="ghost"
            icon="i-lucide-trash-2"
            @click="remove(server.Url)"
          >
            移除
          </UButton>
        </div>
      </article>
    </div>

    <div
      v-else
      class="flex flex-col items-center gap-2 rounded-xl border border-dashed border-default bg-elevated/10 p-8 text-center"
    >
      <UIcon name="i-lucide-server-off" class="size-8 text-muted" />
      <p class="text-highlighted text-sm font-medium">还没有保存的服务器</p>
      <p class="text-muted text-xs">这一页对应 Jellyfin 的服务器选择页，适合管理多个地址。</p>
    </div>

    <div class="flex justify-end gap-2">
      <UButton color="neutral" variant="ghost" icon="i-lucide-arrow-left" @click="router.push('/server/login')">
        返回登录
      </UButton>
      <UButton icon="i-lucide-plus" @click="router.push('/server/add')">添加服务器</UButton>
    </div>
  </div>
</template>
