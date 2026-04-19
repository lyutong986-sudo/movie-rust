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
  ImageTags?: Record<string, string>;
  UserData: {
    PlaybackPositionTicks: number;
    PlayCount: number;
    IsFavorite: boolean;
    Played: boolean;
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
    return this.request('/System/Info/Public', { auth: false });
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

  async items(parentId?: string, searchTerm = '', recursive = false) {
    const userId = this.requireUserId();
    const params = new URLSearchParams({
      Recursive: recursive ? 'true' : 'false',
      SortBy: 'SortName',
      SortOrder: 'Ascending',
      Limit: '120'
    });
    if (parentId) {
      params.set('ParentId', parentId);
    }
    if (searchTerm.trim()) {
      params.set('SearchTerm', searchTerm.trim());
    }
    return this.request<QueryResult<BaseItemDto>>(`/Users/${userId}/Items?${params}`);
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

  itemImageUrl(item: BaseItemDto) {
    if (!item.ImageTags?.Primary) {
      return '';
    }
    return `${this.baseUrl}/Items/${item.Id}/Images/Primary?api_key=${encodeURIComponent(this.token)}`;
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
