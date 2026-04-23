export interface UserDto {
  Id: string;
  Name: string;
  ServerId: string;
  HasPassword?: boolean;
  HasConfiguredPassword?: boolean;
  Policy: UserPolicy;
  Configuration?: Record<string, unknown>;
}

export interface AccessSchedule {
  DayOfWeek: string;
  StartHour: number;
  EndHour: number;
}

export interface UserPolicy {
  IsAdministrator: boolean;
  IsHidden?: boolean;
  IsDisabled?: boolean;
  EnableRemoteAccess?: boolean;
  EnableMediaPlayback?: boolean;
  EnableContentDeletion?: boolean;
  EnableContentDownloading?: boolean;
  EnableAudioPlaybackTranscoding?: boolean;
  EnableVideoPlaybackTranscoding?: boolean;
  EnablePlaybackRemuxing?: boolean;
  EnableUserPreferenceAccess?: boolean;
  MaxParentalRating?: number | null;
  MaxActiveSessions?: number;
  LoginAttemptsBeforeLockout?: number;
  RemoteClientBitrateLimit?: number;
  BlockedTags?: string[];
  AllowedTags?: string[];
  BlockUnratedItems?: string[];
  AccessSchedules?: AccessSchedule[];
  EnabledFolders?: string[];
  EnableAllFolders?: boolean;
  EnabledDevices?: string[];
  EnableAllDevices?: boolean;
  EnableContentDeletionFromFolders?: string[];
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
  EncoderLocationType?: string;
}

export interface SessionInfo {
  Id: string;
  UserId: string;
  UserName: string;
  Client: string;
  DeviceId: string;
  DeviceName: string;
  ApplicationVersion: string;
  IsActive: boolean;
  LastActivityDate: string;
}

export interface ActivityLogEntry {
  Id: string;
  Name: string;
  Type: string;
  ShortOverview?: string;
  Severity: string;
  Date: string;
}

export interface LogFileDto {
  Name: string;
  DateModified: string;
}

export interface PlaybackInfoResponse {
  MediaSources: NonNullable<BaseItemDto['MediaSources']>;
  PlaySessionId: string;
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
  LibraryScanThreadCount: number;
  StrmAnalysisThreadCount: number;
  TmdbMetadataThreadCount: number;
}

export interface StartupRemoteAccess {
  EnableRemoteAccess: boolean;
  EnableAutomaticPortMapping?: boolean;
}

export interface EncodingOptions {
  EnableTranscoding: boolean;
  EnableThrottling: boolean;
  HardwareAccelerationType: string;
  VaapiDevice: string;
  EncodingThreadCount: number;
  DownMixAudioBoost: number;
  EncoderAppPath: string;
  EncoderLocationType: 'System' | 'Custom' | string;
  TranscodingTempPath: string;
  H264Preset: string;
  H264Crf: number;
  MaxTranscodeSessions: number;
}

export interface MediaPathInfo {
  Path: string;
}

export interface LocalizationCulture {
  DisplayName: string;
  Name: string;
  ThreeLetterISOLanguageName: string;
  TwoLetterISOLanguageName: string;
}

export interface LibraryOptions {
  Enabled: boolean;
  EnablePhotos: boolean;
  EnableInternetProviders: boolean;
  DownloadImagesInAdvance: boolean;
  EnableRealtimeMonitor: boolean;
  ExcludeFromSearch: boolean;
  IgnoreHiddenFiles: boolean;
  EnableChapterImageExtraction: boolean;
  ExtractChapterImagesDuringLibraryScan: boolean;
  SaveLocalMetadata: boolean;
  SaveMetadataHidden: boolean;
  MergeTopLevelFolders: boolean;
  PlaceholderMetadataRefreshIntervalDays: number;
  ImportMissingEpisodes: boolean;
  EnableAutomaticSeriesGrouping: boolean;
  EnableEmbeddedTitles: boolean;
  EnableEmbeddedEpisodeInfos: boolean;
  EnableMultiVersionByFiles: boolean;
  EnableMultiVersionByMetadata: boolean;
  EnableMultiPartItems: boolean;
  AutomaticRefreshIntervalDays: number;
  PreferredMetadataLanguage?: string;
  PreferredImageLanguage?: string;
  MetadataCountryCode?: string;
  SeasonZeroDisplayName: string;
  MetadataSavers: string[];
  ImportCollections: boolean;
  MinCollectionItems: number;
  DisabledLocalMetadataReaders: string[];
  LocalMetadataReaderOrder: string[];
  PathInfos: MediaPathInfo[];
}

export interface VirtualFolderInfo {
  Name: string;
  CollectionType: string;
  ItemId: string;
  Locations: string[];
  LibraryOptions: LibraryOptions;
}

export interface FileSystemEntryInfo {
  Name: string;
  Path: string;
  Type: 'File' | 'Directory' | 'NetworkComputer' | 'NetworkShare' | string;
}

export interface CreateLibraryPayload {
  Name: string;
  CollectionType: string;
  Path?: string;
  Paths: string[];
  LibraryOptions: LibraryOptions;
}

export interface ItemQueryOptions {
  includeTypes?: string[];
  genres?: string[];
  sortBy?: string;
  sortOrder?: 'Ascending' | 'Descending';
  limit?: number;
  startIndex?: number;
}

export interface PlaybackReportPayload {
  ItemId: string;
  PlaySessionId: string;
  MediaSourceId?: string;
  PositionTicks?: number;
  IsPaused?: boolean;
  PlayedToCompletion?: boolean;
}

const TOKEN_KEY = 'movie-rust-token';
const USER_KEY = 'movie-rust-user';

export class EmbyApi {
  baseUrl: string;
  token = localStorage.getItem(TOKEN_KEY) || '';
  user: UserDto | null = readJson<UserDto>(USER_KEY);

  constructor(baseUrl = '') {
    this.baseUrl = normalizeBaseUrl(baseUrl);
  }

  get isAuthenticated() {
    return Boolean(this.token && this.user);
  }

  setBaseUrl(baseUrl: string) {
    this.baseUrl = normalizeBaseUrl(baseUrl);
  }

  async publicInfoAt(baseUrl: string) {
    return this.requestAtBaseUrl<PublicSystemInfo>(baseUrl, '/System/Info/Public', { auth: false });
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

  async encodingConfiguration() {
    return this.request<EncodingOptions>('/System/Configuration/encoding');
  }

  async updateEncodingConfiguration(payload: EncodingOptions) {
    return this.request<EncodingOptions>('/System/Configuration/encoding', {
      method: 'POST',
      body: payload
    });
  }

  async updateMediaEncoderPath(payload: { Path: string; PathType: string }) {
    return this.request<EncodingOptions>('/System/MediaEncoder/Path', {
      method: 'POST',
      body: payload
    });
  }

  async users() {
    return this.request<UserDto[]>('/Users');
  }

  async createUser(name: string, options?: { password?: string; copyFromUserId?: string }) {
    return this.request<UserDto>('/Users/New', {
      method: 'POST',
      body: {
        Name: name,
        ...(options?.password ? { Password: options.password } : {}),
        ...(options?.copyFromUserId ? { CopyFromUserId: options.copyFromUserId } : {})
      }
    });
  }

  async deleteUser(userId: string) {
    return this.request<void>(`/Users/${userId}/Delete`, {
      method: 'POST'
    });
  }

  async updateUserPolicy(userId: string, policy: UserPolicy) {
    return this.request<void>(`/Users/${userId}/Policy`, {
      method: 'POST',
      body: policy
    });
  }

  async me() {
    return this.request<UserDto>('/Users/Me');
  }

  async sessions() {
    return this.request<SessionInfo[]>('/Sessions');
  }

  async activity(limit = 50) {
    return this.request<QueryResult<ActivityLogEntry>>(`/System/ActivityLog/Entries?Limit=${limit}`);
  }

  async serverLogs() {
    return this.request<LogFileDto[]>('/System/Logs');
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
    if (options.genres?.length) {
      params.set('Genres', options.genres.join(','));
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

  async playbackInfo(itemId: string) {
    return this.request<PlaybackInfoResponse>(`/Items/${itemId}/PlaybackInfo`);
  }

  async playbackStarted(payload: PlaybackReportPayload) {
    return this.request<void>('/Sessions/Playing', {
      method: 'POST',
      body: payload
    });
  }

  async playbackProgress(payload: PlaybackReportPayload) {
    return this.request<void>('/Sessions/Playing/Progress', {
      method: 'POST',
      body: payload
    });
  }

  async playbackStopped(payload: PlaybackReportPayload) {
    return this.request<void>('/Sessions/Playing/Stopped', {
      method: 'POST',
      body: payload
    });
  }

  async adminLibraries() {
    return this.request<BaseItemDto[]>('/api/admin/libraries');
  }

  async virtualFolders() {
    return this.request<VirtualFolderInfo[]>('/Library/VirtualFolders');
  }

  async environmentDrives() {
    return this.request<FileSystemEntryInfo[]>('/Environment/Drives');
  }

  async directoryContents(path: string, includeFiles = false, includeDirectories = true) {
    const params = new URLSearchParams({
      Path: path,
      IncludeFiles: includeFiles ? 'true' : 'false',
      IncludeDirectories: includeDirectories ? 'true' : 'false'
    });
    return this.request<FileSystemEntryInfo[]>(`/Environment/DirectoryContents?${params}`);
  }

  async parentPath(path: string) {
    const params = new URLSearchParams({ Path: path });
    return this.request<string>(`/Environment/ParentPath?${params}`);
  }

  async localizationCultures() {
    return this.request<LocalizationCulture[]>('/Localization/Cultures');
  }

  async createLibrary(payload: CreateLibraryPayload, refreshLibrary = false) {
    const params = new URLSearchParams({
      refreshLibrary: refreshLibrary ? 'true' : 'false'
    });

    return this.request<BaseItemDto>(`/api/admin/libraries?${params}`, {
      method: 'POST',
      body: payload
    });
  }

  async deleteLibrary(libraryId: string, refreshLibrary = false) {
    const params = new URLSearchParams({
      refreshLibrary: refreshLibrary ? 'true' : 'false'
    });

    return this.request<void>(`/api/admin/libraries/${libraryId}?${params}`, {
      method: 'DELETE'
    });
  }

  async createVirtualFolder(payload: CreateLibraryPayload, refreshLibrary = false) {
    const params = new URLSearchParams({
      name: payload.Name,
      collectionType: payload.CollectionType,
      paths: payload.Paths.join(','),
      refreshLibrary: refreshLibrary ? 'true' : 'false'
    });

    return this.request<void>(`/Library/VirtualFolders?${params}`, {
      method: 'POST',
      body: {
        LibraryOptions: payload.LibraryOptions
      }
    });
  }

  async deleteVirtualFolder(name: string, refreshLibrary = false) {
    const params = new URLSearchParams({
      name,
      refreshLibrary: refreshLibrary ? 'true' : 'false'
    });

    return this.request<void>(`/Library/VirtualFolders?${params}`, {
      method: 'DELETE'
    });
  }

  async updateLibraryOptions(id: string, libraryOptions: LibraryOptions) {
    return this.request<void>('/Library/VirtualFolders/LibraryOptions', {
      method: 'POST',
      body: {
        Id: id,
        LibraryOptions: libraryOptions
      }
    });
  }

  async renameVirtualFolder(name: string, newName: string, refreshLibrary = false) {
    const params = new URLSearchParams({
      name,
      newName,
      refreshLibrary: refreshLibrary ? 'true' : 'false'
    });

    return this.request<void>(`/Library/VirtualFolders/Name?${params}`, {
      method: 'POST'
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

  async changePassword(userId: string, payload: { CurrentPw?: string; CurrentPassword?: string; NewPw: string }) {
    return this.request<void>(`/Users/${userId}/Password`, {
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
      return this.streamUrlForSource(item.MediaSources![0]);
    }

    return `${this.baseUrl}/Videos/${item.Id}/stream?static=true&api_key=${encodeURIComponent(this.token)}`;
  }

  streamUrlForSource(source: NonNullable<BaseItemDto['MediaSources']>[number]) {
    const directUrl = source.DirectStreamUrl;
    if (!directUrl) {
      return '';
    }

    const joiner = directUrl.includes('?') ? '&' : '?';
    return `${this.baseUrl}${directUrl}${joiner}api_key=${encodeURIComponent(this.token)}`;
  }

  subtitleUrl(deliveryUrl?: string) {
    if (!deliveryUrl) {
      return '';
    }

    const joiner = deliveryUrl.includes('?') ? '&' : '?';
    return `${this.baseUrl}${deliveryUrl}${joiner}api_key=${encodeURIComponent(this.token)}`;
  }

  private requireUserId() {
    if (!this.user) {
      throw new Error('未登录');
    }
    return this.user.Id;
  }

  private async request<T>(path: string, options: RequestOptions = {}) {
    return this.requestAtBaseUrl<T>(this.baseUrl, path, options);
  }

  private async requestAtBaseUrl<T>(baseUrl: string, path: string, options: RequestOptions = {}) {
    const headers = new Headers(options.headers);
    headers.set('Content-Type', 'application/json');
    if (options.auth !== false && this.token) {
      headers.set('X-Emby-Token', this.token);
      headers.set(
        'X-Emby-Authorization',
        `MediaBrowser Client="Movie Rust Vue", Device="${navigator.userAgent}", DeviceId="${getDeviceId()}", Version="0.1.0", Token="${this.token}"`
      );
      headers.set(
        'Authorization',
        `MediaBrowser Client="Movie Rust Vue", Device="${navigator.userAgent}", DeviceId="${getDeviceId()}", Version="0.1.0", Token="${this.token}"`
      );
    }

    const response = await fetch(`${normalizeBaseUrl(baseUrl)}${path}`, {
      method: options.method || 'GET',
      headers,
      body: options.body ? JSON.stringify(options.body) : undefined
    });

    if (!response.ok) {
      const text = await response.text();
      if (options.auth !== false && (response.status === 401 || response.status === 403)) {
        this.logout();
      }
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

function normalizeBaseUrl(baseUrl: string) {
  const value = baseUrl.trim();
  if (!value) {
    return '';
  }

  return value.replace(/\/(emby|mediabrowser)\/?$/i, '').replace(/\/$/, '');
}
