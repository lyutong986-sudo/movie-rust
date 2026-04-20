export interface UserDto {
  Id: string;
  Name: string;
  ServerId: string;
  Policy: {
    IsAdministrator: boolean;
  };
}

export interface AuthResult {
  User: UserDto;
  AccessToken: string;
  ServerId: string;
}

export interface PublicSystemInfo {
  LocalAddress: string;
  ServerName: string;
  Version: string;
  ProductName: string;
  OperatingSystem: string;
  Id: string;
  StartupWizardCompleted: boolean;
}

export interface SystemInfo extends PublicSystemInfo {
  CanSelfRestart: boolean;
}

export interface BaseItemDto {
  Id: string;
  Name: string;
  Type: string;
  IsFolder: boolean;
  SortName?: string;
  CollectionType?: string;
  MediaType?: string;
  Container?: string;
  ParentId?: string;
  Path?: string;
  RunTimeTicks?: number;
  ProductionYear?: number;
  Overview?: string;
  Genres?: string[];
  ProviderIds?: Record<string, string>;
  SeriesName?: string;
  SeasonName?: string;
  IndexNumber?: number;
  IndexNumberEnd?: number;
  ParentIndexNumber?: number;
  DateCreated?: string;
  PremiereDate?: string;
  ImageTags?: Record<string, string>;
  BackdropImageTags?: string[];
  PrimaryImageAspectRatio?: number;
  UserData: {
    Rating?: number;
    PlayedPercentage?: number;
    UnplayedItemCount?: number;
    PlaybackPositionTicks: number;
    PlayCount: number;
    IsFavorite: boolean;
    Likes?: boolean;
    Played: boolean;
    LastPlayedDate?: string;
    Key?: string;
    ItemId?: string;
  };
  MediaSources?: Array<{
    Id: string;
    Path: string;
    Container: string;
    DirectStreamUrl: string;
    Size?: number;
    ETag?: string;
    DefaultAudioStreamIndex?: number;
    DefaultSubtitleStreamIndex?: number;
    MediaStreams: MediaStreamDto[];
  }>;
  MediaStreams?: MediaStreamDto[];
  ChildCount?: number;
}

export interface MediaStreamDto {
  Index: number;
  Type: 'Video' | 'Audio' | 'Subtitle' | string;
  Codec?: string;
  Language?: string;
  DisplayTitle?: string;
  IsDefault: boolean;
  IsForced: boolean;
  Width?: number;
  Height?: number;
  BitRate?: number;
  Channels?: number;
  SampleRate?: number;
  IsExternal: boolean;
  DeliveryMethod?: string;
  DeliveryUrl?: string;
  SupportsExternalStream: boolean;
  Path?: string;
}

export interface QueryResult<T> {
  Items: T[];
  TotalRecordCount: number;
  StartIndex?: number;
}

export interface ScanSummary {
  Libraries: number;
  ScannedFiles: number;
  ImportedItems: number;
}

export interface StartupConfiguration {
  ServerName: string;
  UiCulture: string;
  MetadataCountryCode: string;
  PreferredMetadataLanguage: string;
}

export interface StartupRemoteAccess {
  EnableRemoteAccess: boolean;
  EnableAutomaticPortMapping?: boolean;
}

export interface ItemQueryOptions {
  includeTypes?: string[];
  sortBy?: string;
  sortOrder?: 'Ascending' | 'Descending';
  limit?: number;
  startIndex?: number;
}

const TOKEN_KEY = 'movie-rust-token';
const USER_KEY = 'movie-rust-user';

export class EmbyApi {
  readonly baseUrl: string;
  token = localStorage.getItem(TOKEN_KEY) || '';
  user: UserDto | null = readJson<UserDto>(USER_KEY);

  constructor(baseUrl = '') {
    this.baseUrl = baseUrl.replace(/\/$/, '');
  }

  get isAuthenticated() {
    return Boolean(this.token && this.user);
  }

  async publicInfo() {
    return this.request<PublicSystemInfo>('/System/Info/Public', { auth: false });
  }

  async publicUsers() {
    return this.request<UserDto[]>('/Users/Public', { auth: false });
  }

  async systemInfo() {
    return this.request<SystemInfo>('/System/Info');
  }

  async users() {
    return this.request<UserDto[]>('/Users');
  }

  async createFirstAdmin(payload: { Name: string; Password: string }) {
    return this.request<UserDto>('/Startup/User', {
      method: 'POST',
      auth: false,
      body: payload
    });
  }

  async firstStartupUser() {
    return this.request<UserDto | null>('/Startup/User', { auth: false });
  }

  async startupConfiguration() {
    return this.request<StartupConfiguration>('/Startup/Configuration', { auth: false });
  }

  async updateStartupConfiguration(payload: StartupConfiguration) {
    return this.request<void>('/Startup/Configuration', {
      method: 'POST',
      auth: false,
      body: payload
    });
  }

  async updateRemoteAccess(payload: StartupRemoteAccess) {
    return this.request<void>('/Startup/RemoteAccess', {
      method: 'POST',
      auth: false,
      body: payload
    });
  }

  async completeStartup() {
    return this.request<void>('/Startup/Complete', {
      method: 'POST',
      auth: false
    });
  }

  async login(username: string, password: string) {
    const result = await this.request<AuthResult>('/Users/AuthenticateByName', {
      method: 'POST',
      auth: false,
      body: {
        Username: username,
        Pw: password,
        Client: 'Movie Rust Vue',
        Device: navigator.userAgent,
        DeviceId: getDeviceId()
      }
    });
    this.token = result.AccessToken;
    this.user = result.User;
    localStorage.setItem(TOKEN_KEY, result.AccessToken);
    localStorage.setItem(USER_KEY, JSON.stringify(result.User));
    return result;
  }

  logout() {
    this.token = '';
    this.user = null;
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(USER_KEY);
  }

  async libraries() {
    const userId = this.requireUserId();
    return this.request<QueryResult<BaseItemDto>>(`/Users/${userId}/Views`);
  }

  async items(parentId?: string, searchTerm = '', recursive = false, options: ItemQueryOptions = {}) {
    const userId = this.requireUserId();
    const params = new URLSearchParams({
      Recursive: recursive ? 'true' : 'false',
      SortBy: options.sortBy || 'SortName',
      SortOrder: options.sortOrder || 'Ascending',
      Limit: String(options.limit || 120)
    });
    if (options.startIndex !== undefined) {
      params.set('StartIndex', String(options.startIndex));
    }
    if (parentId) {
      params.set('ParentId', parentId);
    }
    if (searchTerm.trim()) {
      params.set('SearchTerm', searchTerm.trim());
    }
    if (options.includeTypes?.length) {
      params.set('IncludeItemTypes', options.includeTypes.join(','));
    }
    return this.request<QueryResult<BaseItemDto>>(`/Users/${userId}/Items?${params}`);
  }

  async item(itemId: string) {
    const userId = this.requireUserId();
    return this.request<BaseItemDto>(`/Users/${userId}/Items/${itemId}`);
  }

  async latest(parentId?: string, limit = 12) {
    const userId = this.requireUserId();
    const params = new URLSearchParams({
      Limit: String(limit)
    });
    if (parentId) {
      params.set('ParentId', parentId);
    }
    return this.request<BaseItemDto[]>(`/Users/${userId}/Items/Latest?${params}`);
  }

  async createLibrary(payload: { Name: string; Path: string; CollectionType: string }) {
    return this.request<BaseItemDto>('/api/admin/libraries', {
      method: 'POST',
      body: payload
    });
  }

  async scan() {
    return this.request<ScanSummary>('/api/admin/scan', {
      method: 'POST'
    });
  }

  async markFavorite(itemId: string, isFavorite: boolean) {
    const userId = this.requireUserId();
    return this.request<BaseItemDto['UserData']>(`/Users/${userId}/FavoriteItems/${itemId}`, {
      method: isFavorite ? 'POST' : 'DELETE'
    });
  }

  async markPlayed(itemId: string, isPlayed: boolean) {
    const userId = this.requireUserId();
    return this.request<BaseItemDto['UserData']>(`/Users/${userId}/PlayedItems/${itemId}`, {
      method: isPlayed ? 'POST' : 'DELETE'
    });
  }

  async updateUserData(
    itemId: string,
    payload: Partial<
      Pick<BaseItemDto['UserData'], 'PlaybackPositionTicks' | 'PlayCount' | 'IsFavorite' | 'Played'>
    >
  ) {
    const userId = this.requireUserId();
    return this.request<BaseItemDto['UserData']>(`/Users/${userId}/Items/${itemId}/UserData`, {
      method: 'POST',
      body: payload
    });
  }

  itemImageUrl(item: BaseItemDto) {
    return this.imageUrl(item, 'Primary', item.ImageTags?.Primary);
  }

  backdropUrl(item: BaseItemDto) {
    return this.imageUrl(item, 'Backdrop', item.BackdropImageTags?.[0], 0);
  }

  private imageUrl(item: BaseItemDto, imageType: string, tag?: string, imageIndex?: number) {
    if (!tag) {
      return '';
    }
    const indexSegment = imageIndex === undefined ? '' : `/${imageIndex}`;
    return `${this.baseUrl}/Items/${item.Id}/Images/${imageType}${indexSegment}?api_key=${encodeURIComponent(this.token)}&tag=${encodeURIComponent(tag)}`;
  }

  streamUrl(item: BaseItemDto) {
    const directUrl = item.MediaSources?.[0]?.DirectStreamUrl;
    if (directUrl) {
      const joiner = directUrl.includes('?') ? '&' : '?';
      return `${this.baseUrl}${directUrl}${joiner}api_key=${encodeURIComponent(this.token)}`;
    }

    return `${this.baseUrl}/Videos/${item.Id}/stream?static=true&api_key=${encodeURIComponent(this.token)}`;
  }

  private requireUserId() {
    if (!this.user) {
      throw new Error('未登录');
    }
    return this.user.Id;
  }

  private async request<T>(path: string, options: RequestOptions = {}) {
    const headers = new Headers(options.headers);
    headers.set('Content-Type', 'application/json');
    if (options.auth !== false && this.token) {
      headers.set('X-Emby-Token', this.token);
      headers.set(
        'Authorization',
        `MediaBrowser Client="Movie Rust Vue", Device="${navigator.userAgent}", DeviceId="${getDeviceId()}", Version="0.1.0", Token="${this.token}"`
      );
    }

    const response = await fetch(`${this.baseUrl}${path}`, {
      method: options.method || 'GET',
      headers,
      body: options.body ? JSON.stringify(options.body) : undefined
    });

    if (!response.ok) {
      const text = await response.text();
      throw new Error(text || `HTTP ${response.status}`);
    }

    if (response.status === 204) {
      return undefined as T;
    }

    return (await response.json()) as T;
  }
}

interface RequestOptions {
  method?: string;
  headers?: HeadersInit;
  body?: unknown;
  auth?: boolean;
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

function getDeviceId() {
  const key = 'movie-rust-device-id';
  const existing = localStorage.getItem(key);
  if (existing) {
    return existing;
  }
  const value = crypto.randomUUID();
  localStorage.setItem(key, value);
  return value;
}
