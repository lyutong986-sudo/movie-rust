<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { RouterView, useRoute, useRouter } from 'vue-router';
import { isAdmin, libraries, loadAdminData, loadLibraries, logout, state, user } from '../store/app';

const router = useRouter();
const route = useRoute();
const searchInput = ref('');

const isAdminSection = computed(() => Boolean(route.meta.admin));
const libraryRouteId = computed(() => String(route.params.id || ''));
const currentTitle = computed(() => {
  if (route.name === 'library') {
    return libraries.value.find((library) => library.Id === libraryRouteId.value)?.Name || '媒体库';
  }

  return String(route.meta.title || '首页');
});
const currentSubtitle = computed(() => {
  if (isAdminSection.value) {
    return '服务器控制台';
  }

  return user.value ? `欢迎回来，${user.value.Name}` : state.serverName;
});

watch(
  () => route.query.q,
  (value) => {
    searchInput.value = typeof value === 'string' ? value : '';
  },
  { immediate: true }
);

watch(
  () => route.fullPath,
  async () => {
    if (!isAdminSection.value) {
      return;
    }

    if (!isAdmin.value) {
      await router.replace('/');
      return;
    }

    await loadAdminData();
  }
);

onMounted(async () => {
  if (!libraries.value.length) {
    await loadLibraries();
  }

  if (isAdminSection.value) {
    if (!isAdmin.value) {
      await router.replace('/');
      return;
    }

    await loadAdminData();
  }
});

async function goHome() {
  await router.push('/');
}

async function goSettings() {
  await router.push('/settings');
}

async function goAdminConsole() {
  await router.push('/settings/server');
}

async function openLibrary(libraryId: string) {
  await router.push(`/library/${libraryId}`);
}

async function submitSearch() {
  const query = searchInput.value.trim();
  if (!query) {
    if (route.name === 'search') {
      await router.replace('/');
    }
    return;
  }

  await router.push({
    name: 'search',
    query: { q: query }
  });
}

async function handleLogout() {
  logout();
  await router.replace('/server/login');
}
</script>

<template>
  <div class="app-shell">
    <aside class="nav-drawer">
      <div class="brand">
        <div class="mark">MR</div>
        <div>
          <p>Movie Rust</p>
          <h1>{{ state.serverName }}</h1>
        </div>
      </div>

      <div class="nav-list">
        <button type="button" :class="{ active: route.path === '/' }" @click="goHome">
          <span>首页</span>
          <small>Home</small>
        </button>
        <button
          type="button"
          :class="{ active: route.path.startsWith('/settings') }"
          @click="goSettings"
        >
          <span>设置</span>
          <small>{{ isAdmin ? 'Admin' : 'User' }}</small>
        </button>
        <button
          v-if="isAdmin"
          type="button"
          :class="{
            active:
              route.path.startsWith('/settings/server') ||
              route.path.startsWith('/settings/users') ||
              route.path.startsWith('/settings/network')
          }"
          @click="goAdminConsole"
        >
          <span>控制台</span>
          <small>Server</small>
        </button>
        <button
          v-for="library in libraries"
          :key="library.Id"
          type="button"
          :class="{ active: route.path.startsWith(`/library/${library.Id}`) }"
          @click="openLibrary(library.Id)"
        >
          <span>{{ library.Name }}</span>
          <small>{{ library.ChildCount || 0 }}</small>
        </button>
      </div>

      <div class="drawer-actions">
        <button class="secondary" type="button" @click="router.back()">返回</button>
        <button type="button" @click="handleLogout">退出</button>
      </div>
    </aside>

    <section class="main-view">
      <header class="top-bar">
        <div>
          <p>{{ currentSubtitle }}</p>
          <h2>{{ currentTitle }}</h2>
        </div>

        <form v-if="!isAdminSection" class="search" @submit.prevent="submitSearch">
          <input v-model="searchInput" placeholder="搜索电影、剧集、媒体库" />
          <button type="submit">搜索</button>
        </form>
        <div v-else class="button-row">
          <button class="secondary" type="button" @click="goHome">返回前台</button>
        </div>

        <div class="button-row">
          <button v-if="!isAdminSection" class="secondary" type="button" @click="goSettings">设置</button>
          <button class="icon-button secondary" type="button" title="返回上一页" @click="router.back()">
            ←
          </button>
        </div>
      </header>

      <p v-if="state.message" class="notice">{{ state.message }}</p>
      <p v-if="state.error" class="notice error">{{ state.error }}</p>

      <RouterView />
    </section>
  </div>
</template>
