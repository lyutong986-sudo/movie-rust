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
  <section class="server-screen">
    <div class="server-card wizard-card">
      <div class="server-brand">
        <div class="mark">MR</div>
        <div>
          <p>Movie Rust</p>
          <h1>选择服务器</h1>
        </div>
      </div>

      <div v-if="serverList.length" class="server-list">
        <article v-for="server in serverList" :key="server.Url" class="server-entry">
          <div>
            <strong>{{ server.Name }}</strong>
            <p>{{ server.Url }}</p>
            <p>{{ server.ProductName || 'Movie Rust' }} {{ server.Version || '' }}</p>
          </div>
          <div class="button-row">
            <button
              :class="{ secondary: server.Url !== currentServerUrl }"
              type="button"
              @click="select(server.Url)"
            >
              {{ server.Url === currentServerUrl ? '当前服务器' : '连接' }}
            </button>
            <button v-if="serverList.length > 1" class="secondary" type="button" @click="remove(server.Url)">
              移除
            </button>
          </div>
        </article>
      </div>
      <div v-else class="empty">
        <p>服务器列表</p>
        <h2>还没有保存的服务器</h2>
        <p>这一步对应 Jellyfin 的服务器选择页，适合管理多个地址。</p>
      </div>

      <div class="button-row">
        <button type="button" @click="router.push('/server/add')">添加服务器</button>
        <button class="secondary" type="button" @click="router.push('/server/login')">返回登录</button>
      </div>
    </div>
  </section>
</template>
