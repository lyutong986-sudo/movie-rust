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
  <section class="server-screen">
    <div class="server-card">
      <div class="server-brand">
        <div class="mark">MR</div>
        <div>
          <p>Movie Rust</p>
          <h1>添加服务器</h1>
        </div>
      </div>

      <form class="form-stack" @submit.prevent="submit">
        <label>
          服务器地址
          <input v-model="serverUrl" placeholder="http://127.0.0.1:10004 或 https://example.com/emby" />
        </label>
        <p>会先探测 `/System/Info/Public`，成功后保存到本地服务器列表。</p>
        <div class="button-row">
          <button class="secondary" type="button" @click="router.push('/server/select')">返回</button>
          <button :disabled="busy" type="submit">添加并连接</button>
        </div>
      </form>

      <p v-if="error" class="notice error">{{ error }}</p>
    </div>
  </section>
</template>
