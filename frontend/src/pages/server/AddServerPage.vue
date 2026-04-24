<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { addServer } from '../../store/app';

const router = useRouter();
const serverUrl = ref('');
const busy = ref(false);
const error = ref('');

async function submit() {
  busy.value = true;
  error.value = '';

  try {
    if (!serverUrl.value.trim()) {
      throw new Error('请输入服务器地址');
    }
    await addServer(serverUrl.value);
    await router.replace('/server/login');
  } catch (submitError) {
    error.value = submitError instanceof Error ? submitError.message : String(submitError);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="space-y-6">
    <header>
      <p class="text-muted text-xs">Movie Rust</p>
      <h2 class="text-highlighted text-xl font-semibold">添加服务器</h2>
    </header>

    <form class="space-y-4" @submit.prevent="submit">
      <UFormField label="服务器地址" required hint="示例：http://127.0.0.1:10004 或 https://example.com/emby">
        <UInput
          v-model="serverUrl"
          icon="i-lucide-globe"
          placeholder="http://127.0.0.1:10004"
          class="w-full"
        />
      </UFormField>
      <p class="text-muted text-xs">
        会先探测 <code class="rounded bg-elevated px-1.5 py-0.5 text-[11px]">/System/Info/Public</code>，成功后保存到本地服务器列表。
      </p>

      <UAlert v-if="error" color="error" variant="subtle" icon="i-lucide-triangle-alert" :title="error" />

      <div class="flex justify-end gap-2 pt-2">
        <UButton color="neutral" variant="ghost" icon="i-lucide-arrow-left" @click="router.push('/server/select')">
          返回
        </UButton>
        <UButton type="submit" :loading="busy" icon="i-lucide-plug-2">添加并连接</UButton>
      </div>
    </form>
  </div>
</template>
