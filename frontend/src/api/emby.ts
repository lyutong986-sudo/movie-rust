export interface UserDto {
  Id: string;
  Name: string;
  ServerId: string;
  HasPassword?: boolean;
  HasConfiguredPassword?: boolean;
  HasConfiguredEasyPassword?: boolean;
  PrimaryImageTag?: string;
  LastLoginDate?: string;
  LastActivityDate?: string;
  DateCreated?: string;
  Policy: UserPolicy;
  Configuration?: UserConfiguration;
}

export interface UserConfiguration {
  PlayDefaultAudioTrack?: boolean;
  PlayDefaultSubtitleTrack?: boolean;
  SubtitleMode?: string;
  AudioLanguagePreference?: string;
  SubtitleLanguagePreference?: string;
  DisplayMissingEpisodes?: boolean;
  GroupedFolders?: string[];
  LatestItemsExcludes?: string[];
  MyMediaExcludes?: string[];
  OrderedViews?: string[];
  HidePlayedInLatest?: boolean;
  RememberAudioSelections?: boolean;
  RememberSubtitleSelections?: boolean;
  EnableLocalPassword?: boolean;
  EnableBackdrops?: boolean;
  EnableThemeSongs?: boolean;
  DisplayUnairedEpisodes?: boolean;
  EnableCinemaMode?: boolean;
  EnableNextEpisodeAutoPlay?: boolean;
  MaxStreamingBitrate?: number;
  MaxChromecastBitrate?: number;
}

export interface BrandingConfiguration {
  LoginDisclaimer: string;
  CustomCss: string;
  SplashscreenEnabled: boolean;
}

export interface PlaybackConfiguration {
  MinResumePct: number;
  MaxResumePct: number;
  MinResumeDurationSeconds: number;
  MinAudiobookResume: number;
  MaxAudiobookResume: number;
}

export interface NetworkConfiguration {
  LocalAddress: string;
  HttpServerPortNumber: number;
  HttpsPortNumber: number;
  PublicHttpPort: number;
  PublicHttpsPort: number;
  CertificatePath: string;
  EnableHttps: boolean;
  ExternalDomain: string;
  EnableUPnP: boolean;
}

export interface LibraryDisplayConfiguration {
  DisplayFolderView: boolean;
  DisplaySpecialsWithinSeasons: boolean;
  GroupMoviesIntoCollections: boolean;
  DisplayCollectionsView: boolean;
  EnableExternalContentInSuggestions: boolean;
  DateAddedBehavior: number;
  MetadataPath: string;
  SaveMetadataHidden: boolean;
  SeasonZeroDisplayName: string;
  FanartApiKey: string;
}

export interface SubtitleDownloadConfiguration {
  DownloadSubtitlesForMovies: boolean;
  DownloadSubtitlesForEpisodes: boolean;
  DownloadLanguages: string[];
  RequirePerfectMatch: boolean;
  SkipIfAudioTrackPresent: boolean;
  SkipIfGraphicalSubsPresent: boolean;
  OpenSubtitlesUsername: string;
  OpenSubtitlesPassword: string;
  OpenSubtitlesApiKey: string;
}

export interface RemoteSubtitleInfo {
  ThreeLetterISOLanguageName?: string;
  Id: string;
  ProviderName: string;
  Name: string;
  Format: string;
  Author?: string;
  Comment?: string;
  DateCreated?: string;
  CommunityRating?: number;
  DownloadCount?: number;
  IsHashMatch?: boolean;
  IsForced?: boolean;
  IsHearingImpaired?: boolean;
  Language?: string;
}

export interface ImageInfo {
  ImageType: string;
  ImageIndex?: number;
  ImageTag?: string;
  Path?: string;
  Height?: number;
  Width?: number;
  Size?: number;
}

export interface RemoteImageInfo {
  ProviderName: string;
  Url: string;
  ThumbnailUrl?: string;
  Height?: number;
  Width?: number;
  CommunityRating?: number;
  VoteCount?: number;
  Language?: string;
  Type: string;
  RatingType?: string;
}

export interface RemoteImageResult {
  Images: RemoteImageInfo[];
  TotalRecordCount: number;
  Providers: string[];
}

export interface ScheduledTaskTrigger {
  Type: string;
  IntervalTicks?: number;
  TimeOfDayTicks?: number;
  DayOfWeek?: string;
  MaxRuntimeTicks?: number;
}

export interface ScheduledTaskInfo {
  Id: string;
  Key?: string;
  Name: string;
  Description: string;
  Category: string;
  State: string;
  CurrentProgressPercentage?: number | null;
  Triggers: ScheduledTaskTrigger[];
  LastExecutionResult?: {
    StartTimeUtc?: string;
    StartTime?: string;
    EndTimeUtc?: string;
    EndTime?: string;
    Status?: string;
    DurationTicks?: number;
    ErrorMessage?: string | null;
  } | null;
  IsHidden?: boolean;
}

export interface ApiKeyInfo {
  Id: string;
  AccessToken: string;
  UserId: string;
  UserName: string;
  AppName: string;
  AppVersion: string;
  DeviceId?: string | null;
  DeviceName?: string | null;
  DateLastActivity?: string;
  ExpirationDate?: string | null;
  IsActive?: boolean;
}

export interface PlaylistInfo {
  Id: string;
  Name: string;
  ServerId: string;
  MediaType: string;
  UserId: string;
  Overview?: string | null;
  ChildCount: number;
  DateCreated: string;
  DateModified: string;
  PrimaryImageTag?: string | null;
}

export interface ForgotPasswordResult {
  Action: string;
  PinFile?: string;
  PinExpirationDate?: string;
}

export interface ForgotPasswordPinResult {
  Success: boolean;
  UsersReset?: string[];
}

export interface AccessSchedule {
  DayOfWeek: string;
  StartHour: number;
  EndHour: number;
}

export interface ImportEmbyUserItem {
  Name: string;
  LegacyPasswordHash: string;
  LegacyPasswordFormat?: string;
  ExternalId?: string;
  Policy?: Partial<UserPolicy>;
}

export interface ImportEmbyUsersRequest {
  Users: ImportEmbyUserItem[];
  ConflictPolicy?: 'skip' | 'overwrite';
  DefaultPolicy?: Partial<UserPolicy>;
  DefaultLegacyFormat?: string;
}

export interface ImportedUserSummary {
  UserId: string;
  Name: string;
  ExternalId?: string | null;
}

export interface ImportFailureSummary {
  Name: string;
  ExternalId?: string | null;
  Error: string;
}

export interface ImportEmbyUsersResponse {
  Created: ImportedUserSummary[];
  Updated: ImportedUserSummary[];
  Skipped: ImportedUserSummary[];
  Failed: ImportFailureSummary[];
}

export interface UserPolicy {
  IsAdministrator: boolean;
  IsHidden?: boolean;
  IsHiddenRemotely?: boolean;
  IsDisabled?: boolean;
  EnableRemoteAccess?: boolean;
  EnableRemoteControlOfOtherUsers?: boolean;
  EnableSharedDeviceControl?: boolean;
  EnablePublicSharing?: boolean;
  EnableMediaPlayback?: boolean;
  EnableContentDeletion?: boolean;
  EnableContentDownloading?: boolean;
  EnableSyncTranscoding?: boolean;
  EnableMediaConversion?: boolean;
  EnableCollectionManagement?: boolean;
  EnableSubtitleManagement?: boolean;
  EnableSubtitleDownloading?: boolean;
  EnableLyricManagement?: boolean;
  EnableAudioPlaybackTranscoding?: boolean;
  EnableVideoPlaybackTranscoding?: boolean;
  EnablePlaybackRemuxing?: boolean;
  ForceRemoteSourceTranscoding?: boolean;
  EnableUserPreferenceAccess?: boolean;
  MaxParentalRating?: number | null;
  MaxParentalSubRating?: number | null;
  SimultaneousStreamLimit?: number;
  InvalidLoginAttemptCount?: number;
  LoginAttemptsBeforeLockout?: number;
  RemoteClientBitrateLimit?: number;
  BlockedTags?: string[];
  AllowedTags?: string[];
  BlockUnratedItems?: string[];
  AccessSchedules?: AccessSchedule[];
  EnabledFolders?: string[];
  EnableAllFolders?: boolean;
  BlockedMediaFolders?: string[];
  EnabledChannels?: string[];
  EnableAllChannels?: boolean;
  BlockedChannels?: string[];
  EnabledDevices?: string[];
  EnableAllDevices?: boolean;
  EnableContentDeletionFromFolders?: string[];
  AuthenticationProviderId?: string;
  PasswordResetProviderId?: string;
  SyncPlayAccess?: string;
  AllowCameraUpload?: boolean;
}

export interface AuthResult {
  User: UserDto;
  SessionInfo?: SessionInfo;
  AccessToken: string;
  ServerId: string;
}

export interface PublicSystemInfo {
  LocalAddress: string;
  LocalAddresses?: string[];
  WanAddress?: string;
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
  HasPendingRestart?: boolean;
  ProgramDataPath?: string;
  ItemsByNamePath?: string;
  LogPath?: string;
  InternalMetadataPath?: string;
  TranscodingTempPath?: string;
  CachePath?: string;
}

export interface SessionInfo {
  Id: string;
  UserId: string;
  UserName: string;
  ServerId: string;
  Client: string;
  DeviceId: string;
  DeviceName: string;
  ApplicationVersion: string;
  IsActive: boolean;
  LastActivityDate: string;
  RemoteEndPoint?: string;
  SupportsRemoteControl?: boolean;
  PlayableMediaTypes?: string[];
  SupportedCommands?: string[];
  NowPlayingItem?: BaseItemDto;
  NowViewingItem?: BaseItemDto;
  PlayState?: Record<string, unknown>;
  AdditionalUsers?: unknown[];
  NowPlayingQueue?: unknown[];
  UserPrimaryImageTag?: string;
}

export interface ActivityLogEntry {
  Id: string;
  Name: string;
  Type: string;
  ShortOverview?: string;
  Severity: string;
  Date: string;
  UserId?: string;
  ItemId?: string;
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
  PlaylistItemId?: string;
  SortName?: string;
  OriginalTitle?: string;
  CollectionType?: string;
  MediaType?: string;
  Status?: string;
  Container?: string;
  ParentId?: string;
  SeriesId?: string;
  SeasonId?: string;
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
  EndDate?: string;
  ProductionLocations?: string[];
  ExternalUrls?: Array<{ Name: string; Url: string }>;
  ImageTags?: Record<string, string>;
  BackdropImageTags?: string[];
  PrimaryImageAspectRatio?: number;
  ImageBlurHashes?: Record<string, Record<string, string>>;
  Chapters?: Array<{
    StartPositionTicks: number;
    Name?: string;
    ImageTag?: string;
  }>;
  RemoteTrailers?: Array<{ Url: string; Name?: string }>;
  LocalTrailerCount?: number;
  SpecialFeatureCount?: number;
  HasSubtitles?: boolean;
  IsHD?: boolean;
  Width?: number;
  Height?: number;
  VideoRange?: string;
  AudioCodec?: string;
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
    TranscodingUrl?: string;
    SupportsDirectPlay?: boolean;
    SupportsDirectStream?: boolean;
    SupportsTranscoding?: boolean;
    AddApiKeyToDirectStreamUrl?: boolean;
    Size?: number;
    Bitrate?: number;
    ETag?: string;
    DefaultAudioStreamIndex?: number;
    DefaultSubtitleStreamIndex?: number;
    MediaStreams: MediaStreamDto[];
  }>;
  MediaStreams?: MediaStreamDto[];
  ChildCount?: number;
  OfficialRating?: string;
  CommunityRating?: number;
  CriticRating?: number;
  Tags?: string[];
  Studios?: Array<{ Id?: string; Name: string }>;
  People?: Array<{
    Id?: string;
    Name: string;
    Role?: string;
    Type?: string;
    PrimaryImageTag?: string;
    ImageBlurHashes?: Record<string, Record<string, string>>;
  }>;
  Taglines?: string[];
  Tagline?: string;
}

export interface MediaStreamDto {
  Index: number;
  Type: 'Video' | 'Audio' | 'Subtitle' | string;
  Codec?: string;
  Language?: string;
  DisplayTitle?: string;
  Title?: string;
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
  AspectRatio?: string;
  RealFrameRate?: number;
  BitDepth?: number;
  ColorSpace?: string;
  ColorPrimaries?: string;
  ColorTransfer?: string;
  VideoRange?: string;
  VideoRangeType?: string;
  Profile?: string;
  Level?: number;
  ChannelLayout?: string;
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

export interface ScanOperation {
  Id: string;
  Trigger: string;
  ScopeKey?: string;
  LibraryId?: string;
  LibraryName?: string;
  Phase?: string;
  CurrentLibrary?: string | null;
  TotalFiles?: number;
  ScannedFiles?: number;
  ImportedItems?: number;
  ScanRatePerSec?: number;
  Status: string;
  Progress: number;
  Queued: boolean;
  Running: boolean;
  Done: boolean;
  CancelRequested: boolean;
  CreatedAt: string;
  StartedAt?: string;
  CompletedAt?: string;
  Attempts: number;
  MaxAttempts: number;
  Result?: ScanSummary;
  Error?: string;
  MonitorUrl: string;
}

export interface ScanQueuedResponse {
  Queued: boolean;
  Message?: string;
  Operation: ScanOperation;
}

export interface MediaUpdateInfo {
  Path: string;
  UpdateType?: string;
}

export interface RemoteEmbySource {
  Id: string;
  Name: string;
  ServerUrl: string;
  Username: string;
  TargetLibraryId: string;
  DisplayMode: 'merge' | 'separate' | string;
  RemoteViewIds: string[];
  RemoteViews: RemoteEmbyView[];
  Enabled: boolean;
  SpoofedUserAgent: string;
  RemoteUserId?: string;
  HasAccessToken: boolean;
  LastSyncAt?: string;
  LastSyncError?: string;
  /** 本地 STRM 输出根目录；实际写入 `{根}/{SanitizedName}.{源Id}` 子目录 */
  StrmOutputPath?: string;
  SyncMetadata?: boolean;
  SyncSubtitles?: boolean;
  TokenRefreshIntervalSecs?: number;
  LastTokenRefreshAt?: string;
  /** 独立显示模式下 view_id → 本地库 id 的映射 */
  ViewLibraryMap?: Record<string, string>;
  /**
   * 流量模式：
   * - `"proxy"`（默认）：本地服务器中转所有流量，客户端无需直连远端
   * - `"redirect"`：返回 302 重定向到远端直链，节省本地带宽（要求客户端能直连远端）
   */
  ProxyMode: 'proxy' | 'redirect' | string;
  CreatedAt: string;
  UpdatedAt: string;
}

export interface RemoteEmbyView {
  Id: string;
  Name: string;
  CollectionType?: string;
}

/** 预览远端 Emby 源时的返回结果 */
export interface RemoteEmbyPreviewResult {
  /** 远端服务器名称（从 /System/Info 获取，可用于自动填充"源名称"） */
  ServerName: string;
  Views: RemoteEmbyView[];
}

export interface RemoteEmbySyncResponse {
  SourceId: string;
  SourceName: string;
  WrittenFiles: number;
  SourceRoot: string;
  ScanSummary: ScanSummary;
}

export interface RemoteEmbySyncOperation {
  Id: string;
  SourceId: string;
  SourceName: string;
  Status: string;
  Progress: number;
  Phase: string;
  TotalItems: number;
  FetchedItems: number;
  WrittenFiles: number;
  Queued: boolean;
  Running: boolean;
  Done: boolean;
  CancelRequested: boolean;
  CreatedAt: string;
  StartedAt?: string;
  CompletedAt?: string;
  Result?: RemoteEmbySyncResponse;
  Error?: string;
  MonitorUrl: string;
}

export interface RemoteEmbySyncQueuedResponse {
  Queued: boolean;
  Message?: string;
  Operation: RemoteEmbySyncOperation;
}

export interface StartupConfiguration {
  ServerName: string;
  UiCulture: string;
  MetadataCountryCode: string;
  PreferredMetadataLanguage: string;
  LibraryScanThreadCount: number;
  StrmAnalysisThreadCount: number;
  TmdbMetadataThreadCount: number;
  TmdbApiKey: string;
  TmdbApiKeys: string[];
  FanartApiKeys: string[];
  SubtitleApiKeys: string[];
  PerformanceTier: string;
  DbMaxConnections: number;
  ImageDownloadThreads: number;
  BackgroundTaskThreads: number;
}

export interface UserSettingsResponse {
  UserId: string;
  Configuration: UserConfiguration;
  Policy: UserPolicy;
  PreferredMetadataLanguage?: string;
  PreferredMetadataCountryCode?: string;
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

export interface ExternalIdInfo {
  Name: string;
  Key: string;
  UrlFormatString?: string;
  Type?: string;
}

export interface MetadataEditorInfo {
  ExternalIdInfos: ExternalIdInfo[];
  ParentalRatingOptions?: Array<{ Name: string; Value: number }>;
  Countries?: Array<{ DisplayName: string; Name: string; TwoLetterISORegionName: string }>;
  Cultures?: Array<{ DisplayName: string; Name: string; ThreeLetterISOLanguageName: string; TwoLetterISOLanguageName: string }>;
  ContentType?: string;
  ContentTypeOptions?: Array<{ Name: string; Value: string }>;
}

export interface ItemQueryOptions {
  includeTypes?: string[];
  genres?: string[];
  years?: number[];
  isFavorite?: boolean;
  filters?: string[];
  sortBy?: string;
  sortOrder?: 'Ascending' | 'Descending';
  limit?: number;
  startIndex?: number;
  fields?: string[];
  videoTypes?: string[];
  hasSubtitles?: boolean;
  enableImages?: boolean;
  enableUserData?: boolean;
  imageTypeLimit?: number;
  enableImageTypes?: string[];
  enableTotalRecordCount?: boolean;
  nameStartsWith?: string;
  nameLessThan?: string;
}

export interface LatestQueryOptions {
  parentId?: string;
  includeTypes?: string[];
  isPlayed?: boolean;
  limit?: number;
  groupItems?: boolean;
  fields?: string[];
  enableImages?: boolean;
  enableUserData?: boolean;
  imageTypeLimit?: number;
  enableImageTypes?: string[];
}

export interface SimilarQueryOptions {
  limit?: number;
  sortBy?: string;
  fields?: string[];
  enableImages?: boolean;
  enableUserData?: boolean;
  imageTypeLimit?: number;
  enableImageTypes?: string[];
}

export interface NextUpQueryOptions {
  seriesId?: string;
  parentId?: string;
  startIndex?: number;
  limit?: number;
  fields?: string[];
  enableImages?: boolean;
  enableUserData?: boolean;
  imageTypeLimit?: number;
  enableImageTypes?: string[];
  enableTotalRecordCount?: boolean;
}

export interface ShowEpisodesQueryOptions {
  season?: number;
  seasonId?: string;
  isMissing?: boolean;
  adjacentTo?: string;
  startItemId?: string;
  startIndex?: number;
  limit?: number;
  sortBy?: string;
  fields?: string[];
  enableImages?: boolean;
  enableUserData?: boolean;
  imageTypeLimit?: number;
  enableImageTypes?: string[];
}

export interface ShowSeasonsQueryOptions {
  isSpecialSeason?: boolean;
  isMissing?: boolean;
  adjacentTo?: string;
  fields?: string[];
  enableImages?: boolean;
  enableUserData?: boolean;
  imageTypeLimit?: number;
  enableImageTypes?: string[];
}

export interface ResumeQueryOptions {
  parentId?: string;
  includeTypes?: string[];
  excludeActiveSessions?: boolean;
  startIndex?: number;
  limit?: number;
  fields?: string[];
  enableImages?: boolean;
  enableUserData?: boolean;
  imageTypeLimit?: number;
  enableImageTypes?: string[];
  enableTotalRecordCount?: boolean;
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
  onUnauthorized: (() => void) | null = null;

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

  async restartServer() {
    return this.request<void>('/System/Restart', { method: 'POST' });
  }

  async shutdownServer() {
    return this.request<void>('/System/Shutdown', { method: 'POST' });
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

  async brandingConfiguration() {
    return this.request<BrandingConfiguration>('/Branding/Configuration');
  }

  async updateBrandingConfiguration(payload: BrandingConfiguration) {
    return this.request<BrandingConfiguration>('/Branding/Configuration', {
      method: 'POST',
      body: payload
    });
  }

  async playbackConfiguration() {
    return this.request<PlaybackConfiguration>('/System/Configuration/Playback');
  }

  async updatePlaybackConfiguration(payload: PlaybackConfiguration) {
    return this.request<PlaybackConfiguration>('/System/Configuration/Playback', {
      method: 'POST',
      body: payload
    });
  }

  async networkConfiguration() {
    return this.request<NetworkConfiguration>('/System/Configuration/Network');
  }

  async updateNetworkConfiguration(payload: NetworkConfiguration) {
    return this.request<NetworkConfiguration>('/System/Configuration/Network', {
      method: 'POST',
      body: payload
    });
  }

  async libraryDisplayConfiguration() {
    return this.request<LibraryDisplayConfiguration>('/System/Configuration/LibraryDisplay');
  }

  async updateLibraryDisplayConfiguration(payload: LibraryDisplayConfiguration) {
    return this.request<LibraryDisplayConfiguration>('/System/Configuration/LibraryDisplay', {
      method: 'POST',
      body: payload
    });
  }

  async subtitleDownloadConfiguration() {
    return this.request<SubtitleDownloadConfiguration>('/System/Configuration/SubtitleDownload');
  }

  async updateSubtitleDownloadConfiguration(payload: SubtitleDownloadConfiguration) {
    return this.request<SubtitleDownloadConfiguration>('/System/Configuration/SubtitleDownload', {
      method: 'POST',
      body: payload
    });
  }

  async scheduledTasks() {
    return this.request<ScheduledTaskInfo[]>('/ScheduledTasks');
  }

  async scheduledTask(taskId: string) {
    return this.request<ScheduledTaskInfo>(`/ScheduledTasks/${encodeURIComponent(taskId)}`);
  }

  async startScheduledTask(taskId: string) {
    return this.request<void>(`/ScheduledTasks/Running/${encodeURIComponent(taskId)}`, {
      method: 'POST'
    });
  }

  async cancelScheduledTask(taskId: string) {
    return this.request<void>(`/ScheduledTasks/Running/${encodeURIComponent(taskId)}/Cancel`, {
      method: 'POST'
    });
  }

  async updateScheduledTaskTriggers(taskId: string, triggers: ScheduledTaskTrigger[]) {
    return this.request<void>(`/ScheduledTasks/${encodeURIComponent(taskId)}/Triggers`, {
      method: 'POST',
      body: triggers
    });
  }

  async listAuthKeys() {
    return this.request<QueryResult<ApiKeyInfo>>('/Auth/Keys');
  }

  async createAuthKey(options: { app?: string; appVersion?: string; expiresInDays?: number } = {}) {
    const params = new URLSearchParams();
    if (options.app) params.set('App', options.app);
    if (options.appVersion) params.set('AppVersion', options.appVersion);
    if (options.expiresInDays) params.set('ExpiresInDays', String(options.expiresInDays));
    const query = params.toString();
    return this.request<ApiKeyInfo>(`/Auth/Keys${query ? `?${query}` : ''}`, {
      method: 'POST'
    });
  }

  async deleteAuthKey(key: string) {
    return this.request<void>(`/Auth/Keys/${encodeURIComponent(key)}`, {
      method: 'DELETE'
    });
  }

  async listPlaylists() {
    return this.request<QueryResult<PlaylistInfo>>('/Playlists');
  }

  async createPlaylist(payload: {
    Name: string;
    MediaType?: string;
    Overview?: string;
    Ids?: string[];
  }) {
    return this.request<PlaylistInfo>('/Playlists', {
      method: 'POST',
      body: payload
    });
  }

  async getPlaylist(id: string) {
    return this.request<PlaylistInfo>(`/Playlists/${encodeURIComponent(id)}`);
  }

  async updatePlaylist(id: string, payload: { Name?: string; Overview?: string | null }) {
    return this.request<PlaylistInfo>(`/Playlists/${encodeURIComponent(id)}`, {
      method: 'POST',
      body: payload
    });
  }

  async deletePlaylist(id: string) {
    return this.request<void>(`/Playlists/${encodeURIComponent(id)}/Delete`, {
      method: 'POST'
    });
  }

  async listPlaylistItems(id: string, options: { StartIndex?: number; Limit?: number } = {}) {
    const params = new URLSearchParams();
    if (options.StartIndex !== undefined) params.set('StartIndex', String(options.StartIndex));
    if (options.Limit !== undefined) params.set('Limit', String(options.Limit));
    const query = params.toString();
    return this.request<QueryResult<BaseItemDto>>(
      `/Playlists/${encodeURIComponent(id)}/Items${query ? `?${query}` : ''}`
    );
  }

  async addPlaylistItems(id: string, itemIds: string[]) {
    const params = new URLSearchParams({ Ids: itemIds.join(',') });
    return this.request<void>(
      `/Playlists/${encodeURIComponent(id)}/Items?${params.toString()}`,
      {
        method: 'POST'
      }
    );
  }

  async removePlaylistItems(id: string, entryIds: string[]) {
    return this.request<void>(`/Playlists/${encodeURIComponent(id)}/Items/Delete`, {
      method: 'POST',
      body: { EntryIds: entryIds }
    });
  }

  async movePlaylistItem(id: string, entryId: string, newIndex: number) {
    return this.request<void>(
      `/Playlists/${encodeURIComponent(id)}/Items/${encodeURIComponent(entryId)}/Move/${newIndex}`,
      { method: 'POST' }
    );
  }

  async forgotPassword(username: string) {
    return this.request<ForgotPasswordResult>('/Users/ForgotPassword', {
      method: 'POST',
      auth: false,
      body: { EnteredUsername: username }
    });
  }

  async forgotPasswordPin(pin: string, newPassword: string) {
    return this.request<ForgotPasswordPinResult>('/Users/ForgotPassword/Pin', {
      method: 'POST',
      auth: false,
      body: { EnteredPin: pin, NewPw: newPassword }
    });
  }

  async users() {
    return this.request<UserDto[]>('/Users');
  }

  async getUser(userId: string) {
    return this.request<UserDto>(`/Users/${encodeURIComponent(userId)}`);
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

  async authProviders() {
    return this.request<Array<{ Id: string; Name: string; Type?: string; IsEnabled?: boolean }>>('/Auth/Providers');
  }

  async userSettings(userId: string) {
    return this.request<UserSettingsResponse>(`/Users/${userId}/Settings`);
  }

  async updateUserSettings(userId: string, configuration: UserConfiguration) {
    return this.request<UserConfiguration>(`/Users/${userId}/Settings`, {
      method: 'POST',
      body: configuration
    });
  }

  async me() {
    return this.request<UserDto>('/Users/Me');
  }

  async sessions() {
    return this.request<SessionInfo[]>('/Sessions');
  }

  async activity(limit = 50, userId?: string) {
    const params = new URLSearchParams({ Limit: String(limit) });
    if (userId) params.set('UserId', userId);
    return this.request<QueryResult<ActivityLogEntry>>(`/System/ActivityLog/Entries?${params}`);
  }

  async serverLogs() {
    return this.request<LogFileDto[]>('/System/Logs');
  }

  async getLogFile(filename: string) {
    return this.request<string>(`/System/Logs/${encodeURIComponent(filename)}`, {
      responseType: 'text'
    });
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
    return this.request<StartupConfiguration>('/Startup/Configuration');
  }

  async updateStartupConfiguration(payload: StartupConfiguration) {
    return this.request<void>('/Startup/Configuration', {
      method: 'POST',
      body: payload
    });
  }

  async updateRemoteAccess(payload: StartupRemoteAccess) {
    return this.request<void>('/Startup/RemoteAccess', {
      method: 'POST',
      body: payload
    });
  }

  async completeStartup() {
    return this.request<void>('/Startup/Complete', {
      method: 'POST'
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
    if (options.years?.length) {
      params.set('Years', options.years.join(','));
    }
    if (options.isFavorite) {
      params.set('IsFavorite', 'true');
    }
    if (options.filters?.length) {
      params.set('Filters', options.filters.join(','));
    }
    if (options.videoTypes?.length) {
      params.set('VideoTypes', options.videoTypes.join(','));
    }
    if (options.hasSubtitles !== undefined) {
      params.set('HasSubtitles', String(options.hasSubtitles));
    }
    if (options.nameStartsWith) {
      params.set('NameStartsWith', options.nameStartsWith);
    }
    if (options.nameLessThan) {
      params.set('NameLessThan', options.nameLessThan);
    }
    if (options.fields?.length) {
      params.set('Fields', options.fields.join(','));
    }
    this.applyDtoOptions(params, options);
    return this.request<QueryResult<BaseItemDto>>(`/Users/${userId}/Items?${params}`);
  }

  async genres(parentId?: string) {
    const params = new URLSearchParams({ Limit: '200' });
    if (parentId) params.set('ParentId', parentId);
    return this.request<QueryResult<BaseItemDto>>(`/Genres?${params}`);
  }

  async item(itemId: string) {
    const userId = this.requireUserId();
    return this.request<BaseItemDto>(`/Users/${userId}/Items/${itemId}`);
  }

  async latest(parentId?: string, limit?: number): Promise<BaseItemDto[]>;
  async latest(options: LatestQueryOptions): Promise<BaseItemDto[]>;
  async latest(
    parentIdOrOptions?: string | LatestQueryOptions,
    limitOrOptions: number | LatestQueryOptions = 12
  ) {
    const userId = this.requireUserId();
    let options: LatestQueryOptions = {};
    if (typeof parentIdOrOptions === 'string' || parentIdOrOptions === undefined) {
      options = typeof limitOrOptions === 'number'
        ? { parentId: parentIdOrOptions, limit: limitOrOptions }
        : { ...limitOrOptions, parentId: parentIdOrOptions ?? limitOrOptions.parentId };
    } else {
      options = parentIdOrOptions;
    }
    const params = new URLSearchParams({
      Limit: String(options.limit ?? 12)
    });
    if (options.parentId) {
      params.set('ParentId', options.parentId);
    }
    if (options.includeTypes?.length) {
      params.set('IncludeItemTypes', options.includeTypes.join(','));
    }
    if (options.isPlayed !== undefined) {
      params.set('IsPlayed', String(options.isPlayed));
    }
    if (options.groupItems !== undefined) {
      params.set('GroupItems', String(options.groupItems));
    }
    if (options.fields?.length) {
      params.set('Fields', options.fields.join(','));
    }
    this.applyDtoOptions(params, options);
    return this.request<BaseItemDto[]>(`/Users/${userId}/Items/Latest?${params}`);
  }

  async playbackInfo(itemId: string) {
    const userId = this.requireUserId();
    const params = new URLSearchParams({
      UserId: userId,
      IsPlayback: 'true'
    });
    return this.request<PlaybackInfoResponse>(`/Items/${itemId}/PlaybackInfo?${params}`, {
      method: 'POST',
      body: {
        DeviceProfile: webPlaybackDeviceProfile()
      }
    });
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

  async scan(waitForCompletion = false, libraryId?: string) {
    const params = new URLSearchParams({
      WaitForCompletion: waitForCompletion ? 'true' : 'false'
    });
    if (libraryId) {
      params.set('LibraryId', libraryId);
    }
    return this.request<ScanSummary | ScanQueuedResponse>(`/api/admin/scan?${params}`, {
      method: 'POST'
    });
  }

  async libraryMediaUpdated(updates: MediaUpdateInfo[]) {
    return this.request<void>('/Library/Media/Updated', {
      method: 'POST',
      body: {
        Updates: updates
      }
    });
  }

  async scanOperation(operationId: string) {
    return this.request<ScanOperation>(`/api/admin/scan/operations/${encodeURIComponent(operationId)}`);
  }

  async scanOperations(limit = 20) {
    const params = new URLSearchParams({ Limit: String(limit) });
    return this.request<ScanOperation[]>(`/api/admin/scan/operations?${params}`);
  }

  async cancelScanOperation(operationId: string) {
    return this.request<ScanOperation>(
      `/api/admin/scan/operations/${encodeURIComponent(operationId)}/cancel`,
      {
        method: 'POST'
      }
    );
  }

  async remoteEmbySources() {
    return this.request<RemoteEmbySource[]>('/api/admin/remote-emby/sources');
  }

  async createRemoteEmbySource(payload: {
    Name: string;
    ServerUrl: string;
    Username: string;
    Password: string;
    TargetLibraryId: string;
    DisplayMode?: 'merge' | 'separate';
    RemoteViewIds?: string[];
    RemoteViews?: RemoteEmbyView[];
    ViewLibraryMap?: Record<string, string>;
    SpoofedUserAgent?: string;
    Enabled?: boolean;
    StrmOutputPath?: string;
    SyncMetadata?: boolean;
    SyncSubtitles?: boolean;
    TokenRefreshIntervalSecs?: number;
    ProxyMode?: 'proxy' | 'redirect';
  }) {
    return this.request<RemoteEmbySource>('/api/admin/remote-emby/sources', {
      method: 'POST',
      body: payload
    });
  }

  async updateRemoteEmbySource(
    sourceId: string,
    payload: {
      Name: string;
      ServerUrl: string;
      Username: string;
      Password?: string;
      TargetLibraryId: string;
      DisplayMode?: 'merge' | 'separate';
      RemoteViewIds?: string[];
      RemoteViews?: RemoteEmbyView[];
      ViewLibraryMap?: Record<string, string>;
      SpoofedUserAgent?: string;
      Enabled?: boolean;
      StrmOutputPath?: string;
      SyncMetadata?: boolean;
      SyncSubtitles?: boolean;
      TokenRefreshIntervalSecs?: number;
      ProxyMode?: 'proxy' | 'redirect';
    }
  ) {
    return this.request<RemoteEmbySource>(`/api/admin/remote-emby/sources/${encodeURIComponent(sourceId)}`, {
      method: 'PUT',
      body: payload
    });
  }

  async previewRemoteEmbyViews(payload: {
    ServerUrl: string;
    Username: string;
    Password: string;
    SpoofedUserAgent?: string;
  }) {
    return this.request<RemoteEmbyPreviewResult>('/api/admin/remote-emby/views/preview', {
      method: 'POST',
      body: payload
    });
  }

  async deleteRemoteEmbySource(sourceId: string) {
    return this.request<void>(`/api/admin/remote-emby/sources/${encodeURIComponent(sourceId)}`, {
      method: 'DELETE'
    });
  }

  async startRemoteEmbySync(sourceId: string) {
    return this.request<RemoteEmbySyncQueuedResponse>(
      `/api/admin/remote-emby/sources/${encodeURIComponent(sourceId)}/sync`,
      {
        method: 'POST'
      }
    );
  }

  async remoteEmbySyncOperation(operationId: string) {
    return this.request<RemoteEmbySyncOperation>(
      `/api/admin/remote-emby/sync/operations/${encodeURIComponent(operationId)}`
    );
  }

  async remoteEmbySyncOperations(limit = 20) {
    const params = new URLSearchParams({ Limit: String(limit) });
    return this.request<RemoteEmbySyncOperation[]>(`/api/admin/remote-emby/sync/operations?${params}`);
  }

  async cancelRemoteEmbySync(operationId: string) {
    return this.request<RemoteEmbySyncOperation>(
      `/api/admin/remote-emby/sync/operations/${encodeURIComponent(operationId)}/cancel`,
      { method: 'POST' }
    );
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

  async deleteItem(itemId: string) {
    return this.request<void>(`/Items/${itemId}/Delete`, { method: 'POST' });
  }

  async refreshItemMetadata(
    itemId: string,
    options?: {
      metadataRefreshMode?: 'Default' | 'FullRefresh' | 'ValidationOnly';
      imageRefreshMode?: 'Default' | 'FullRefresh' | 'ValidationOnly';
      replaceAllMetadata?: boolean;
      replaceAllImages?: boolean;
    }
  ) {
    const params = new URLSearchParams({
      MetadataRefreshMode: options?.metadataRefreshMode || 'FullRefresh',
      ImageRefreshMode: options?.imageRefreshMode || 'FullRefresh',
      ReplaceAllMetadata: String(options?.replaceAllMetadata ?? true),
      ReplaceAllImages: String(options?.replaceAllImages ?? true)
    });
    return this.request<void>(`/Items/${itemId}/Refresh?${params}`, {
      method: 'POST'
    });
  }

  async updateItem(itemId: string, body: Partial<BaseItemDto>) {
    return this.request<void>(`/Items/${encodeURIComponent(itemId)}`, {
      method: 'POST',
      body
    });
  }

  async getMetadataEditor(itemId: string) {
    return this.request<MetadataEditorInfo>(`/Items/${encodeURIComponent(itemId)}/MetadataEditor`);
  }

  async remoteSearchMovie(query: { SearchInfo: { Name: string; Year?: number; ProviderIds?: Record<string, string> } }) {
    return this.request<any[]>('/Items/RemoteSearch/Movie', {
      method: 'POST',
      body: query
    });
  }

  async remoteSearchSeries(query: { SearchInfo: { Name: string; Year?: number; ProviderIds?: Record<string, string> } }) {
    return this.request<any[]>('/Items/RemoteSearch/Series', {
      method: 'POST',
      body: query
    });
  }

  async remoteSearchApply(itemId: string, result: any) {
    return this.request<void>(`/Items/RemoteSearch/Apply/${encodeURIComponent(itemId)}`, {
      method: 'POST',
      body: result
    });
  }

  async searchSubtitles(itemId: string, language: string) {
    return this.request<RemoteSubtitleInfo[]>(
      `/Items/${itemId}/RemoteSearch/Subtitles/${encodeURIComponent(language)}`
    );
  }

  async downloadSubtitle(itemId: string, subtitleId: string) {
    return this.request<void>(
      `/Items/${itemId}/RemoteSearch/Subtitles/${encodeURIComponent(subtitleId)}`,
      { method: 'POST' }
    );
  }

  async listItemImages(itemId: string) {
    return this.request<ImageInfo[]>(`/Items/${itemId}/Images`);
  }

  async listRemoteImages(itemId: string, options?: { type?: string; IncludeAllLanguages?: boolean; limit?: number }) {
    const params = new URLSearchParams();
    if (options?.type) params.set('Type', options.type);
    if (options?.IncludeAllLanguages) params.set('IncludeAllLanguages', 'true');
    if (options?.limit) params.set('Limit', String(options.limit));
    const qs = params.toString();
    return this.request<RemoteImageResult>(`/Items/${itemId}/RemoteImages${qs ? `?${qs}` : ''}`);
  }

  async downloadRemoteImage(itemId: string, imageUrl: string, imageType: string) {
    const params = new URLSearchParams({ ImageUrl: imageUrl, Type: imageType });
    return this.request<void>(`/Items/${itemId}/RemoteImages/Download?${params}`, {
      method: 'POST'
    });
  }

  async uploadItemImage(itemId: string, imageType: string, imageData: Blob) {
    const arrayBuffer = await imageData.arrayBuffer();
    return this.request<void>(`/Items/${itemId}/Images/${imageType}`, {
      method: 'POST',
      headers: { 'Content-Type': imageData.type || 'image/jpeg' },
      rawBody: new Uint8Array(arrayBuffer)
    });
  }

  async deleteItemImage(itemId: string, imageType: string, index?: number) {
    const path = index !== undefined
      ? `/Items/${itemId}/Images/${imageType}/${index}`
      : `/Items/${itemId}/Images/${imageType}`;
    return this.request<void>(path, { method: 'DELETE' });
  }

  async changePassword(userId: string, payload: { CurrentPw?: string; CurrentPassword?: string; NewPw: string }) {
    return this.request<void>(`/Users/${userId}/Password`, {
      method: 'POST',
      body: payload
    });
  }

  async getCollections() {
    return this.request<QueryResult<BaseItemDto>>(`/Items?IncludeItemTypes=BoxSet&Recursive=true&api_key=${encodeURIComponent(this.token)}`);
  }

  async createCollection(name: string, ids?: string[]) {
    const params = new URLSearchParams({ Name: name });
    if (ids?.length) params.set('Ids', ids.join(','));
    return this.request<BaseItemDto>(`/Collections?${params}`, { method: 'POST' });
  }

  async addCollectionItems(collectionId: string, ids: string[]) {
    const params = new URLSearchParams({ Ids: ids.join(',') });
    return this.request<void>(`/Collections/${collectionId}/Items?${params}`, { method: 'POST' });
  }

  async removeCollectionItems(collectionId: string, ids: string[]) {
    const params = new URLSearchParams({ Ids: ids.join(',') });
    return this.request<void>(`/Collections/${collectionId}/Items?${params}`, { method: 'DELETE' });
  }

  itemImageUrl(item: BaseItemDto) {
    return this.imageUrl(item, 'Primary', item.ImageTags?.Primary);
  }

  backdropUrl(item: BaseItemDto) {
    return this.imageUrl(item, 'Backdrop', item.BackdropImageTags?.[0], 0);
  }

  logoUrl(item: BaseItemDto) {
    return this.imageUrl(item, 'Logo', item.ImageTags?.Logo);
  }

  thumbUrl(item: BaseItemDto) {
    return this.imageUrl(item, 'Thumb', item.ImageTags?.Thumb);
  }

  chapterImageUrl(item: BaseItemDto, chapterIndex: number, tag?: string) {
    if (!tag) return '';
    return `${this.baseUrl}/Items/${item.Id}/Images/Chapter/${chapterIndex}?api_key=${encodeURIComponent(this.token)}&tag=${encodeURIComponent(tag)}`;
  }

  userImageUrl(userObj: { Id?: string; PrimaryImageTag?: string } | null | undefined) {
    if (!userObj?.Id || !userObj.PrimaryImageTag) return '';
    return `${this.baseUrl}/Users/${userObj.Id}/Images/Primary?api_key=${encodeURIComponent(this.token)}&tag=${encodeURIComponent(userObj.PrimaryImageTag)}`;
  }

  personImageUrl(person: { Id?: string; PrimaryImageTag?: string }) {
    if (!person.Id || !person.PrimaryImageTag) return '';
    return `${this.baseUrl}/Items/${person.Id}/Images/Primary?api_key=${encodeURIComponent(this.token)}&tag=${encodeURIComponent(person.PrimaryImageTag)}`;
  }

  async getPerson(personId: string): Promise<BaseItemDto> {
    return this.request<BaseItemDto>(`/Persons/${personId}`);
  }

  async getStudio(name: string): Promise<BaseItemDto> {
    return this.request<BaseItemDto>(`/Studios/${encodeURIComponent(name)}`);
  }

  async getStudioItems(name: string, opts?: { limit?: number; startIndex?: number }): Promise<BaseItemDto[]> {
    const params = new URLSearchParams();
    if (opts?.limit) params.set('Limit', String(opts.limit));
    if (opts?.startIndex) params.set('StartIndex', String(opts.startIndex));
    const qs = params.toString();
    return this.request<BaseItemDto[]>(`/Studios/${encodeURIComponent(name)}/Items${qs ? '?' + qs : ''}`);
  }

  async getSpecialFeatures(itemId: string): Promise<BaseItemDto[]> {
    const userId = this.requireUserId();
    return this.request<BaseItemDto[]>(`/Users/${userId}/Items/${itemId}/SpecialFeatures`);
  }

  async getPersonItems(personId: string, opts?: { limit?: number; startIndex?: number }): Promise<BaseItemDto[]> {
    const params = new URLSearchParams();
    if (opts?.limit) params.set('Limit', String(opts.limit));
    if (opts?.startIndex) params.set('StartIndex', String(opts.startIndex));
    const qs = params.toString();
    return this.request<BaseItemDto[]>(`/Persons/${personId}/Items${qs ? '?' + qs : ''}`);
  }

  /**
   * 触发后端从 TMDB 拉取该演员的简介、出生日期、出生地与头像。
   * 若 `replaceAllImages=true` 会强制覆盖已有头像，否则仅在缺失时下载。
   */
  async refreshPerson(personId: string, opts?: { replaceAllImages?: boolean }): Promise<void> {
    const params = new URLSearchParams();
    if (opts?.replaceAllImages) params.set('ReplaceAllImages', 'true');
    const qs = params.toString();
    await this.request<void>(`/Persons/${personId}/Refresh${qs ? '?' + qs : ''}`, {
      method: 'POST',
    });
  }

  /** 批量从 Emby SQLite 用户库导入。Body 见后端 `ImportEmbyUsersRequest`。 */
  async importEmbyUsers(payload: ImportEmbyUsersRequest): Promise<ImportEmbyUsersResponse> {
    return this.request<ImportEmbyUsersResponse>(`/api/admin/users/import-emby`, {
      method: 'POST',
      body: payload as unknown as Record<string, unknown>,
    });
  }

  /** 一次性给一组用户应用相同的 Policy patch（部分字段，只覆盖 patch 内的键）。 */
  async bulkUpdateUserPolicy(payload: {
    UserIds: string[];
    PolicyPatch: Partial<UserPolicy>;
  }): Promise<{ Updated: string[]; Failed: Array<{ Name: string; Error: string }> }> {
    return this.request(`/api/admin/users/policy/bulk`, {
      method: 'POST',
      body: payload as unknown as Record<string, unknown>,
    });
  }

  async similar(itemId: string, limit?: number): Promise<QueryResult<BaseItemDto>>;
  async similar(itemId: string, options: SimilarQueryOptions): Promise<QueryResult<BaseItemDto>>;
  async similar(itemId: string, limitOrOptions: number | SimilarQueryOptions = 24) {
    const userId = this.requireUserId();
    const options: SimilarQueryOptions = typeof limitOrOptions === 'number'
      ? { limit: limitOrOptions }
      : limitOrOptions;
    const params = new URLSearchParams({ Limit: String(options.limit ?? 24), UserId: userId });
    if (options.fields?.length) {
      params.set('Fields', options.fields.join(','));
    }
    if (options.sortBy) {
      params.set('SortBy', options.sortBy);
    }
    this.applyDtoOptions(params, options);
    return this.request<QueryResult<BaseItemDto>>(`/Items/${itemId}/Similar?${params}`);
  }

  async resume(parentId?: string, limit?: number): Promise<QueryResult<BaseItemDto>>;
  async resume(options: ResumeQueryOptions): Promise<QueryResult<BaseItemDto>>;
  async resume(
    parentIdOrOptions?: string | ResumeQueryOptions,
    limitOrOptions: number | ResumeQueryOptions = 24
  ) {
    const userId = this.requireUserId();
    let options: ResumeQueryOptions = {};
    if (typeof parentIdOrOptions === 'string' || parentIdOrOptions === undefined) {
      options = typeof limitOrOptions === 'number'
        ? { parentId: parentIdOrOptions, limit: limitOrOptions }
        : { ...limitOrOptions, parentId: parentIdOrOptions ?? limitOrOptions.parentId };
    } else {
      options = parentIdOrOptions;
    }
    const params = new URLSearchParams({ Limit: String(options.limit ?? 24) });
    if (options.parentId) params.set('ParentId', options.parentId);
    if (options.startIndex !== undefined) {
      params.set('StartIndex', String(options.startIndex));
    }
    if (options.includeTypes?.length) {
      params.set('IncludeItemTypes', options.includeTypes.join(','));
    }
    if (options.excludeActiveSessions !== undefined) {
      params.set('ExcludeActiveSessions', String(options.excludeActiveSessions));
    }
    if (options.fields?.length) {
      params.set('Fields', options.fields.join(','));
    }
    this.applyDtoOptions(params, options);
    return this.request<QueryResult<BaseItemDto>>(`/Users/${userId}/Items/Resume?${params}`);
  }

  async nextUp(seriesId: string, limit?: number): Promise<QueryResult<BaseItemDto>>;
  async nextUp(options: NextUpQueryOptions): Promise<QueryResult<BaseItemDto>>;
  async nextUp(
    seriesIdOrOptions: string | NextUpQueryOptions,
    limit = 1
  ) {
    const userId = this.requireUserId();
    const options: NextUpQueryOptions = typeof seriesIdOrOptions === 'string'
      ? { seriesId: seriesIdOrOptions, limit }
      : seriesIdOrOptions;
    const params = new URLSearchParams({
      UserId: userId,
      Limit: String(options.limit ?? 1)
    });
    if (options.seriesId) {
      params.set('SeriesId', options.seriesId);
    }
    if (options.parentId) {
      params.set('ParentId', options.parentId);
    }
    if (options.startIndex !== undefined) {
      params.set('StartIndex', String(options.startIndex));
    }
    if (options.fields?.length) {
      params.set('Fields', options.fields.join(','));
    }
    this.applyDtoOptions(params, options);
    return this.request<QueryResult<BaseItemDto>>(`/Shows/NextUp?${params}`);
  }

  async showSeasons(seriesId: string, options: ShowSeasonsQueryOptions = {}) {
    const userId = this.requireUserId();
    const params = new URLSearchParams({ UserId: userId });
    if (options.isSpecialSeason !== undefined) {
      params.set('IsSpecialSeason', String(options.isSpecialSeason));
    }
    if (options.isMissing !== undefined) {
      params.set('IsMissing', String(options.isMissing));
    }
    if (options.adjacentTo) {
      params.set('AdjacentTo', options.adjacentTo);
    }
    if (options.fields?.length) {
      params.set('Fields', options.fields.join(','));
    }
    this.applyDtoOptions(params, options);
    return this.request<QueryResult<BaseItemDto>>(`/Shows/${seriesId}/Seasons?${params}`);
  }

  async showEpisodes(seriesId: string, options: ShowEpisodesQueryOptions = {}) {
    const userId = this.requireUserId();
    const params = new URLSearchParams({ UserId: userId });
    if (options.season !== undefined) {
      params.set('Season', String(options.season));
    }
    if (options.seasonId) {
      params.set('SeasonId', options.seasonId);
    }
    if (options.isMissing !== undefined) {
      params.set('IsMissing', String(options.isMissing));
    }
    if (options.adjacentTo) {
      params.set('AdjacentTo', options.adjacentTo);
    }
    if (options.startItemId) {
      params.set('StartItemId', options.startItemId);
    }
    if (options.startIndex !== undefined) {
      params.set('StartIndex', String(options.startIndex));
    }
    if (options.limit !== undefined) {
      params.set('Limit', String(options.limit));
    }
    if (options.sortBy) {
      params.set('SortBy', options.sortBy);
    }
    if (options.fields?.length) {
      params.set('Fields', options.fields.join(','));
    }
    this.applyDtoOptions(params, options);
    return this.request<QueryResult<BaseItemDto>>(`/Shows/${seriesId}/Episodes?${params}`);
  }

  private applyDtoOptions(
    params: URLSearchParams,
    options: {
      enableImages?: boolean;
      enableUserData?: boolean;
      imageTypeLimit?: number;
      enableImageTypes?: string[];
      enableTotalRecordCount?: boolean;
    }
  ) {
    if (options.enableImages !== undefined) {
      params.set('EnableImages', String(options.enableImages));
    }
    if (options.enableUserData !== undefined) {
      params.set('EnableUserData', String(options.enableUserData));
    }
    if (options.imageTypeLimit !== undefined) {
      params.set('ImageTypeLimit', String(options.imageTypeLimit));
    }
    if (options.enableImageTypes?.length) {
      params.set('EnableImageTypes', options.enableImageTypes.join(','));
    }
    if (options.enableTotalRecordCount !== undefined) {
      params.set('EnableTotalRecordCount', String(options.enableTotalRecordCount));
    }
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

  hlsUrlForSource(
    itemId: string,
    source?: NonNullable<BaseItemDto['MediaSources']>[number],
    playSessionId?: string
  ) {
    const transcodingUrl = source?.TranscodingUrl;
    if (transcodingUrl) {
      return this.absoluteUrlWithOptionalApiKey(transcodingUrl);
    }
    if (source && source.SupportsTranscoding === false) {
      return '';
    }

    const params = new URLSearchParams();
    if (source?.Id) {
      params.set('MediaSourceId', source.Id);
    }
    if (playSessionId) {
      params.set('PlaySessionId', playSessionId);
    }
    params.set('DeviceId', getDeviceId());
    if (this.token) {
      params.set('api_key', this.token);
    }
    return `${this.baseUrl}/Videos/${itemId}/master.m3u8?${params}`;
  }

  streamUrlForSource(source: NonNullable<BaseItemDto['MediaSources']>[number]) {
    const directUrl = source.DirectStreamUrl;
    if (!directUrl) {
      return '';
    }

    const absoluteUrl = this.absoluteUrlWithOptionalApiKey(directUrl, source.AddApiKeyToDirectStreamUrl !== false);
    if (!absoluteUrl) {
      return '';
    }

    return absoluteUrl;
  }

  private absoluteUrlWithOptionalApiKey(pathOrUrl: string, appendApiKey = true) {
    const absoluteUrl = /^https?:\/\//i.test(pathOrUrl) ? pathOrUrl : `${this.baseUrl}${pathOrUrl}`;
    if (!appendApiKey || !this.token) {
      return absoluteUrl;
    }

    // PlaybackInfo 可能已经内嵌 api_key，避免重复拼接导致后端按非法查询参数拒绝。
    if (/[?&]api_key=/i.test(absoluteUrl)) {
      return absoluteUrl;
    }

    const joiner = absoluteUrl.includes('?') ? '&' : '?';
    return `${absoluteUrl}${joiner}api_key=${encodeURIComponent(this.token)}`;
  }

  subtitleUrl(deliveryUrl?: string) {
    if (!deliveryUrl) {
      return '';
    }

    const joiner = deliveryUrl.includes('?') ? '&' : '?';
    return `${this.baseUrl}${deliveryUrl}${joiner}api_key=${encodeURIComponent(this.token)}`;
  }

  async getMediaSegments(itemId: string) {
    return this.request<{ Items: Array<{
      Id: string; Type: string; StartTicks: number; EndTicks: number;
    }>; TotalRecordCount: number }>(`/MediaSegments/${itemId}`);
  }

  async getTrickplayInfo(itemId: string) {
    return this.request<{ Resolutions: Record<string, {
      Width: number; Height: number; TileWidth: number; TileHeight: number;
      TileCount: number; Interval: number; Bandwidth: number;
    }> }>(`/Items/${itemId}/Trickplay`);
  }

  trickplayTileUrl(itemId: string, width: number, tileIndex: number) {
    return `${this.baseUrl}/Videos/${itemId}/Trickplay/${width}/${tileIndex}.jpg`;
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
    if (!options.rawBody) {
      headers.set('Content-Type', 'application/json');
    }
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

    let fetchBody: BodyInit | undefined;
    if (options.rawBody) {
      fetchBody = options.rawBody;
    } else if (options.body) {
      fetchBody = JSON.stringify(options.body);
    }

    const response = await fetch(`${normalizeBaseUrl(baseUrl)}${path}`, {
      method: options.method || 'GET',
      headers,
      body: fetchBody
    });

    if (!response.ok) {
      const text = await response.text();
      if (options.auth !== false && (response.status === 401 || response.status === 403)) {
        this.logout();
        this.onUnauthorized?.();
      }
      throw new Error(text || `HTTP ${response.status}`);
    }

    if (response.status === 204) {
      return undefined as T;
    }

    if (options.responseType === 'text') {
      return (await response.text()) as T;
    }

    return (await response.json()) as T;
  }
}

interface RequestOptions {
  method?: string;
  headers?: HeadersInit;
  body?: unknown;
  rawBody?: BodyInit;
  auth?: boolean;
  responseType?: 'json' | 'text';
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

function webPlaybackDeviceProfile() {
  return {
    MaxStaticBitrate: 200_000_000,
    MaxStreamingBitrate: 200_000_000,
    DirectPlayProfiles: [
      { Type: 'Video' },
      { Type: 'Audio' }
    ],
    TranscodingProfiles: [
      {
        Container: 'ts',
        Type: 'Video',
        AudioCodec: 'aac,mp3,ac3,eac3,opus',
        VideoCodec: 'h264',
        Context: 'Streaming',
        Protocol: 'hls',
        MaxAudioChannels: '6',
        MinSegments: '1',
        BreakOnNonKeyFrames: true,
        ManifestSubtitles: 'vtt'
      }
    ],
    ContainerProfiles: [],
    SubtitleProfiles: [
      { Format: 'vtt', Method: 'External' },
      { Format: 'webvtt', Method: 'External' },
      { Format: 'srt', Method: 'External' },
      { Format: 'ass', Method: 'External' },
      { Format: 'ssa', Method: 'External' },
      { Format: 'subrip', Method: 'Embed' },
      { Format: 'srt', Method: 'Embed' },
      { Format: 'ass', Method: 'Embed' },
      { Format: 'ssa', Method: 'Embed' },
      { Format: 'vtt', Method: 'Hls' }
    ]
  };
}

function normalizeBaseUrl(baseUrl: string) {
  const value = baseUrl.trim();
  if (!value) {
    return '';
  }

  return value.replace(/\/(emby|mediabrowser)\/?$/i, '').replace(/\/$/, '');
}
