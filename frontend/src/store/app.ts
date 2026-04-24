import { computed, reactive, ref } from 'vue';
import { EmbyApi } from '../api/emby';
import type {
  BaseItemDto,
  CreateLibraryPayload,
  LocalizationCulture,
  LibraryOptions,
  SystemInfo,
  UserDto,
  VirtualFolderInfo
} from '../api/emby';

export type AdminPage = 'overview' | 'server' | 'libraries' | 'users' | 'playback' | 'network';

export interface ServerEntry {
  Id: string;
  Name: string;
  Url: string;
  Version?: string;
  ProductName?: string;
  LastConnected?: string;
}

export const api = new EmbyApi(import.meta.env.VITE_API_BASE || '');
api.onUnauthorized = handleUnauthorized;

/** 后端若缺字段或解析异常时避免 `undefined` 写入 ref，防止首页模板访问 `.length` 直接崩溃 */
function itemsFromQuery<T>(result: { Items?: T[] | null }): T[] {
  return result.Items ?? [];
}

const SERVERS_KEY = 'movie-rust-servers';
const CURRENT_SERVER_KEY = 'movie-rust-current-server';

export const state = reactive({
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
  libraryScanThreadCount: 2,
  strmAnalysisThreadCount: 8,
  tmdbMetadataThreadCount: 4,
  allowRemoteAccess: false,
  enableUPNP: false,
  showWizardPassword: false,
  showLoginPassword: false,
  libraryName: '电影',
  libraryPath: '',
  collectionType: 'movies',
  selectedLibraryId: '',
  libraryViewType: '',
  librarySortBy: 'SortName',
  librarySortAscending: true,
  libraryGenres: [] as string[],
  libraryYears: [] as number[],
  libraryFavoritesOnly: false,
  libraryOnly4K: false,
  libraryOnlyHDR: false,
  librarySubtitlesOnly: false,
  search: '',
  busy: false,
  message: '',
  error: '',
  startupWizardCompleted: true,
  wizardStep: 1,
  showAddLibrary: false,
  editingLibrary: null as VirtualFolderInfo | null,
  loginAsOther: false,
  initialized: false
});

export const servers = ref<ServerEntry[]>(readJson<ServerEntry[]>(SERVERS_KEY) || []);
export const currentServerUrl = ref(readText(CURRENT_SERVER_KEY) || defaultServerUrl());
export const user = ref(api.user);
export const publicUsers = ref<UserDto[]>([]);
export const adminUsers = ref<UserDto[]>([]);
export const metadataCultures = ref<LocalizationCulture[]>([]);
export const libraries = ref<BaseItemDto[]>([]);
export const virtualFolders = ref<VirtualFolderInfo[]>([]);
export const items = ref<BaseItemDto[]>([]);
export const homeItems = ref<BaseItemDto[]>([]);
export const recentlyAddedTitles = ref<BaseItemDto[]>([]);
export const latestByLibrary = ref<Record<string, BaseItemDto[]>>({});
export const systemInfo = ref<SystemInfo | null>(null);
export const selectedItem = ref<BaseItemDto | null>(null);
export const parentStack = ref<BaseItemDto[]>([]);

export const isAdmin = computed(() => Boolean(user.value?.Policy?.IsAdministrator));
export const currentServer = computed(
  () =>
    servers.value.find(
      (server) => normalizeServerUrl(server.Url) === normalizeServerUrl(currentServerUrl.value)
    ) || null
);
export const selectedLibrary = computed(() =>
  libraries.value.find((library) => library.Id === state.selectedLibraryId)
);
export const selectedMediaSource = computed(() => selectedItem.value?.MediaSources?.[0]);
export const selectedStreams = computed(
  () => selectedMediaSource.value?.MediaStreams || selectedItem.value?.MediaStreams || []
);
export const currentItems = computed(() => (state.selectedLibraryId ? items.value : homeItems.value));
export const currentParentName = computed(
  () => parentStack.value.at(-1)?.Name || selectedLibrary.value?.Name || '首页'
);
export const continueWatching = computed(() =>
  homeItems.value
    .filter((item) => item.UserData?.PlaybackPositionTicks > 0 && !item.UserData?.Played)
    .slice(0, 12)
);
export const favorites = computed(() =>
  homeItems.value.filter((item) => item.UserData?.IsFavorite).slice(0, 12)
);
export const latest = computed(() => recentlyAddedTitles.value.slice(0, 18));
export const libraryCards = computed(() => libraries.value);
export const totalLibraryItems = computed(() =>
  libraries.value.reduce((sum, library) => sum + (library.ChildCount || 0), 0)
);

export async function initialize(force = false) {
  if (state.initialized && !force) {
    return;
  }

  state.initialized = false;
  try {
    ensureDefaultServer();
    api.setBaseUrl(currentServerUrl.value);
    await loadPublicInfo();

    if (!state.startupWizardCompleted) {
      await loadStartupWizard();
      return;
    }

    publicUsers.value = await safePublicUsers();
    if (api.isAuthenticated) {
      user.value = api.user;
      try {
        await enterHome();
      } catch (error) {
        if (!api.isAuthenticated) {
          handleUnauthorized();
        }
        state.error = error instanceof Error ? error.message : String(error);
      }
    }
  } finally {
    state.initialized = true;
  }
}

export async function loadPublicInfo() {
  try {
    const info = await api.publicInfo();
    state.serverName = info.ServerName || state.serverName;
    state.startupWizardCompleted = info.StartupWizardCompleted;
    upsertServer({
      Id: info.Id || normalizeServerUrl(currentServerUrl.value),
      Name: info.ServerName || currentServer.value?.Name || 'Movie Rust',
      Url: normalizeServerUrl(currentServerUrl.value),
      Version: info.Version,
      ProductName: info.ProductName,
      LastConnected: new Date().toISOString()
    });
  } catch {
    state.serverName = 'Movie Rust';
  }
}

export async function safePublicUsers() {
  try {
    return await api.publicUsers();
  } catch {
    return [];
  }
}

export async function loadStartupWizard() {
  await run(async () => {
    const configuration = await api.startupConfiguration();
    state.serverName = configuration.ServerName || state.serverName;
    state.uiCulture = configuration.UiCulture || state.uiCulture;
    state.metadataLanguage = configuration.PreferredMetadataLanguage || state.metadataLanguage;
    state.metadataCountry = configuration.MetadataCountryCode || state.metadataCountry;
    state.libraryScanThreadCount =
      configuration.LibraryScanThreadCount || state.libraryScanThreadCount;
    state.strmAnalysisThreadCount =
      configuration.StrmAnalysisThreadCount || state.strmAnalysisThreadCount;
    state.tmdbMetadataThreadCount =
      configuration.TmdbMetadataThreadCount || state.tmdbMetadataThreadCount;

    const firstUser = await api.firstStartupUser();
    if (firstUser) {
      state.adminName = firstUser.Name;
      state.adminCreated = true;
      state.wizardStep = Math.max(state.wizardStep, 3);
    }
  });
}

export function startupConfigurationPayload() {
  return {
    ServerName: state.serverName,
    UiCulture: state.uiCulture,
    MetadataCountryCode: state.metadataCountry,
    PreferredMetadataLanguage: state.metadataLanguage,
    LibraryScanThreadCount: Number(state.libraryScanThreadCount) || 2,
    StrmAnalysisThreadCount: Number(state.strmAnalysisThreadCount) || 8,
    TmdbMetadataThreadCount: Number(state.tmdbMetadataThreadCount) || 4
  };
}

export async function saveLanguageAndContinue() {
  await run(async () => {
    await api.updateStartupConfiguration(startupConfigurationPayload());
    state.wizardStep = 2;
  });
}

export async function createWizardAdmin() {
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
      const result = await api.login(adminName, state.adminPassword);
      user.value = result.User;
    }

    state.adminName = adminName;
    state.wizardStep = 3;
  }, '管理员已创建');
}

export async function saveMetadataAndContinue() {
  await run(async () => {
    await api.updateStartupConfiguration(startupConfigurationPayload());
    state.wizardStep = 4;
  });
}

export async function completeWizard() {
  const wizardCompleted = await run(async () => {
    await api.updateRemoteAccess({
      EnableRemoteAccess: state.allowRemoteAccess,
      EnableAutomaticPortMapping: state.enableUPNP
    });
    await api.completeStartup();
    state.startupWizardCompleted = true;
    publicUsers.value = await safePublicUsers();

    if (!user.value && state.adminPassword) {
      const result = await api.login(state.adminName.trim(), state.adminPassword);
      user.value = result.User;
    }
  }, state.adminPassword ? '设置完成' : '设置完成，请登录');

  if (!wizardCompleted) {
    return;
  }

  // enterHome 不包在 run 内：避免与「设置完成」同一次 catch 混在一起，且确保导航到首页前数据已拉取
  if (user.value) {
    try {
      await enterHome();
    } catch (error) {
      state.error = error instanceof Error ? error.message : String(error);
    }
  }
}

export async function login(name = state.username, password = state.password) {
  await run(async () => {
    const result = await api.login(name, password);
    user.value = result.User;
    state.loginAsOther = false;
    await enterHome();
  }, '已登录');
}

export async function enterHome() {
  state.selectedLibraryId = '';
  parentStack.value = [];
  await loadLibraries();
  await Promise.all([loadHome(), loadRecentlyAddedTitles(), loadLatestByLibrary()]);
}

export async function addServer(url: string) {
  const normalized = normalizeServerUrl(url);
  const info = await api.publicInfoAt(normalized);
  upsertServer({
    Id: info.Id || normalized,
    Name: info.ServerName || normalized,
    Url: normalized,
    Version: info.Version,
    ProductName: info.ProductName,
    LastConnected: new Date().toISOString()
  });
  await switchServer(normalized);
}

export async function switchServer(url: string) {
  currentServerUrl.value = normalizeServerUrl(url);
  localStorage.setItem(CURRENT_SERVER_KEY, currentServerUrl.value);
  api.logout();
  clearClientState(false);
  api.setBaseUrl(currentServerUrl.value);
  await initialize(true);
}

export function removeServer(url: string) {
  const normalized = normalizeServerUrl(url);
  servers.value = servers.value.filter((server) => normalizeServerUrl(server.Url) !== normalized);
  persistServers();

  if (normalizeServerUrl(currentServerUrl.value) === normalized) {
    const fallback = servers.value[0]?.Url || defaultServerUrl();
    currentServerUrl.value = fallback;
    localStorage.setItem(CURRENT_SERVER_KEY, fallback);
  }
}

export async function loadLibraries() {
  const result = await api.libraries();
  libraries.value = itemsFromQuery(result);
}

export async function loadVirtualFolders() {
  if (!isAdmin.value) {
    virtualFolders.value = [];
    return;
  }

  virtualFolders.value = await api.virtualFolders();
}

export async function loadHome() {
  await run(async () => {
    const searching = Boolean(state.search.trim());
    const result = await api.items(undefined, state.search, true, {
      sortBy: searching ? 'SortName' : 'DateCreated',
      sortOrder: searching ? 'Ascending' : 'Descending',
      limit: 180
    });
    homeItems.value = itemsFromQuery(result);
  }, '', { rethrow: true });
}

export async function loadLatestByLibrary() {
  const entries = await Promise.all(
    libraries.value.map(async (library) => [library.Id, await loadLibraryHomeItems(library)] as const)
  );
  latestByLibrary.value = Object.fromEntries(entries);
}

export async function loadRecentlyAddedTitles() {
  const entries = await Promise.all(
    libraries.value.map(async (library) => {
      if (library.CollectionType === 'tvshows') {
        const result = await api.items(library.Id, '', false, {
          includeTypes: ['Series'],
          sortBy: 'DateCreated',
          sortOrder: 'Descending',
          limit: 36
        });
        return itemsFromQuery(result);
      }

      if (library.CollectionType === 'movies') {
        const result = await api.items(library.Id, '', false, {
          includeTypes: ['Movie'],
          sortBy: 'DateCreated',
          sortOrder: 'Descending',
          limit: 36
        });
        return itemsFromQuery(result);
      }

      return [] as BaseItemDto[];
    })
  );

  recentlyAddedTitles.value = dedupeItemsById(entries.flat())
    .sort(compareDateCreatedDesc)
    .slice(0, 36);
}

async function loadLibraryHomeItems(library: BaseItemDto) {
  if (library.CollectionType === 'tvshows') {
    const result = await api.items(library.Id, '', false, {
      includeTypes: ['Series'],
      sortBy: 'SortName',
      sortOrder: 'Ascending',
      limit: 36
    });
    return itemsFromQuery(result);
  }

  if (library.CollectionType === 'movies') {
    const result = await api.items(library.Id, '', false, {
      includeTypes: ['Movie'],
      sortBy: 'DateCreated',
      sortOrder: 'Descending',
      limit: 36
    });
    return itemsFromQuery(result);
  }

  if (library.CollectionType === 'music') {
    const result = await api.items(library.Id, '', true, {
      includeTypes: ['Audio'],
      sortBy: 'DateCreated',
      sortOrder: 'Descending',
      limit: 36
    });
    return itemsFromQuery(result);
  }

  return api.latest(library.Id, 18);
}

export async function loadItems() {
  if (!state.selectedLibraryId) {
    await loadHome();
    return;
  }

  await run(async () => {
    const parentId = parentStack.value.at(-1)?.Id || state.selectedLibraryId;
    const videoTypes: string[] = [];
    if (state.libraryOnly4K) videoTypes.push('Video4K');
    const result = await api.items(parentId, state.search, Boolean(state.search.trim()), {
      includeTypes: state.libraryViewType ? [state.libraryViewType] : undefined,
      genres: state.libraryGenres.length ? state.libraryGenres : undefined,
      years: state.libraryYears.length ? state.libraryYears : undefined,
      isFavorite: state.libraryFavoritesOnly || undefined,
      videoTypes: videoTypes.length ? videoTypes : undefined,
      hasSubtitles: state.librarySubtitlesOnly ? true : undefined,
      sortBy: state.librarySortBy || 'SortName',
      sortOrder: state.librarySortAscending ? 'Ascending' : 'Descending',
      limit: 180,
      fields: ['MediaStreams', 'MediaSources', 'ChildCount', 'Overview']
    });
    let list = itemsFromQuery(result);
    if (state.libraryOnlyHDR) {
      list = list.filter((item) => {
        const vs = item.MediaStreams?.find((s) => s.Type === 'Video') ||
          item.MediaSources?.[0]?.MediaStreams?.find((s) => s.Type === 'Video');
        const vr = (vs as unknown as { VideoRange?: string } | undefined)?.VideoRange || '';
        return /HDR|DOVI|DOLBY/i.test(vr);
      });
    }
    items.value = list;
  });
}

export async function selectLibrary(libraryId: string) {
  state.selectedLibraryId = libraryId;
  state.search = '';
  parentStack.value = [];
  await loadItems();
}

export const libraryGenresCache = ref<Record<string, string[]>>({});

export async function loadLibraryGenres(libraryId: string) {
  if (libraryGenresCache.value[libraryId]) return libraryGenresCache.value[libraryId];
  try {
    const res = await api.genres(libraryId);
    const names = itemsFromQuery(res)
      .map((g) => g.Name)
      .filter(Boolean);
    libraryGenresCache.value[libraryId] = names;
    return names;
  } catch {
    libraryGenresCache.value[libraryId] = [];
    return [];
  }
}

export function resetLibraryFilters() {
  state.libraryGenres = [];
  state.libraryYears = [];
  state.libraryFavoritesOnly = false;
  state.libraryOnly4K = false;
  state.libraryOnlyHDR = false;
  state.librarySubtitlesOnly = false;
}

export async function backToHome() {
  state.selectedLibraryId = '';
  state.search = '';
  parentStack.value = [];
  await Promise.all([loadHome(), loadRecentlyAddedTitles(), loadLatestByLibrary()]);
}

export async function loadAdminData() {
  await run(async () => {
    const [info, users, configuration, cultures] = await Promise.all([
      api.systemInfo(),
      api.users(),
      api.startupConfiguration(),
      api.localizationCultures().catch(() => [])
    ]);
    systemInfo.value = info;
    adminUsers.value = users;
    metadataCultures.value = cultures;
    state.serverName = configuration.ServerName || state.serverName;
    state.uiCulture = configuration.UiCulture || state.uiCulture;
    state.metadataLanguage = configuration.PreferredMetadataLanguage || state.metadataLanguage;
    state.metadataCountry = configuration.MetadataCountryCode || state.metadataCountry;
    state.libraryScanThreadCount =
      configuration.LibraryScanThreadCount || state.libraryScanThreadCount;
    state.strmAnalysisThreadCount =
      configuration.StrmAnalysisThreadCount || state.strmAnalysisThreadCount;
    state.tmdbMetadataThreadCount =
      configuration.TmdbMetadataThreadCount || state.tmdbMetadataThreadCount;
    await loadVirtualFolders();
  });
}

export async function backToParent() {
  parentStack.value.pop();
  await loadItems();
}

export function defaultLibraryOptions(paths: string[] = []): LibraryOptions {
  return {
    Enabled: true,
    EnablePhotos: true,
    EnableInternetProviders: true,
    DownloadImagesInAdvance: false,
    EnableRealtimeMonitor: false,
    ExcludeFromSearch: false,
    IgnoreHiddenFiles: true,
    EnableChapterImageExtraction: false,
    ExtractChapterImagesDuringLibraryScan: false,
    SaveLocalMetadata: true,
    SaveMetadataHidden: false,
    MergeTopLevelFolders: false,
    PlaceholderMetadataRefreshIntervalDays: 0,
    ImportMissingEpisodes: false,
    EnableAutomaticSeriesGrouping: true,
    EnableEmbeddedTitles: false,
    EnableEmbeddedEpisodeInfos: true,
    EnableMultiVersionByFiles: true,
    EnableMultiVersionByMetadata: false,
    EnableMultiPartItems: true,
    AutomaticRefreshIntervalDays: 0,
    PreferredMetadataLanguage: state.metadataLanguage || 'zh',
    PreferredImageLanguage: state.metadataLanguage || 'zh',
    MetadataCountryCode: state.metadataCountry || 'CN',
    SeasonZeroDisplayName: 'Specials',
    MetadataSavers: ['Nfo'],
    ImportCollections: true,
    MinCollectionItems: 2,
    DisabledLocalMetadataReaders: [],
    LocalMetadataReaderOrder: ['Nfo'],
    PathInfos: paths.filter(Boolean).map((path) => ({ Path: path }))
  };
}

export function libraryPayloadFromState(): CreateLibraryPayload {
  const paths = state.libraryPath
    .split(/\r?\n|,/)
    .map((path) => path.trim())
    .filter(Boolean);

  return {
    Name: state.libraryName.trim(),
    CollectionType: state.collectionType,
    Path: paths[0] || '',
    Paths: paths,
    LibraryOptions: defaultLibraryOptions(paths)
  };
}

export async function createLibrary(payload = libraryPayloadFromState()) {
  await run(async () => {
    const library = await api.createLibrary(payload, true);
    libraries.value.push(library);
    state.libraryPath = '';
    state.showAddLibrary = false;
    await loadLibraries();
    await loadVirtualFolders();
    await Promise.all([loadHome(), loadRecentlyAddedTitles(), loadLatestByLibrary()]);
    state.selectedLibraryId = library.Id;
    await loadItems();
  }, '媒体库已创建');
}

export async function deleteLibrary(library: BaseItemDto | VirtualFolderInfo) {
  await run(async () => {
    if ('ItemId' in library) {
      await api.deleteVirtualFolder(library.Name, true);
      if (state.selectedLibraryId === library.ItemId) {
        state.selectedLibraryId = '';
      }
    } else {
      await api.deleteLibrary(library.Id, true);
      if (state.selectedLibraryId === library.Id) {
        state.selectedLibraryId = '';
      }
    }

    await loadLibraries();
    await loadVirtualFolders();
    await Promise.all([loadHome(), loadRecentlyAddedTitles(), loadLatestByLibrary()]);
    await loadItems();
  }, '媒体库已删除');
}

export async function saveServerSettings() {
  await run(async () => {
    await api.updateStartupConfiguration(startupConfigurationPayload());
    await api.updateRemoteAccess({
      EnableRemoteAccess: state.allowRemoteAccess,
      EnableAutomaticPortMapping: state.enableUPNP
    });
    systemInfo.value = await api.systemInfo();
  }, '服务器设置已保存');
}

export async function scan() {
  await run(async () => {
    const summary = await api.scan();
    await loadLibraries();
    await loadVirtualFolders();
    await loadRecentlyAddedTitles();
    await loadLatestByLibrary();
    await loadItems();
    state.message = `扫描完成，新增 ${summary.ImportedItems} 个条目`;
  });
}

export async function search() {
  if (state.selectedLibraryId) {
    await loadItems();
  } else {
    await loadHome();
  }
}

export function logout() {
  api.logout();
  clearClientState(true);
  void refreshPublicUsersAfterLogout();
}

function handleUnauthorized() {
  clearClientState(true);
  void refreshPublicUsersAfterLogout();
}

function clearClientState(keepInitialized: boolean) {
  user.value = null;
  systemInfo.value = null;
  publicUsers.value = [];
  libraries.value = [];
  items.value = [];
  homeItems.value = [];
  recentlyAddedTitles.value = [];
  latestByLibrary.value = {};
  adminUsers.value = [];
  virtualFolders.value = [];
  selectedItem.value = null;
  parentStack.value = [];
  state.username = '';
  state.password = '';
  state.search = '';
  state.selectedLibraryId = '';
  state.libraryViewType = '';
  state.librarySortBy = 'SortName';
  state.librarySortAscending = true;
  state.message = '';
  state.error = '';
  state.loginAsOther = false;
  state.initialized = keepInitialized;
}

async function refreshPublicUsersAfterLogout() {
  if (!state.startupWizardCompleted) {
    return;
  }

  publicUsers.value = await safePublicUsers();
}

export function openItem(item: BaseItemDto) {
  if (item.Type === 'CollectionFolder') {
    selectLibrary(item.Id);
    return;
  }

  if (item.IsFolder) {
    parentStack.value.push(item);
    void loadItems();
    return;
  }

  selectedItem.value = item;
}

export async function toggleFavorite(item: BaseItemDto) {
  await run(async () => {
    const userData = await api.markFavorite(item.Id, !item.UserData.IsFavorite);
    applyUserData(item.Id, userData);
  });
}

export async function togglePlayed(item: BaseItemDto) {
  await run(async () => {
    const userData = await api.markPlayed(item.Id, !item.UserData.Played);
    applyUserData(item.Id, userData);
  });
}

export function applyUserData(itemId: string, userData: BaseItemDto['UserData']) {
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

export function itemSubtitle(item: BaseItemDto) {
  if (item.Type === 'Episode') {
    const season = item.ParentIndexNumber
      ? `S${String(item.ParentIndexNumber).padStart(2, '0')}`
      : '';
    const episode = item.IndexNumber ? `E${String(item.IndexNumber).padStart(2, '0')}` : '';
    return [item.SeriesName, `${season}${episode}`].filter(Boolean).join(' ');
  }

  if (item.IsFolder) {
    return `${item.Type} · ${item.ChildCount || 0}`;
  }

  return [
    item.ProductionYear,
    item.MediaSources?.[0]?.Container || item.Container || item.MediaType || item.Type
  ]
    .filter(Boolean)
    .join(' · ');
}

export function streamLabel(type: string) {
  if (type === 'Video') return '视频';
  if (type === 'Audio') return '音频';
  if (type === 'Subtitle') return '字幕';
  return type;
}

export function streamText(stream: NonNullable<BaseItemDto['MediaStreams']>[number]) {
  const parts = [
    stream.DisplayTitle,
    stream.Codec,
    stream.Language,
    stream.Width && stream.Height ? `${stream.Width}x${stream.Height}` : '',
    stream.IsExternal ? '外挂' : ''
  ].filter(Boolean);
  return parts.join(' · ') || '默认轨道';
}

export function fileSize(size?: number) {
  if (!size) return '';
  const gb = size / 1024 / 1024 / 1024;
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  return `${(size / 1024 / 1024).toFixed(1)} MB`;
}

type RunOptions = {
  rethrow?: boolean;
};

export async function run(task: () => Promise<void>, success = '', options: RunOptions = {}): Promise<boolean> {
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
    return true;
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
    if (options.rethrow) {
      throw error;
    }
    return false;
  } finally {
    state.busy = false;
  }
}

function upsertServer(server: ServerEntry) {
  const normalized = normalizeServerUrl(server.Url);
  const next = { ...server, Url: normalized };
  const index = servers.value.findIndex(
    (entry) => normalizeServerUrl(entry.Url) === normalized
  );

  if (index >= 0) {
    servers.value[index] = {
      ...servers.value[index],
      ...next
    };
  } else {
    servers.value.push(next);
  }

  persistServers();
}

function ensureDefaultServer() {
  const fallback = defaultServerUrl();
  if (!currentServerUrl.value) {
    currentServerUrl.value = fallback;
    localStorage.setItem(CURRENT_SERVER_KEY, fallback);
  }

  if (
    !servers.value.some(
      (server) =>
        normalizeServerUrl(server.Url) === normalizeServerUrl(currentServerUrl.value)
    )
  ) {
    upsertServer({
      Id: normalizeServerUrl(currentServerUrl.value),
      Name: '当前服务器',
      Url: currentServerUrl.value
    });
  }
}

function persistServers() {
  localStorage.setItem(SERVERS_KEY, JSON.stringify(servers.value));
}

function normalizeServerUrl(url: string) {
  const value = url.trim();
  if (!value) {
    return defaultServerUrl();
  }

  return value.replace(/\/(emby|mediabrowser)\/?$/i, '').replace(/\/$/, '');
}

function compareDateCreatedDesc(left: BaseItemDto, right: BaseItemDto) {
  return parseDateValue(right.DateCreated) - parseDateValue(left.DateCreated);
}

function dedupeItemsById(items: BaseItemDto[]) {
  const seen = new Set<string>();
  return items.filter((item) => {
    if (seen.has(item.Id)) {
      return false;
    }

    seen.add(item.Id);
    return true;
  });
}

function parseDateValue(value?: string) {
  if (!value) {
    return 0;
  }

  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? 0 : timestamp;
}

function defaultServerUrl() {
  const configured = import.meta.env.VITE_API_BASE || '';
  if (!configured) {
    return window.location.origin;
  }

  if (/^https?:\/\//i.test(configured)) {
    return normalizeConfiguredUrl(configured);
  }

  return normalizeConfiguredUrl(new URL(configured, window.location.origin).toString());
}

function normalizeConfiguredUrl(url: string) {
  return url.replace(/\/(emby|mediabrowser)\/?$/i, '').replace(/\/$/, '');
}

function readJson<T>(key: string): T | null {
  const raw = localStorage.getItem(key);
  if (!raw) {
    return null;
  }

  try {
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

function readText(key: string) {
  return localStorage.getItem(key) || '';
}

// ======== 播放队列 / 稍后观看 ========

const QUEUE_KEY = 'movie-rust-queue';
const WATCH_LATER_KEY = 'movie-rust-watch-later';

export const playQueue = ref<BaseItemDto[]>(readJson<BaseItemDto[]>(QUEUE_KEY) || []);
export const watchLater = ref<BaseItemDto[]>(readJson<BaseItemDto[]>(WATCH_LATER_KEY) || []);
export const playQueueIndex = ref(0);

function persistQueue() {
  localStorage.setItem(QUEUE_KEY, JSON.stringify(playQueue.value));
}
function persistWatchLater() {
  localStorage.setItem(WATCH_LATER_KEY, JSON.stringify(watchLater.value));
}

export function enqueue(item: BaseItemDto, position: 'next' | 'last' = 'last') {
  const existing = playQueue.value.findIndex((entry) => entry.Id === item.Id);
  if (existing >= 0) {
    playQueue.value.splice(existing, 1);
  }
  if (position === 'next') {
    const insertAt = Math.min(playQueueIndex.value + 1, playQueue.value.length);
    playQueue.value.splice(insertAt, 0, item);
  } else {
    playQueue.value.push(item);
  }
  persistQueue();
}

export function dequeueAt(index: number) {
  playQueue.value.splice(index, 1);
  if (playQueueIndex.value >= playQueue.value.length) {
    playQueueIndex.value = Math.max(0, playQueue.value.length - 1);
  }
  persistQueue();
}

export function clearQueue() {
  playQueue.value = [];
  playQueueIndex.value = 0;
  persistQueue();
}

export function setQueue(items: BaseItemDto[], startIndex = 0) {
  playQueue.value = items.slice();
  playQueueIndex.value = Math.max(0, Math.min(startIndex, playQueue.value.length - 1));
  persistQueue();
}

export function nextInQueue(): BaseItemDto | null {
  if (playQueueIndex.value + 1 < playQueue.value.length) {
    playQueueIndex.value += 1;
    return playQueue.value[playQueueIndex.value];
  }
  return null;
}

export function toggleWatchLater(item: BaseItemDto) {
  const i = watchLater.value.findIndex((entry) => entry.Id === item.Id);
  if (i >= 0) {
    watchLater.value.splice(i, 1);
  } else {
    watchLater.value.unshift(item);
  }
  persistWatchLater();
}

export function isInWatchLater(id: string) {
  return watchLater.value.some((entry) => entry.Id === id);
}

// ======== 详情栈编码到 URL（刷新保留面包屑） ========

export function serializeParentStack() {
  if (!parentStack.value.length) return '';
  return parentStack.value.map((item) => item.Id).join(',');
}

export async function hydrateParentStack(serialized: string) {
  if (!serialized) {
    parentStack.value = [];
    return;
  }
  const ids = serialized.split(',').filter(Boolean);
  if (!ids.length) {
    parentStack.value = [];
    return;
  }
  try {
    const results = await Promise.all(ids.map((id) => api.item(id).catch(() => null)));
    parentStack.value = results.filter((item): item is BaseItemDto => Boolean(item));
  } catch {
    parentStack.value = [];
  }
}
