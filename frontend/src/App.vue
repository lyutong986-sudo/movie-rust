<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { EmbyApi } from './api/emby';
import type { BaseItemDto } from './api/emby';

const api = new EmbyApi(import.meta.env.VITE_API_BASE || '');

const state = reactive({
  serverName: 'Movie Rust',
  username: 'admin',
  password: 'admin123',
  libraryName: '电影',
  libraryPath: '',
  collectionType: 'movies',
  selectedLibraryId: '',
  search: '',
  busy: false,
  message: '',
  error: ''
});

const user = ref(api.user);
const libraries = ref<BaseItemDto[]>([]);
const items = ref<BaseItemDto[]>([]);
const selectedItem = ref<BaseItemDto | null>(null);
const parentStack = ref<BaseItemDto[]>([]);

const selectedLibrary = computed(() =>
  libraries.value.find((library) => library.Id === state.selectedLibraryId)
);
const currentParentName = computed(() => parentStack.value.at(-1)?.Name || selectedLibrary.value?.Name || '媒体库');
const selectedMediaSource = computed(() => selectedItem.value?.MediaSources?.[0]);
const selectedStreams = computed(() => selectedMediaSource.value?.MediaStreams || selectedItem.value?.MediaStreams || []);

onMounted(async () => {
  await loadPublicInfo();
  if (api.isAuthenticated) {
    await loadLibraries();
  }
});

async function loadPublicInfo() {
  try {
    const info = (await api.publicInfo()) as { ServerName?: string };
    state.serverName = info.ServerName || state.serverName;
  } catch {
    state.serverName = 'Movie Rust';
  }
}

async function login() {
  await run(async () => {
    const result = await api.login(state.username, state.password);
    user.value = result.User;
    await loadLibraries();
  }, '已登录');
}

async function loadLibraries() {
  await run(async () => {
    const result = await api.libraries();
    libraries.value = result.Items;
    if (!state.selectedLibraryId && libraries.value.length > 0) {
      state.selectedLibraryId = libraries.value[0].Id;
    }
    await loadItems();
  });
}

async function loadItems() {
  if (!state.selectedLibraryId) {
    items.value = [];
    return;
  }

  await run(async () => {
    const parentId = parentStack.value.at(-1)?.Id || state.selectedLibraryId;
    const result = await api.items(parentId, state.search, Boolean(state.search.trim()));
    items.value = result.Items;
  });
}

async function selectLibrary(libraryId: string) {
  state.selectedLibraryId = libraryId;
  state.search = '';
  parentStack.value = [];
  await loadItems();
}

async function backToParent() {
  parentStack.value.pop();
  await loadItems();
}

async function createLibrary() {
  await run(async () => {
    const library = await api.createLibrary({
      Name: state.libraryName,
      Path: state.libraryPath,
      CollectionType: state.collectionType
    });
    libraries.value.push(library);
    state.selectedLibraryId = library.Id;
    state.libraryPath = '';
  }, '媒体库已创建');
}

async function scan() {
  await run(async () => {
    const summary = await api.scan();
    await loadItems();
    state.message = `扫描完成：${summary.ImportedItems} 个条目`;
  });
}

function logout() {
  api.logout();
  user.value = null;
  libraries.value = [];
  items.value = [];
  selectedItem.value = null;
}

function openItem(item: BaseItemDto) {
  if (item.IsFolder) {
    parentStack.value.push(item);
    loadItems();
    return;
  }

  selectedItem.value = item;
}

async function toggleFavorite(item: BaseItemDto) {
  await run(async () => {
    const userData = await api.markFavorite(item.Id, !item.UserData.IsFavorite);
    applyUserData(item.Id, userData);
  });
}

async function togglePlayed(item: BaseItemDto) {
  await run(async () => {
    const userData = await api.markPlayed(item.Id, !item.UserData.Played);
    applyUserData(item.Id, userData);
  });
}

function applyUserData(itemId: string, userData: BaseItemDto['UserData']) {
  const item = items.value.find((candidate) => candidate.Id === itemId);
  if (item) {
    item.UserData = { ...item.UserData, ...userData };
  }
  if (selectedItem.value?.Id === itemId) {
    selectedItem.value.UserData = { ...selectedItem.value.UserData, ...userData };
  }
}

function itemSubtitle(item: BaseItemDto) {
  if (item.Type === 'Episode') {
    const season = item.ParentIndexNumber ? `S${String(item.ParentIndexNumber).padStart(2, '0')}` : '';
    const episode = item.IndexNumber ? `E${String(item.IndexNumber).padStart(2, '0')}` : '';
    return [item.SeriesName, `${season}${episode}`].filter(Boolean).join(' ');
  }

  if (item.IsFolder) {
    return `${item.Type} · ${item.ChildCount || 0}`;
  }

  return item.MediaSources?.[0]?.Container || item.Container || item.MediaType || item.Type;
}

function streamLabel(type: string) {
  if (type === 'Video') return '视频';
  if (type === 'Audio') return '音频';
  if (type === 'Subtitle') return '字幕';
  return type;
}

function streamText(stream: NonNullable<BaseItemDto['MediaStreams']>[number]) {
  const parts = [
    stream.DisplayTitle,
    stream.Codec,
    stream.Language,
    stream.Width && stream.Height ? `${stream.Width}x${stream.Height}` : '',
    stream.IsExternal ? '外挂' : ''
  ].filter(Boolean);
  return parts.join(' · ') || '默认轨道';
}

function fileSize(size?: number) {
  if (!size) return '';
  const gb = size / 1024 / 1024 / 1024;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  return `${(size / 1024 / 1024).toFixed(1)} MB`;
}

async function run(task: () => Promise<void>, success = '') {
  state.busy = true;
  state.error = '';
  if (success) {
    state.message = '';
  }

  try {
    await task();
    if (success) {
      state.message = success;
    }
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.busy = false;
  }
}
</script>

<template>
  <main class="shell">
    <aside class="sidebar">
      <div class="brand">
        <div class="mark">MR</div>
        <div>
          <h1>{{ state.serverName }}</h1>
          <p v-if="user">{{ user.Name }}</p>
        </div>
      </div>

      <form v-if="!user" class="login" @submit.prevent="login">
        <label>
          账号
          <input v-model="state.username" autocomplete="username" />
        </label>
        <label>
          密码
          <input v-model="state.password" type="password" autocomplete="current-password" />
        </label>
        <button :disabled="state.busy" type="submit">登录</button>
      </form>

      <template v-else>
        <nav class="libraries">
          <button
            v-for="library in libraries"
            :key="library.Id"
            :class="{ active: library.Id === state.selectedLibraryId }"
            type="button"
            @click="selectLibrary(library.Id)"
          >
            <span>{{ library.Name }}</span>
            <small>{{ library.ChildCount || 0 }}</small>
          </button>
        </nav>

        <form class="library-form" @submit.prevent="createLibrary">
          <input v-model="state.libraryName" placeholder="名称" />
          <input v-model="state.libraryPath" placeholder="本地路径，例如 D:\Movies" />
          <select v-model="state.collectionType">
            <option value="movies">电影</option>
            <option value="tvshows">剧集</option>
            <option value="music">音乐</option>
          </select>
          <button :disabled="state.busy || !state.libraryPath" type="submit">添加</button>
        </form>

        <div class="sidebar-actions">
          <button :disabled="state.busy" type="button" title="扫描" @click="scan">扫描</button>
          <button type="button" title="退出" @click="logout">退出</button>
        </div>
      </template>
    </aside>

    <section class="content">
      <header class="toolbar">
        <div>
          <p>{{ selectedLibrary?.CollectionType || 'media' }}</p>
          <h2>{{ currentParentName }}</h2>
        </div>
        <div class="search">
          <input v-model="state.search" placeholder="搜索标题" @keydown.enter.prevent="loadItems" />
          <button :disabled="state.busy" type="button" @click="loadItems">搜索</button>
        </div>
      </header>

      <div v-if="parentStack.length" class="crumbs">
        <button type="button" title="返回上级" @click="backToParent">‹</button>
        <span>{{ selectedLibrary?.Name }}</span>
        <span v-for="parent in parentStack" :key="parent.Id">/ {{ parent.Name }}</span>
      </div>

      <p v-if="state.error" class="notice error">{{ state.error }}</p>
      <p v-else-if="state.message" class="notice">{{ state.message }}</p>

      <div v-if="!user" class="empty">
        <h2>连接媒体服务器</h2>
        <p>使用默认账号登录后即可添加本地媒体目录。</p>
      </div>

      <div v-else-if="items.length === 0" class="empty">
        <h2>暂无条目</h2>
        <p>添加媒体库路径并扫描后，影片会显示在这里。</p>
      </div>

      <div v-else class="grid">
        <article v-for="item in items" :key="item.Id" class="poster" @click="openItem(item)">
          <img v-if="api.itemImageUrl(item)" :src="api.itemImageUrl(item)" :alt="item.Name" />
          <div v-else class="poster-fallback" :class="{ folder: item.IsFolder }">
            {{ item.IsFolder ? item.Type.slice(0, 2) : item.Name.slice(0, 2) }}
          </div>
          <h3>{{ item.Name }}</h3>
          <p>{{ itemSubtitle(item) }}</p>
        </article>
      </div>
    </section>

    <div v-if="selectedItem" class="dialog-backdrop" @click.self="selectedItem = null">
      <article class="dialog">
        <button class="close" type="button" title="关闭" @click="selectedItem = null">x</button>
        <div class="dialog-poster">
          <img v-if="api.itemImageUrl(selectedItem)" :src="api.itemImageUrl(selectedItem)" :alt="selectedItem.Name" />
          <div v-else class="poster-fallback">{{ selectedItem.Name.slice(0, 2) }}</div>
        </div>
        <div class="dialog-body">
          <p>{{ selectedItem.Type }}</p>
          <h2>{{ selectedItem.Name }}</h2>
          <div class="meta">
            <span v-if="selectedItem.ProductionYear">{{ selectedItem.ProductionYear }}</span>
            <span v-if="selectedItem.SeriesName">{{ selectedItem.SeriesName }}</span>
            <span v-if="selectedItem.SeasonName">{{ selectedItem.SeasonName }}</span>
            <span v-if="selectedMediaSource?.Container">{{ selectedMediaSource.Container }}</span>
            <span v-if="fileSize(selectedMediaSource?.Size)">{{ fileSize(selectedMediaSource?.Size) }}</span>
          </div>
          <div class="state-actions">
            <button type="button" :class="{ secondary: !selectedItem.UserData.IsFavorite }" @click="toggleFavorite(selectedItem)">
              {{ selectedItem.UserData.IsFavorite ? '取消收藏' : '收藏' }}
            </button>
            <button type="button" :class="{ secondary: !selectedItem.UserData.Played }" @click="togglePlayed(selectedItem)">
              {{ selectedItem.UserData.Played ? '标记未看' : '标记已看' }}
            </button>
          </div>
          <p v-if="selectedItem.Path" class="path">{{ selectedItem.Path }}</p>
          <video v-if="selectedItem.MediaType === 'Video'" controls :src="api.streamUrl(selectedItem)"></video>
          <div v-if="selectedStreams.length" class="streams">
            <div v-for="stream in selectedStreams" :key="`${stream.Type}-${stream.Index}`">
              <strong>{{ streamLabel(stream.Type) }} {{ stream.Index }}</strong>
              <span>{{ streamText(stream) }}</span>
            </div>
          </div>
          <a class="play-link" :href="api.streamUrl(selectedItem)" target="_blank" rel="noreferrer">打开直链</a>
        </div>
      </article>
    </div>
  </main>
</template>
