<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { EmbyApi } from './api/emby';
import type { BaseItemDto, UserDto } from './api/emby';

const api = new EmbyApi(import.meta.env.VITE_API_BASE || '');

const state = reactive({
  serverName: 'Movie Rust',
  username: '',
  password: '',
  adminName: 'admin',
  adminPassword: '',
  adminPasswordConfirm: '',
  adminCreated: false,
  uiCulture: 'zh-CN',
  metadataLanguage: 'zh',
  metadataCountry: 'CN',
  allowRemoteAccess: false,
  enableUPNP: false,
  showWizardPassword: false,
  showLoginPassword: false,
  libraryName: '电影',
  libraryPath: '',
  collectionType: 'movies',
  selectedLibraryId: '',
  search: '',
  busy: false,
  message: '',
  error: '',
  startupWizardCompleted: true,
  wizardStep: 1,
  showAddLibrary: false,
  loginAsOther: false
});

const user = ref(api.user);
const publicUsers = ref<UserDto[]>([]);
const libraries = ref<BaseItemDto[]>([]);
const items = ref<BaseItemDto[]>([]);
const homeItems = ref<BaseItemDto[]>([]);
const selectedItem = ref<BaseItemDto | null>(null);
const parentStack = ref<BaseItemDto[]>([]);

const selectedLibrary = computed(() =>
  libraries.value.find((library) => library.Id === state.selectedLibraryId)
);
const currentParentName = computed(() => parentStack.value.at(-1)?.Name || selectedLibrary.value?.Name || '首页');
const selectedMediaSource = computed(() => selectedItem.value?.MediaSources?.[0]);
const selectedStreams = computed(() => selectedMediaSource.value?.MediaStreams || selectedItem.value?.MediaStreams || []);
const currentItems = computed(() => (state.selectedLibraryId ? items.value : homeItems.value));
const continueWatching = computed(() =>
  homeItems.value.filter((item) => item.UserData?.PlaybackPositionTicks > 0 && !item.UserData?.Played).slice(0, 12)
);
const favorites = computed(() => homeItems.value.filter((item) => item.UserData?.IsFavorite).slice(0, 12));
const latest = computed(() => homeItems.value.filter((item) => !item.IsFolder).slice(0, 18));
const libraryCards = computed(() => libraries.value);

onMounted(async () => {
  await loadPublicInfo();
  if (!state.startupWizardCompleted) {
    await loadStartupWizard();
    return;
  }

  publicUsers.value = await safePublicUsers();
  if (api.isAuthenticated) {
    user.value = api.user;
    await enterHome();
  }
});

async function loadPublicInfo() {
  try {
    const info = await api.publicInfo();
    state.serverName = info.ServerName || state.serverName;
    state.startupWizardCompleted = info.StartupWizardCompleted;
  } catch {
    state.serverName = 'Movie Rust';
  }
}

async function safePublicUsers() {
  try {
    return await api.publicUsers();
  } catch {
    return [];
  }
}

async function loadStartupWizard() {
  await run(async () => {
    const configuration = await api.startupConfiguration();
    state.serverName = configuration.ServerName || state.serverName;
    state.uiCulture = configuration.UiCulture || state.uiCulture;
    state.metadataLanguage = configuration.PreferredMetadataLanguage || state.metadataLanguage;
    state.metadataCountry = configuration.MetadataCountryCode || state.metadataCountry;

    const firstUser = await api.firstStartupUser();
    if (firstUser) {
      state.adminName = firstUser.Name;
      state.adminCreated = true;
      state.wizardStep = Math.max(state.wizardStep, 3);
    }
  });
}

function startupConfigurationPayload() {
  return {
    ServerName: state.serverName,
    UiCulture: state.uiCulture,
    MetadataCountryCode: state.metadataCountry,
    PreferredMetadataLanguage: state.metadataLanguage
  };
}

async function saveLanguageAndContinue() {
  await run(async () => {
    await api.updateStartupConfiguration(startupConfigurationPayload());
    state.wizardStep = 2;
  });
}

async function createWizardAdmin() {
  await run(async () => {
    const adminName = state.adminName.trim();
    if (!adminName) {
      throw new Error('管理员名称不能为空');
    }

    if (!state.adminCreated) {
      if (state.adminPassword.length < 4) {
        throw new Error('管理员密码至少需要 4 个字符');
      }
      if (state.adminPassword !== state.adminPasswordConfirm) {
        throw new Error('两次输入的密码不一致');
      }

      await api.createFirstAdmin({
        Name: adminName,
        Password: state.adminPassword
      });
      state.adminCreated = true;
    }
    state.adminName = adminName;
    state.wizardStep = 3;
  }, '管理员已创建');
}

async function saveMetadataAndContinue() {
  await run(async () => {
    await api.updateStartupConfiguration(startupConfigurationPayload());
    state.wizardStep = 4;
  });
}

async function completeWizard() {
  await run(async () => {
    await api.updateRemoteAccess({
      EnableRemoteAccess: state.allowRemoteAccess,
      EnableAutomaticPortMapping: state.enableUPNP
    });
    await api.completeStartup();
    state.startupWizardCompleted = true;
    publicUsers.value = await safePublicUsers();

    if (state.adminPassword) {
      const result = await api.login(state.adminName.trim(), state.adminPassword);
      user.value = result.User;
      await enterHome();
    }
  }, state.adminPassword ? '设置完成' : '设置完成，请登录');
}

async function login(name = state.username, password = state.password) {
  await run(async () => {
    const result = await api.login(name, password);
    user.value = result.User;
    state.loginAsOther = false;
    await enterHome();
  }, '已登录');
}

async function enterHome() {
  state.selectedLibraryId = '';
  parentStack.value = [];
  await loadLibraries();
  await loadHome();
}

async function loadLibraries() {
  const result = await api.libraries();
  libraries.value = result.Items;
}

async function loadHome() {
  await run(async () => {
    const result = await api.items(undefined, state.search, true);
    homeItems.value = result.Items;
  });
}

async function loadItems() {
  if (!state.selectedLibraryId) {
    await loadHome();
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

async function backToHome() {
  state.selectedLibraryId = '';
  state.search = '';
  parentStack.value = [];
  await loadHome();
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
    state.showAddLibrary = false;
    await loadItems();
  }, '媒体库已创建');
}

async function scan() {
  await run(async () => {
    const summary = await api.scan();
    await loadLibraries();
    await loadItems();
    state.message = `扫描完成：${summary.ImportedItems} 个条目`;
  });
}

async function search() {
  if (state.selectedLibraryId) {
    await loadItems();
  } else {
    await loadHome();
  }
}

function logout() {
  api.logout();
  user.value = null;
  libraries.value = [];
  items.value = [];
  homeItems.value = [];
  selectedItem.value = null;
  state.username = '';
  state.password = '';
}

function openItem(item: BaseItemDto) {
  if (item.Type === 'CollectionFolder') {
    selectLibrary(item.Id);
    return;
  }

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
  for (const collection of [items.value, homeItems.value]) {
    const item = collection.find((candidate) => candidate.Id === itemId);
    if (item) {
      item.UserData = { ...item.UserData, ...userData };
    }
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

  return [item.ProductionYear, item.MediaSources?.[0]?.Container || item.Container || item.MediaType || item.Type]
    .filter(Boolean)
    .join(' · ');
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
  <main class="app-shell">
    <section v-if="!state.startupWizardCompleted" class="server-screen">
      <div class="server-card wizard-card">
        <div class="server-brand">
          <div class="mark">MR</div>
          <div>
            <p>Movie Rust</p>
            <h1>欢迎使用 {{ state.serverName }}</h1>
          </div>
        </div>
        <div class="steps" aria-label="首次启动向导">
          <span :class="{ active: state.wizardStep === 1, done: state.wizardStep > 1 }">语言</span>
          <span :class="{ active: state.wizardStep === 2, done: state.wizardStep > 2 }">管理员</span>
          <span :class="{ active: state.wizardStep === 3, done: state.wizardStep > 3 }">元数据</span>
          <span :class="{ active: state.wizardStep === 4 }">远程访问</span>
        </div>
        <div v-if="state.wizardStep === 1" class="wizard-pane">
          <h2>选择你的媒体服务器语言</h2>
          <p>当前界面使用简体中文，后端会按 Jellyfin/Emby 的 Startup 配置接口保存首选语言。</p>
          <label>
            显示语言
            <select v-model="state.uiCulture">
              <option value="zh-CN">简体中文</option>
              <option value="en-US">English</option>
            </select>
          </label>
          <button :disabled="state.busy" type="button" @click="saveLanguageAndContinue">继续</button>
        </div>
        <form v-else-if="state.wizardStep === 2" class="wizard-pane form-stack" @submit.prevent="createWizardAdmin">
          <h2>创建管理员账户</h2>
          <p v-if="state.adminCreated">管理员账户已创建，可以继续设置元数据。</p>
          <label>
            用户名
            <input v-model="state.adminName" autocomplete="username" :disabled="state.adminCreated" />
          </label>
          <label>
            密码
            <div class="password-field">
              <input v-model="state.adminPassword" :type="state.showWizardPassword ? 'text' : 'password'" autocomplete="new-password" :disabled="state.adminCreated" />
              <button type="button" :title="state.showWizardPassword ? '隐藏密码' : '显示密码'" @click="state.showWizardPassword = !state.showWizardPassword">
                {{ state.showWizardPassword ? '◐' : '●' }}
              </button>
            </div>
          </label>
          <label>
            确认密码
            <input v-model="state.adminPasswordConfirm" :type="state.showWizardPassword ? 'text' : 'password'" autocomplete="new-password" :disabled="state.adminCreated" />
          </label>
          <div class="button-row">
            <button class="secondary" type="button" @click="state.wizardStep = 1">返回</button>
            <button :disabled="state.busy" type="submit">继续</button>
          </div>
        </form>
        <form v-else-if="state.wizardStep === 3" class="wizard-pane form-stack" @submit.prevent="saveMetadataAndContinue">
          <h2>首选元数据语言</h2>
          <p>这一步对应 Jellyfin 的元数据语言配置，后续扫描和识别媒体时会沿用这些首选项。</p>
          <label>
            元数据语言
            <select v-model="state.metadataLanguage">
              <option value="zh">中文</option>
              <option value="en">English</option>
              <option value="ja">日本語</option>
              <option value="ko">한국어</option>
            </select>
          </label>
          <label>
            元数据国家/地区
            <select v-model="state.metadataCountry">
              <option value="CN">中国</option>
              <option value="US">United States</option>
              <option value="JP">日本</option>
              <option value="KR">韩国</option>
            </select>
          </label>
          <div class="button-row">
            <button class="secondary" type="button" @click="state.wizardStep = 2">返回</button>
            <button :disabled="state.busy" type="submit">继续</button>
          </div>
        </form>
        <div v-else class="wizard-pane">
          <h2>远程访问</h2>
          <p>保留 Jellyfin/Emby 的远程访问设置入口。当前版本会保存选择，实际端口映射可以在部署层继续配置。</p>
          <label class="check-row">
            <input v-model="state.allowRemoteAccess" type="checkbox" />
            允许远程连接到服务器
          </label>
          <label class="check-row">
            <input v-model="state.enableUPNP" type="checkbox" />
            自动端口映射
          </label>
          <div class="button-row">
            <button class="secondary" type="button" @click="state.wizardStep = 3">返回</button>
            <button :disabled="state.busy" type="button" @click="completeWizard">完成设置</button>
          </div>
        </div>
        <p v-if="state.error" class="notice error">{{ state.error }}</p>
      </div>
    </section>

    <section v-else-if="!user" class="server-screen">
      <div class="server-card">
        <div class="server-brand centered">
          <div class="mark">MR</div>
          <h1>{{ state.serverName }}</h1>
        </div>
        <div v-if="publicUsers.length && !state.loginAsOther" class="user-picker">
          <h2>选择用户</h2>
          <div class="user-grid">
            <button v-for="publicUser in publicUsers" :key="publicUser.Id" type="button" @click="state.username = publicUser.Name; state.loginAsOther = true">
              <span>{{ publicUser.Name.slice(0, 1).toUpperCase() }}</span>
              {{ publicUser.Name }}
            </button>
          </div>
          <button class="secondary" type="button" @click="state.loginAsOther = true">手动登录</button>
        </div>
        <form v-else class="form-stack" @submit.prevent="login()">
          <h2>登录</h2>
          <label>
            用户名
            <input v-model="state.username" autocomplete="username" />
          </label>
          <label>
            密码
            <div class="password-field">
              <input v-model="state.password" :type="state.showLoginPassword ? 'text' : 'password'" autocomplete="current-password" />
              <button type="button" :title="state.showLoginPassword ? '隐藏密码' : '显示密码'" @click="state.showLoginPassword = !state.showLoginPassword">
                {{ state.showLoginPassword ? '◐' : '●' }}
              </button>
            </div>
          </label>
          <div class="button-row">
            <button v-if="publicUsers.length" class="secondary" type="button" @click="state.loginAsOther = false">返回</button>
            <button :disabled="state.busy" type="submit">登录</button>
          </div>
        </form>
        <p v-if="state.error" class="notice error">{{ state.error }}</p>
      </div>
    </section>

    <template v-else>
      <aside class="nav-drawer">
        <div class="brand">
          <div class="mark">MR</div>
          <div>
            <h1>{{ state.serverName }}</h1>
            <p>{{ user.Name }}</p>
          </div>
        </div>
        <nav class="nav-list">
          <button :class="{ active: !state.selectedLibraryId }" type="button" @click="backToHome">⌂ 首页</button>
          <button
            v-for="library in libraries"
            :key="library.Id"
            :class="{ active: library.Id === state.selectedLibraryId }"
            type="button"
            @click="selectLibrary(library.Id)"
          >
            <span>{{ library.CollectionType === 'tvshows' ? '▤' : '▥' }} {{ library.Name }}</span>
            <small>{{ library.ChildCount || 0 }}</small>
          </button>
        </nav>
        <div class="drawer-actions">
          <button type="button" @click="state.showAddLibrary = true">＋ 媒体库</button>
          <button class="secondary" type="button" @click="logout">退出</button>
        </div>
      </aside>

      <section class="main-view">
        <header class="top-bar">
          <div>
            <p>{{ selectedLibrary?.CollectionType || 'home' }}</p>
            <h2>{{ currentParentName }}</h2>
          </div>
          <form class="search" @submit.prevent="search">
            <input v-model="state.search" placeholder="搜索媒体" />
            <button :disabled="state.busy" type="submit">搜索</button>
          </form>
          <button class="icon-button" :disabled="state.busy" type="button" title="扫描媒体库" @click="scan">↻</button>
        </header>

        <div v-if="parentStack.length" class="crumbs">
          <button type="button" title="返回上级" @click="backToParent">‹</button>
          <span>{{ selectedLibrary?.Name }}</span>
          <span v-for="parent in parentStack" :key="parent.Id">/ {{ parent.Name }}</span>
        </div>

        <p v-if="state.error" class="notice error">{{ state.error }}</p>
        <p v-else-if="state.message" class="notice">{{ state.message }}</p>

        <section v-if="!state.selectedLibraryId" class="home-sections">
          <div class="hero-strip" v-if="latest[0]">
            <img v-if="api.itemImageUrl(latest[0])" :src="api.itemImageUrl(latest[0])" :alt="latest[0].Name" />
            <div>
              <p>最近添加</p>
              <h2>{{ latest[0].Name }}</h2>
              <button type="button" @click="openItem(latest[0])">播放 / 详情</button>
            </div>
          </div>
          <section class="media-row">
            <div class="section-heading">
              <h3>媒体库</h3>
              <span>{{ libraryCards.length }}</span>
            </div>
            <div class="rail">
              <article v-for="item in libraryCards" :key="item.Id" class="library-tile" @click="openItem(item)">
                <div class="library-icon">{{ item.CollectionType === 'tvshows' ? '▤' : '▥' }}</div>
                <h4>{{ item.Name }}</h4>
                <p>{{ item.ChildCount || 0 }} 个条目</p>
              </article>
            </div>
          </section>

          <section v-if="continueWatching.length" class="media-row">
            <div class="section-heading">
              <h3>继续观看</h3>
              <span>{{ continueWatching.length }}</span>
            </div>
            <div class="rail poster-rail">
              <article v-for="item in continueWatching" :key="item.Id" class="poster-card" @click="openItem(item)">
                <div class="poster-art thumb">
                  <img v-if="api.itemImageUrl(item)" :src="api.itemImageUrl(item)" :alt="item.Name" />
                  <div v-else class="poster-fallback">{{ item.Name.slice(0, 2) }}</div>
                  <button class="play-fab" type="button" @click.stop="openItem(item)">▶</button>
                </div>
                <h3>{{ item.Name }}</h3>
                <p>{{ itemSubtitle(item) }}</p>
              </article>
            </div>
          </section>

          <section v-if="favorites.length" class="media-row">
            <div class="section-heading">
              <h3>收藏</h3>
              <span>{{ favorites.length }}</span>
            </div>
            <div class="rail poster-rail">
              <article v-for="item in favorites" :key="item.Id" class="poster-card" @click="openItem(item)">
                <div class="poster-art">
                  <img v-if="api.itemImageUrl(item)" :src="api.itemImageUrl(item)" :alt="item.Name" />
                  <div v-else class="poster-fallback">{{ item.Name.slice(0, 2) }}</div>
                  <span class="favorite">♥</span>
                  <button class="play-fab" type="button" @click.stop="openItem(item)">▶</button>
                </div>
                <h3>{{ item.Name }}</h3>
                <p>{{ itemSubtitle(item) }}</p>
              </article>
            </div>
          </section>

          <section class="media-row">
            <div class="section-heading">
              <h3>最新媒体</h3>
              <span>{{ latest.length }}</span>
            </div>
            <div class="rail poster-rail">
              <article v-for="item in latest" :key="item.Id" class="poster-card" @click="openItem(item)">
                <div class="poster-art">
                  <img v-if="api.itemImageUrl(item)" :src="api.itemImageUrl(item)" :alt="item.Name" />
                  <div v-else class="poster-fallback">{{ item.Name.slice(0, 2) }}</div>
                  <span v-if="item.UserData?.Played" class="watched">✓</span>
                  <button class="play-fab" type="button" @click.stop="openItem(item)">▶</button>
                </div>
                <h3>{{ item.Name }}</h3>
                <p>{{ itemSubtitle(item) }}</p>
              </article>
            </div>
          </section>
        </section>

        <section v-else>
          <div v-if="currentItems.length === 0" class="empty">
            <h2>这里还没有媒体</h2>
            <p>添加容器内媒体路径后点击扫描，条目就会显示在这里。</p>
            <button type="button" @click="state.showAddLibrary = true">添加媒体库</button>
          </div>
          <div v-else class="poster-grid">
            <article v-for="item in currentItems" :key="item.Id" class="poster-card" @click="openItem(item)">
              <div class="poster-art">
                <img v-if="api.itemImageUrl(item)" :src="api.itemImageUrl(item)" :alt="item.Name" />
                <div v-else class="poster-fallback" :class="{ folder: item.IsFolder }">
                  {{ item.IsFolder ? item.Type.slice(0, 2) : item.Name.slice(0, 2) }}
                </div>
                <span v-if="item.UserData?.Played" class="watched">✓</span>
                <button v-if="!item.IsFolder" class="play-fab" type="button" @click.stop="openItem(item)">▶</button>
              </div>
              <h3>{{ item.Name }}</h3>
              <p>{{ itemSubtitle(item) }}</p>
            </article>
          </div>
        </section>
      </section>

      <div v-if="state.showAddLibrary" class="dialog-backdrop" @click.self="state.showAddLibrary = false">
        <form class="small-dialog form-stack" @submit.prevent="createLibrary">
          <button class="close" type="button" title="关闭" @click="state.showAddLibrary = false">×</button>
          <h2>添加媒体库</h2>
          <label>
            名称
            <input v-model="state.libraryName" placeholder="电影" />
          </label>
          <label>
            路径
            <input v-model="state.libraryPath" placeholder="容器内路径，例如 /media/movies" />
          </label>
          <label>
            类型
            <select v-model="state.collectionType">
              <option value="movies">电影</option>
              <option value="tvshows">剧集</option>
              <option value="music">音乐</option>
            </select>
          </label>
          <button :disabled="state.busy || !state.libraryPath" type="submit">添加</button>
        </form>
      </div>

      <div v-if="selectedItem" class="dialog-backdrop detail-backdrop" @click.self="selectedItem = null">
        <article class="detail-dialog">
          <button class="close" type="button" title="关闭" @click="selectedItem = null">×</button>
          <div class="detail-hero">
            <img v-if="api.itemImageUrl(selectedItem)" :src="api.itemImageUrl(selectedItem)" :alt="selectedItem.Name" />
            <div class="detail-copy">
              <p>{{ selectedItem.Type }}</p>
              <h2>{{ selectedItem.Name }}</h2>
              <div class="meta">
                <span v-if="selectedItem.ProductionYear">{{ selectedItem.ProductionYear }}</span>
                <span v-if="selectedItem.SeriesName">{{ selectedItem.SeriesName }}</span>
                <span v-if="selectedItem.SeasonName">{{ selectedItem.SeasonName }}</span>
                <span v-if="selectedMediaSource?.Container">{{ selectedMediaSource.Container }}</span>
                <span v-if="fileSize(selectedMediaSource?.Size)">{{ fileSize(selectedMediaSource?.Size) }}</span>
              </div>
              <div class="button-row">
                <a class="play-link" :href="api.streamUrl(selectedItem)" target="_blank" rel="noreferrer">▶ 播放</a>
                <button type="button" :class="{ secondary: !selectedItem.UserData.IsFavorite }" @click="toggleFavorite(selectedItem)">
                  {{ selectedItem.UserData.IsFavorite ? '♥ 已收藏' : '♡ 收藏' }}
                </button>
                <button type="button" :class="{ secondary: !selectedItem.UserData.Played }" @click="togglePlayed(selectedItem)">
                  {{ selectedItem.UserData.Played ? '标记未看' : '✓ 标记已看' }}
                </button>
              </div>
            </div>
          </div>
          <video v-if="selectedItem.MediaType === 'Video'" controls :src="api.streamUrl(selectedItem)"></video>
          <p v-if="selectedItem.Path" class="path">{{ selectedItem.Path }}</p>
          <div v-if="selectedStreams.length" class="streams">
            <div v-for="stream in selectedStreams" :key="`${stream.Type}-${stream.Index}`">
              <strong>{{ streamLabel(stream.Type) }} {{ stream.Index }}</strong>
              <span>{{ streamText(stream) }}</span>
            </div>
          </div>
        </article>
      </div>
    </template>
  </main>
</template>
