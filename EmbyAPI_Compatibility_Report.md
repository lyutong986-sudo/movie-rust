# Emby API 兼容性审计报告

**项目**: Movie Rust  
**更新时间**: 2026-04-22  
**审计目标**: 对照本地播放器模板、Emby 模板和 EmbySDK/OpenAPI，持续确认后端端点、请求参数、响应字段和真实数据来源是否足够兼容 Emby 客户端。  
**本轮结论**: 电影、电视剧、播放、筛选、图片、用户数据主链路继续保持可用。本轮继续修补播放兼容层，重点增强 `PlaybackInfo.TranscodingInfo` 的 EmbySDK 字段、按真实触发条件生成 `TranscodeReasons`、让 `MaxAudioChannels` 不依赖 `DeviceProfile` 也能生效，并扩展常见 `DeviceProfile` 条件属性。仍未宣称完整 Emby：真实转码生命周期、完整 WebSocket 推送、非影视域模型、直播/频道/同步等仍需要独立数据模型和功能链路。

## 对照来源

- 本地播放器 API 调用: `模板项目/本地播放器模板/packages/lin_player_server_api/lib/services/emby_api.dart`
- 本地播放器页面调用: `模板项目/本地播放器模板/lib/play_network_page*.dart`、`show_detail_page.dart`、`desktop_detail_page.dart`
- Emby 模板: `模板项目/Emby模板`
- Emby SDK/OpenAPI: `模板项目/EmbySDK/Documentation/Download/openapi_v2_noversion.json`
- 当前后端路由: `backend/src/routes/*.rs`
- 当前 DTO/查询模型: `backend/src/models.rs`
- 当前数据库 DTO 组装: `backend/src/repository.rs`

## 本地播放器主链路覆盖

### 认证与服务器

- `POST /Users/AuthenticateByName`
- `GET /Users/Me`
- `GET /System/Info/Public`
- `GET /System/Info`
- `GET /System/Ext/ServerDomains`

状态: 已覆盖。服务器地址、远程访问、品牌与系统信息来自真实配置。

### 媒体库与首页

- `GET /Users/{userId}/Views`
- `GET /Items/Counts`
- `GET /Users/{userId}/Items/Counts`
- `GET /Users/{userId}/Items`
- `GET /Users/{userId}/Items/Latest`
- `GET /Users/{userId}/Items/Resume`
- `GET /Users/{userId}/Suggestions`
- `GET /Users/{userId}/HomeSections`

状态: 已覆盖。`ParentId == userId` 的根目录查询按客户端习惯兼容处理，避免把用户 ID 当成普通媒体父级导致首页空列表。

### 列表、搜索与筛选

- `GET /Users/{userId}/Items`
- `GET /Items/Filters`
- `GET /Users/{userId}/Items/Filters`
- `GET /Genres`
- `GET /Users/{userId}/Genres`
- `GET /Studios`
- `GET /Years`
- `GET /Tags`
- `GET /OfficialRatings`
- `GET /Containers`
- `GET /AudioCodecs`
- `GET /VideoCodecs`
- `GET /SubtitleCodecs`
- `GET /Artists`

状态: 已覆盖核心并返回真实聚合值。`ItemsQuery` 已建模并应用大量 SDK 参数，包括 `MediaTypes`、`VideoTypes`、`ImageTypes`、`Genres`、`OfficialRatings`、`Tags`、`Years`、`PersonIds`、`PersonTypes`、`Artists`、`ArtistIds`、`Albums`、`Studios`、`StudioIds`、`Containers`、`AudioCodecs`、`VideoCodecs`、`SubtitleCodecs`、`NameStartsWith`、`NameStartsWithOrGreater`、`NameLessThan`、`IsPlayed`、`IsFavorite`、`IsHD`、`HasSubtitles`、`HasTrailer`、`HasTmdbId`、`HasImdbId`、日期范围和 `SeriesStatus`。

`ProjectToMedia` 当前语义为排除 `CollectionFolder`、`Folder`、`BoxSet` 等文件夹式项目，避免播放器请求媒体投影时混入虚拟目录。它已可用，但还未实现 Emby 全量投影字段裁剪。

### 电视剧

- `GET /Shows/{seriesId}/Seasons`
- `GET /Shows/{seriesId}/Episodes`
- `GET /Seasons/{seasonId}/Episodes`
- `GET /Shows/NextUp`
- `GET /Shows/Missing`
- `GET /Shows/Upcoming`

状态: 已覆盖核心。`Shows/NextUp` 已按剧集归属分组返回每部剧下一集。`Shows/Missing` 使用 `series_episode_catalog` 的真实缺集目录。`Shows/Upcoming` 使用实际 `premiere_date`。本轮已补充三个列表端点的 SDK 查询过滤、分页和常用响应裁剪语义，包括媒体类型、图片类型、类型筛选、年份、评分、用户播放状态、收藏、HD、字幕、日期范围、搜索词、`EnableImages=false`、`ImageTypeLimit=0`、`EnableImageTypes` 和 `EnableUserData=false`。

仍需补强: 更完整的 `Fields` 字段投影、按字段级别精确裁剪 DTO、更多 Show 专属 SDK 参数语义。

### 详情、相似内容与人物

- `GET /Users/{userId}/Items/{itemId}`
- `GET /Items/{itemId}`
- `GET /Users/{userId}/Items/{itemId}/Similar`
- `GET /Items/{itemId}/Similar`
- `GET /Movies/{itemId}/Similar`
- `GET /Shows/{itemId}/Similar`
- `GET /Persons`
- `GET /Persons/{personId}`
- `GET /Persons/{personId}/Items`

状态: 已覆盖核心。`BaseItemDto` 已包含播放器核心字段，例如 `Id`、`Name`、`Type`、`ParentId`、`Overview`、`CommunityRating`、`OfficialRating`、`PremiereDate`、`ProductionYear`、`Status`、`Genres`、`GenreItems`、`Tags`、`RunTimeTicks`、`Size`、`Container`、`ProviderIds`、`SeriesId`、`SeriesName`、`SeasonName`、`ParentIndexNumber`、`IndexNumber`、`ImageTags`、`BackdropImageTags`、父级/剧集图片标签、`UserData`、`People`、`MediaSources`、`MediaStreams`。

已补充的真实长尾字段包括 `PreferredMetadataLanguage`、`PreferredMetadataCountryCode`、`SortIndexNumber`、`SortParentIndexNumber`、`CustomRating`、音乐艺术家/专辑字段、`Video3DFormat`、`CanMakePublic`、`CanManageAccess`、`CanLeaveContent`。

不伪造字段: `SyncStatus`、`CurrentProgram`、完整频道信息、完整同步任务等需要真实模型时才返回。

### 播放

- `GET /Items/{itemId}/PlaybackInfo`
- `POST /Items/{itemId}/PlaybackInfo`
- `GET/HEAD /Videos/{itemId}/stream`
- `GET/HEAD /Videos/{itemId}/stream.{container}`
- `GET/HEAD /Videos/{itemId}/{mediaSourceId}/stream`
- `GET/HEAD /Videos/{itemId}/{mediaSourceId}/stream.{container}`
- `GET/HEAD /Videos/{itemId}/master.m3u8`
- `GET/HEAD /Videos/{itemId}/main.m3u8`
- `GET/HEAD /Videos/{itemId}/hls1/{playlistId}/{segment}`
- `GET/HEAD /Videos/{itemId}/Subtitles/{index}/Stream.{format}`
- `GET/HEAD /Videos/{itemId}/{mediaSourceId}/Subtitles/{index}/Stream.{format}`
- `GET/HEAD /Videos/{itemId}/{mediaSourceId}/Attachments/{index}/Stream`
- `GET /Videos/{itemId}/AdditionalParts`

状态: 已覆盖核心。`PlaybackInfo` 支持 GET/POST、多版本 `MediaSources`、`DirectStreamUrl`、`TranscodingUrl`、`TranscodingContainer`、`TranscodingSubProtocol`、默认音轨/字幕索引、`RequiredHttpHeaders`、`TranscodingInfo`。

本轮修复: `TranscodingInfo` 已补充 `SubProtocol`、`AudioBitrate`、`VideoBitrate`、`TranscodingPositionTicks`、`TranscodingStartPositionTicks`、`AudioChannels` 等 EmbySDK 字段，且只使用当前媒体流和播放请求能真实计算的值。`TranscodeReasons` 不再使用笼统兜底，改为根据真实触发条件生成，例如 `ContainerBitrateExceedsLimit`、`AudioChannelsNotSupported`、`VideoCodecNotSupported`、`AudioCodecNotSupported`、`InterlacedVideoNotSupported`、`SubtitleCodecNotSupported`。`MaxAudioChannels`、`AllowVideoStreamCopy=false`、`AllowAudioStreamCopy=false`、`AllowInterlacedVideoStreamCopy=false` 已脱离 `DeviceProfile` 独立生效。

既有修复: `DeviceProfile.ContainerProfiles`、`CodecProfiles`、`ResponseProfiles`、`SubtitleProfiles` 已改为保留 EmbySDK 对象数组，避免客户端 POST `/PlaybackInfo` 时请求体反序列化失败并忽略设备能力配置。已补充常见 `ContainerProfiles`/`CodecProfiles` 条件判定，覆盖容器、视频/音频 codec、宽高、码率、bit depth、level、profile、video range、音频声道、隔行扫描等常见属性。本轮继续扩展到 ref frames、像素格式、色彩空间、色彩传递、色彩原色、ExtendedVideoType、ExtendedVideoSubType、音频码率、采样率、bit depth、字幕 codec、IsAnamorphic、IsAvc 等属性。

仍需补强: `ResponseProfiles` 输出重写、HDR/Dolby Vision 更细 profile/subtype 策略、字幕 burn-in 输出策略、真实转码任务生命周期、实时进度、硬件加速状态、失败原因和关闭清理。

### 播放上报与用户数据

- `POST /Sessions/Playing`
- `POST /Sessions/Playing/Progress`
- `POST /Sessions/Playing/Stopped`
- `POST /Users/{userId}/Items/{itemId}/UserData`
- `POST /UserItems/{itemId}/UserData`
- `POST /Users/{userId}/Items/{itemId}/HideFromResume`
- `POST /Users/{userId}/FavoriteItems/{itemId}`
- `DELETE /Users/{userId}/FavoriteItems/{itemId}`
- `POST /Users/{userId}/FavoriteItems/{itemId}/Delete`
- `POST /Users/{userId}/PlayedItems/{itemId}`
- `DELETE /Users/{userId}/PlayedItems/{itemId}`

状态: 已覆盖。播放进度、收藏、已播放、继续观看隐藏均写入真实 `user_item_data`。`HideFromResume` 已按本地播放器无 query POST 的行为默认隐藏并清空进度，只有显式 `Hide=false` 才不清理。

### 图片与远程图片

- `GET /Items/{itemId}/Images`
- `GET/HEAD /Items/{itemId}/Images/{type}`
- `GET /Items/{itemId}/Images/{type}/{index}/Url`
- `POST /Items/{itemId}/Images/{type}`
- `DELETE /Items/{itemId}/Images/{type}`
- `GET /Items/{itemId}/RemoteImages`
- `GET /Items/{itemId}/RemoteImages/Providers`
- `POST /Items/{itemId}/RemoteImages/Download`
- `GET /Images/Remote?ImageUrl=...`
- `GET/HEAD /Users/{userId}/Images/{type}`
- `POST /Users/{userId}/Images/{type}`
- `DELETE /Users/{userId}/Images/{type}`
- `GET/HEAD /Persons/{name}/Images/{type}`
- `GET/HEAD /Artists/{name}/Images/{type}`
- `GET/HEAD /Genres/{name}/Images/{type}`

状态: 图片读取、上传、删除、远程下载已覆盖。`RemoteImages` 会返回本地已存在远程图片，并在有 TMDB provider 与 provider id 时聚合 TMDB 候选图。

本轮修复: `RemoteImages` 支持 `Language`/`language` 查询参数，并按 `zh-CN`、`zh`、`en-US`、`en` 等语言前缀匹配候选图。未指定语言且未开启 `IncludeAllLanguages` 时，保留中文、英文和无语言图片。

仍需补强: 更多远程 provider、远程图片评分排序策略与 Emby 完全一致、按 provider/type/language 的更细管理端体验。

### 章节与片头

- `GET /Items/{itemId}/Chapters`
- `GET /Episodes/{itemId}/IntroTimestamps`
- `GET /Items/{itemId}/IntroTimestamps`
- `GET /Videos/{itemId}/IntroTimestamps`

状态: 已覆盖。响应兼容本地播放器 `IntroTimestamps.tryParse` 的多字段读取。

### Sessions、远控、WebSocket

- `GET /Sessions`
- `POST /Sessions/Capabilities`
- `POST /Sessions/Capabilities/Full`
- `GET /Sessions/PlayQueue`
- `GET /Sessions/{id}/Commands`
- `POST /Sessions/{id}/Command`
- `POST /Sessions/{id}/Command/{command}`
- `POST /Sessions/{id}/Message`
- `POST /Sessions/{id}/Viewing`
- `GET /embywebsocket`

状态: Sessions 摘要、播放状态、capabilities、播放队列、远控命令队列已有真实持久化。`DisplayMessage`、`SetAudioStreamIndex`、`SetSubtitleStreamIndex`、`SetVolume`、`SetAdditionalUser` 等命令已落到 session 摘要状态。

仍需补强: WebSocket 入口存在，但不是完整 Emby 原生实时推送模型。

## 本轮修复记录

### 2026-04-22 本轮

- 补充 `TranscodingInfoDto` 的 EmbySDK 字段: `SubProtocol`、`AudioBitrate`、`VideoBitrate`、`TranscodingPositionTicks`、`TranscodingStartPositionTicks`、`AudioChannels`。
- 将 `TranscodeReasons` 改为根据真实播放请求和媒体流条件生成，减少笼统兜底原因。
- 修复 `MaxAudioChannels`、`AllowVideoStreamCopy=false`、`AllowAudioStreamCopy=false`、`AllowInterlacedVideoStreamCopy=false` 只有传入 `DeviceProfile` 时才生效的问题。
- 扩展 `DeviceProfile` 条件属性判定，增加 HDR/色彩/像素格式/字幕/音频采样等常见属性读取。
- 增加 `transcoding_info_reports_real_reasons_and_sdk_fields` 单元测试。
- 修复 `DeviceProfile.ContainerProfiles`、`CodecProfiles`、`ResponseProfiles`、`SubtitleProfiles` 的对象数组反序列化，避免 EmbySDK 客户端 POST `/PlaybackInfo` 被忽略。
- 为 `PlaybackInfo` 增加 `playback_info_accepts_emby_sdk_profile_object_arrays` 单元测试。
- 深化 `DeviceProfile` 条件判定，开始评估常见容器、codec、分辨率、码率、bit depth、level、profile、video range 和音频声道条件。
- 为 `DeviceProfile` 条件判定增加 `device_profile_conditions_evaluate_stream_properties` 单元测试。
- 为 `RemoteImages` 增加 `Language`/`language` 参数过滤，支持语言前缀匹配。
- 为 `Shows/NextUp`、`Shows/Missing`、`Shows/Upcoming` 补充路由层 SDK 查询过滤、排序和分页，避免这些端点只返回核心查询结果而忽略客户端筛选条件。
- 为 `Shows/NextUp`、`Shows/Missing`、`Shows/Upcoming` 增加常用响应裁剪，支持 `EnableImages=false`、`ImageTypeLimit=0`、`EnableImageTypes` 和 `EnableUserData=false`。
- 重新运行后端检查和针对性单元测试。

### 2026-04-22 既有修复

- 修复 `UserData` 已播放语义，写入真实 `LastPlayedDate`，已播放时归零 `PlaybackPositionTicks`。
- 修复取消已播放时清理 `LastPlayedDate`。
- 补充 `BaseItemDto` 长尾真实字段。
- `SyncStatus` 未伪造；项目没有真实同步模型时不返回。
- 修复 `Shows/NextUp` 按剧集返回下一集。
- 修复 `HideFromResume` 默认隐藏语义并补充单元测试。
- 修复未探测媒体源 fallback `DirectStreamUrl`。
- 修复 `PlaybackInfo` 转码 URL 生成，优先使用客户端 `DeviceProfile.TranscodingProfiles.Protocol/Container`。

## 当前兼容状态矩阵

| 模块 | 本地播放器主链路 | EmbySDK 覆盖度 | 当前状态 |
| --- | --- | --- | --- |
| 认证 Users | 已覆盖 | 中高 | 认证、用户列表、策略、密码修改可用；外部认证 provider 仍轻量 |
| System | 已覆盖 | 中高 | Info、Endpoint、Domains、Branding、Logs、ActivityLog 使用真实配置/日志 |
| 媒体库 Views | 已覆盖 | 中高 | 媒体库、Counts、Root、MediaFolders 可用 |
| Items 查询 | 已覆盖 | 高 | 大量过滤参数已建模并应用；`ProjectToMedia` 已排除虚拟目录，但全量字段投影仍未实现 |
| Filters/Genres | 已覆盖 | 高 | 真实聚合筛选值和辅助端点可用 |
| BaseItemDto | 已覆盖核心 | 中高 | 电影/剧集核心字段完整；直播、频道、同步、额外媒体域仍缺真实模型 |
| 电视剧 | 已覆盖核心 | 中高 | Season/Episode/NextUp/Missing/Upcoming 可用；本轮补充列表过滤、分页和常用响应裁剪 |
| PlaybackInfo | 已覆盖核心 | 中高 | DirectStream/Transcoding 基础可用；本轮补充 TranscodingInfo SDK 字段、真实转码原因和更多 profile 条件属性 |
| Videos/Streams | 已覆盖核心 | 中 | 直链、STRM 代理、HLS 入口、字幕和附件流可用；真实转码生命周期待深化 |
| Sessions | 已覆盖核心 | 中 | 上报、状态、队列、命令可用；实时 WebSocket 推送待补 |
| Images | 已覆盖核心 | 中高 | 本地图片、上传删除、TMDB 候选图和语言过滤可用 |
| Persons | 部分使用 | 中 | 人物列表、详情、作品关联可用；远程 credits 落库仍可增强 |
| DisplayPreferences | 客户端可能调用 | 中 | 持久化 GET/POST 可用；完整客户端布局偏好模型待扩展 |
| Localization/UserSettings | 客户端可能调用 | 中 | 使用真实启动配置/用户配置；字段集仍轻量 |
| Auth Keys | 管理端可能调用 | 中 | 基于 sessions 的 key 管理可用；权限、过期和审计策略待细化 |
| 音乐/直播/频道/BoxSet | 非本地播放器主链路 | 低到中 | 部分兼容，不是完整 Emby 域模型 |

## 真实数据来源说明

- 媒体条目: `media_items`
- 媒体流和章节: `media_streams`、`media_chapters`
- 缺集目录: `series_episode_catalog`
- 用户进度、收藏、已播放: `user_item_data`
- 用户、策略、配置: `users` 与系统配置表
- 会话和远控: `sessions`、session capabilities、play queue、command queue 相关表
- 图片: 媒体图片路径、用户头像路径、人物/类型图片路径、TMDB 远程候选图
- 元数据: NFO、路径 provider id、TMDB provider、扫描器落库字段
- 系统配置: 启动配置、远程访问配置、branding 配置、日志目录

原则: 没有真实模型的字段不伪造业务状态。例如真实 `SyncStatus`、直播节目 `CurrentProgram`、完整频道数据、完整同步任务等，在模型缺失时不返回或返回真实空集合。

## 已知缺口和风险

### P0: 直接影响播放的剩余缺口

- `TranscodingInfo` 已补更多 SDK 字段和真实触发原因，但仍缺少真实转码任务生命周期、实时进度、硬件加速状态、失败原因、转码会话关闭清理等完整链路。
- HLS playlist 当前是兼容入口，仍需要和实际转码器输出、segment 缓存、Range/seek 行为做更深整合。

### P1: 影响筛选、剧集和详情完整度的缺口

- `DeviceProfile` 已能完整接收 EmbySDK 对象数组，并已支持更多 `ContainerProfiles`、`CodecProfiles` 条件属性；仍需继续深化 `ResponseProfiles` 输出重写、HDR/DV subtype 的精细策略、字幕格式与 burn-in 输出策略。
- `ProjectToMedia` 已处理虚拟目录排除，但还未做 Emby 全量字段裁剪投影。
- `Shows/Missing`、`Shows/Upcoming`、`Shows/NextUp` 已补常见过滤分页和图片/用户数据裁剪；仍需补完整 `Fields` 字段投影。
- `BaseItemDto` 仍缺少需要独立模型支撑的字段，如 `ChannelId`、`ChannelName`、`CurrentProgram`、`ExtraType`、`Subviews`、真实 `SyncStatus`、直播字段。
- 人物远程 credits 和作品关联仍可继续增强，尤其是从 TMDB credits 稳定落到 `person_roles` 并反映到人物页/作品页。

### P2: 管理端和完整 Emby 客户端体验缺口

- `RemoteImages` 已支持 TMDB 候选图、分页、provider/type/language 过滤基础，但更多 provider、排序和管理体验仍可继续补。
- `DisplayPreferences`、`Localization`、`UserSettings` 已持久化/配置化，但还不是完整 Emby 客户端布局偏好模型。
- Sessions 远控已有命令队列，但 WebSocket 还不是完整 Emby 原生实时推送。
- Auth Keys 有兼容基础，但 API Key 权限范围、过期策略、审计策略仍需细化。
- Library 管理端点有基础能力，但完整媒体库选项、刮削器选项、刷新任务状态、计划任务模型还不完整。

### P3: 非电影电视剧域

- 音乐、Artist、MusicAlbum、直播、频道、录制、GameGenre 等域只有部分兼容。
- 如果目标是完整 Emby 全量客户端，这些域需要单独设计数据库模型、扫描器、DTO 和路由。

## 后续修复顺序

1. 接入真实转码生命周期: `TranscodingInfo` 进度、ActiveEncodings、关闭/清理、失败原因、硬件加速状态。
2. 深化 `DeviceProfile` 剩余判定: `ResponseProfiles`、HDR/DV subtype、字幕格式与 burn-in、输出容器/codec 重写。
3. 继续复核 `Users/{userId}/Items` 的 `ProjectToMedia` 字段投影和 BoxSet/Collection 行为。
4. 继续补 `Shows/Missing/Upcoming/NextUp` 的完整 `Fields` 字段投影和更多 Show 专属参数。
5. 扩展 `RemoteImages` 更多 provider 和排序策略。
6. 为本地播放器强依赖端点补路由级集成测试: 认证、Views、Items、Filters、Seasons/Episodes、PlaybackInfo、Videos stream、UserData、HideFromResume、FavoriteItems、Images、IntroTimestamps。
7. 推进完整 Emby 管理后台: Library options、任务队列、扫描状态、远程图片管理、DisplayPreferences 模板。
8. 最后展开非影视域: 音乐、直播、频道、录制、BoxSet。

## 验证记录

### 2026-04-23 媒体库目录选择器

- 参考 EmbySDK `EnvironmentService` 补齐管理员目录浏览端点: `/Environment/Drives`、`/Environment/DirectoryContents`、`/Environment/ParentPath`，响应 `FileSystemEntryInfo` 的 `Name`、`Path`、`Type` 字段。
- 添加媒体库弹窗的 `path-editor` 改为可视化目录选择器，支持查看服务器驱动器、逐级进入目录、返回上级并将当前文件夹加入媒体库路径，不再要求手动输入路径。
- 验证通过: `cargo check --manifest-path backend/Cargo.toml`、`npm.cmd run build`。

### 2026-04-23 媒体库管理字段补齐

- 对照 Emby 模板 `libraryoptionseditor` 与线上 `/settings/libraries` 页面，当前项目的媒体库管理已补齐模板里真实使用但此前缺失的库级选项：
  - `EnableInternetProviders`
  - `ImportMissingEpisodes`
- `LibraryOptionsDto`、前端 `LibraryOptions` 类型、默认值和更新请求体已同步补齐这两个字段，旧库配置缺失时会按 Emby 习惯回退到：
  - `EnableInternetProviders = true`
  - `ImportMissingEpisodes = false`
- 媒体库管理页新增“编辑”入口，不再只能看摘要和删除：
  - 支持修改媒体库名称
  - 支持调整路径列表
  - 支持编辑元数据、扫描、电视剧专属、章节图片等库级设置
- 新的媒体库编辑弹窗按内容类型做了 Emby 风格显隐：
  - `homevideos` 不显示互联网元数据语言/地区与预下载图片
  - `tvshows` 额外显示“导入缺失剧集占位条目”和“自动合并同名剧集”
  - `photos` 的本地元数据保存开关预留了显隐逻辑，但由于当前后端尚未正式开放该建库类型，本轮没有在创建下拉中暴露
- 元数据语言不再只是自由输入框，已接后端 `/Localization/Cultures` 生成语言下拉；国家/地区则先提供常用 Emby 场景选项，保证库设置体验更接近模板。
- 本轮优先补齐媒体库管理核心字段与交互，字幕相关库选项按要求暂未扩展。

### 2026-04-23 媒体库管理聚焦电影与剧集

- 按当前项目范围，媒体库创建界面已收敛为只支持：
  - `movies`
  - `tvshows`
- 不再在创建页暴露 `music`、`homevideos`、`mixed`，避免把当前并不打算完成的内容类型继续带入管理流程。
- 继续对照 EmbySDK `LibraryOptions`，为电影库/剧集库补齐一批更接近 Emby 高级设置的字段并打通前后端持久化：
  - `ExcludeFromSearch`
  - `IgnoreHiddenFiles`
  - `SaveMetadataHidden`
  - `MergeTopLevelFolders`
  - `PlaceholderMetadataRefreshIntervalDays`
  - `PreferredImageLanguage`
  - `EnableMultiVersionByFiles`
  - `EnableMultiVersionByMetadata`
  - `EnableMultiPartItems`
  - `ImportCollections`
  - `MinCollectionItems`
- 媒体库编辑弹窗新增更细的电影/剧集分组：
  - 通用扫描与读取：隐藏文件、隐藏元数据、排除搜索、顶层目录合并、多版本/多分段识别、占位条目刷新间隔
  - 剧集专属：缺失剧集占位、自动合并同名剧集、特别篇名称
  - 电影专属：导入电影合集、自动合集最少影片数
- `PreferredImageLanguage` 会在空值时自动回退到 `PreferredMetadataLanguage`；`MinCollectionItems` 在服务端归一化时至少为 `2`，防止旧配置写入无效值。
- 当前这一轮主要完成的是 **Emby 风格字段与管理面板补齐 + 后端配置持久化**；这些新字段的扫描器/索引器实际行为后续还需要逐项向 Emby 继续靠拢。

### 2026-04-23 媒体库添加/删除链路修复

- 修复媒体库名称未唯一化的问题：
  - 新初始化数据库的 `libraries.name` 已直接带 `UNIQUE`
  - 启动兼容 SQL 会在不存在重名库时补建 `lower(name)` 唯一索引
  - 仓储层 `create_library` / `rename_library` 也会主动拒绝重名，避免仅靠数据库报错
- 修复媒体路径跨库冲突：
  - 新建媒体库、更新媒体库路径时，都会校验目标路径是否已被其他媒体库占用
  - 同时拦截“相同目录”以及“父子目录重叠”两类冲突，减少重复导入和镜像内容
- 修复 `refreshLibrary` 被忽略的问题：
  - `/api/admin/libraries`
  - `/api/admin/libraries/{id}`
  - `/Library/VirtualFolders`
  - `/Library/VirtualFolders/Paths`
  - `/Library/VirtualFolders/Paths/Update`
  这些链路现在都会实际读取 `refreshLibrary`，开启后触发库扫描刷新
- 前端创建/删除媒体库时已默认带上 `refreshLibrary=true`，因此新建库后会自动扫描，删除后也会走一次刷新流程，不再要求用户手动再点“扫描所有媒体库”。

### 2026-04-23 人物图片 TMDB 按需缓存

- 对照 Emby 模板可确认服务端图片默认是“被客户端请求时再下载”，而不是让客户端长期直接依赖第三方图链；人物卡片/详情仍会通过 `/Items/{personId}/Images/Primary` 或 `/Persons/{personId}/Images/Primary` 取图。
- 当前后端已改为人物图片首次命中远程 TMDB URL 时，自动下载到 `static_dir/person-images`，并把 `persons.primary_image_path/backdrop_image_path/logo_image_path` 更新为本地缓存文件路径，后续请求直接走本地文件。
- 同时补齐 `/Items/{personId}/Images` 对人物实体的兼容，避免人物 DTO 带 `ImageTags.Primary` 时落到 `媒体条目不存在` 的 404。

### 2026-04-23 TMDB 语言与图片预下载

- TMDB provider 启动时改为使用 `APP_PREFERRED_METADATA_LANGUAGE` 和 `APP_METADATA_COUNTRY_CODE` 组装语言参数，不再固定 `en-US`；扫描媒体库时还会优先使用媒体库自己的 `PreferredMetadataLanguage` / `MetadataCountryCode`。
- 新增并打通 `LibraryOptions.DownloadImagesInAdvance`，前端创建媒体库表单和媒体库设置摘要都已暴露该开关。
- 扫描时若开启 `DownloadImagesInAdvance`：
  - 人物图片继续下载到服务端缓存目录 `static_dir/person-images`
  - 电影/剧集图片在 `SaveLocalMetadata=true` 时写入对应媒体目录；否则回退到 `static_dir/item-images`
- 对照 Jellyfin 后端 `ImageSaver` 后，又把本地图片命名进一步对齐到 Emby/Jellyfin 习惯：
  - 电影/剧集主图保存为 `poster.jpg`
  - 背景图保存为 `fanart.jpg`
  - 横图保存为 `landscape.jpg`
  - 季图保存为 `season01-poster.jpg`、`season01-fanart.jpg`、`season01-logo.jpg`、`season01-landscape.jpg`
  - 集图在本地元数据模式下仅落 `SxxExx-thumb.jpg` 风格主图，不再错误尝试写入额外 backdrop/logo
- 人物图片仍保持走服务端内部缓存目录，这一点也与 Jellyfin 的 `PeoplePath -> InternalMetadataPath/People/...` 设计一致。
- 扫描器本地图片读取规则也已同步补齐：
  - 剧集季级别会识别 `season01-*` / `season-specials-*` 这类 Emby/Jellyfin 常见命名
  - 集级别会额外识别 sidecar `fanart/backdrop/background/logo/clearlogo/thumb/landscape`
  - 这样 `DownloadImagesInAdvance + SaveLocalMetadata` 写回磁盘后的图片，在后续重扫时能稳定重新识别并回填数据库
- 已增加季图命名识别单测，验证 `season01-poster.jpg` 与 `season-specials-fanart.jpg` 两类路径均可命中。

### 2026-04-23 TMDB 链路审计修复

- 修复 `combined_credits` 反序列化兼容性：TMDB 人物作品里的电影 credit 可能只有 `title` 没有 `name`，现在人物作品模型已兼容 movie/tv 混合返回，避免人物作品合集因字段缺失整段失败。
- 修正季/集远程图片行为：当前 provider 在缺少 season/episode 独立 provider id 的前提下，不再错误复用整部剧的 series poster/backdrop 作为季图/集图；`RemoteImages`、预下载和手动下载不会再把剧集图误写到季/集对象。
- 进一步补齐季/集独立取图：
  - Season 现在会使用 `series TMDb id + season number` 调 TMDB season details，提取季海报作为远程图来源
  - Episode 现在会使用 `series TMDb id + season/episode number` 从 TMDB season catalog 中提取 `still_path`，并作为 `Primary/Thumb` 远程图来源
  - `RemoteImages`、扫描预下载、手动下载远程图片三条路径已统一复用这套 child-item 图源逻辑
- 远程人物同步改为“替换式”清理当前条目的 TMDb 人物关系后再重建，减少演员/导演更新后旧角色长期残留的问题。
- 手动“刷新元数据”和“下载远程图片”已对齐扫描链路：
  - 刷新时会按条目所属媒体库的 `PreferredMetadataLanguage` / `MetadataCountryCode` 创建 TMDB provider
  - 下载图片时会遵守 `SaveLocalMetadata`，优先写回媒体目录 sidecar；否则回退到 `static_dir/item-images`
- 统一了人物详情链路中 TMDb/IMDb provider id 的写法与查找兼容，避免手动从外部 provider 拉取人物时因为 key 大小写不一致而无法复用已有记录。

### 2026-04-23 初始化管理员数据库修复

- 初始化用户表 `0001_init.sql` 已直接包含 Emby 用户运行所需字段: `policy`、`configuration`、`primary_image_path`、`backdrop_image_path`、`logo_image_path`、`date_modified`，新项目初始化后可直接创建管理员。
- 删除已并入初始化脚本的 users 补丁迁移: `0007_user_policy.sql`、`0012_user_configuration.sql`、`0015_user_image_fields.sql`，避免初始化项目时重复/半升级导致 `users.configuration` 不存在。
- 启动兼容 SQL 同步补齐 `is_hidden`、`is_disabled`、`policy`、`configuration` 和用户图片字段，已存在的半初始化数据库重启后也会自动修复。
- 验证通过: `cargo check --manifest-path backend/Cargo.toml`。

### 2026-04-23 审计修复

- 修复 `/Users/{userId}/Items`、`/Users/{userId}/Items/Latest`、`/Users/{userId}/Items/{itemId}`、`/Users/{userId}/Items/Resume` 缺少用户访问校验的问题，带用户 ID 的读接口现在要求本人或管理员访问。
- HLS 播放入口改为启动真实 ffmpeg HLS 转码会话，读取转码器生成的 `playlist.m3u8`，并将分片 URL 重写到 `/Videos|Audio/{itemId}/hls1/{sessionId}/{segment}`；segment 请求改为从对应转码会话输出目录读取真实分片，不再把 `0.ts` 伪装成整文件流。
- 转码启动上下文改为使用当前认证用户 ID，并从 `DeviceId` 查询参数或 Emby 授权头读取真实设备 ID，避免所有转码会话落到 nil user/default-device。
- 媒体分析调用 ffprobe 时改为直接传入 `Path` 的 `OsStr`，避免非 UTF-8 本地路径触发 `to_str().unwrap()` panic。
- 验证通过: `cargo check --manifest-path backend/Cargo.toml`。仍存在一批既有 Rust warning，主要为未使用字段/import/未来扩展模型，不阻塞构建。

本轮修复后已通过:

```text
cargo check --manifest-path backend/Cargo.toml
cargo test --manifest-path backend/Cargo.toml playback_info_accepts_emby_sdk_profile_object_arrays -- --nocapture
cargo test --manifest-path backend/Cargo.toml device_profile_conditions_evaluate_stream_properties -- --nocapture
cargo test --manifest-path backend/Cargo.toml transcoding_info_reports_real_reasons_and_sdk_fields -- --nocapture
```

当前仍存在一批既有 Rust warning，主要是未使用 import、未使用字段、未使用辅助函数和部分未来扩展模型；它们不阻塞构建，但建议后续在功能稳定后统一清理。

### 2026-04-23 本地播放器播放 URL 兼容修复

- 对照本地播放器模板 `PlaybackSourceBuilder` 后确认：客户端优先使用 `PlaybackInfo.MediaSources[].DirectStreamUrl`，并会把相对 URL 直接通过 `baseUrl.resolve()` 解析；因此服务端返回 `/Videos/...` 在 `/emby` 前缀部署或代理场景下可能绕过 Emby API 前缀。
- `PlaybackInfo` 现在会把内部 DirectStream URL 装饰为 `/emby/Videos/...`，并补齐 `MediaSourceId`、`mediaSourceId`、`PlaySessionId`、`UserId`、`DeviceId`、`api_key`、`X-Emby-Token`、`X-MediaBrowser-Token` 查询参数，同时继续保留 `RequiredHttpHeaders`，兼容 MPV/Exo/VLC 不同播放核心。
- `TranscodingUrl` 同步改为 `/emby/Videos/...`，并补齐 `api_key`、`UserId`、`DeviceId` 和 Emby token 参数，避免 Exo 触发 HLS/HTTP 转码后拿到无法鉴权或缺少前缀的播放地址。
- 当设备配置或码率/音轨条件触发转码时，被选中的 `MediaSource` 会移除 `DirectStreamUrl`，避免本地播放器“有直连地址就优先直连”的逻辑绕过 `TranscodingUrl`。
- 新增后端单元测试 `playback_info_decorates_direct_stream_urls_for_local_player`，覆盖本地播放器强依赖的 URL 字段。
- 验证通过：
  - `cargo check --manifest-path backend\Cargo.toml`
  - `cargo test --manifest-path backend\Cargo.toml playback_info -- --nocapture`

### 2026-04-23 播放链路审计修复

- `PlaybackInfo` 现在会先检查 `ENABLE_TRANSCODING`：当客户端条件触发转码但服务端关闭转码时，不再返回不可播放的 `TranscodingUrl`，而是保留可用的直连媒体源并返回 `ErrorCode=TranscodingDisabled`，避免播放器请求 `/master.m3u8` 后直接 400。
- `TranscodingUrl` 已补齐 `VideoCodec` / `AudioCodec` 参数，优先使用 `DeviceProfile.TranscodingProfiles` 中的目标编码；HLS 场景缺省会落到 `h264+aac`，转码器也会把 `h264` 映射到 `libx264`、把常见音频编码映射到 ffmpeg 可用 encoder，避免“声明转码但实际 copy 原始 HEVC/TrueHD”的情况。
- `/Sessions/Playing`、`/Sessions/Playing/Progress`、`/Sessions/Playing/Stopped` 以及 legacy `/Users/{userId}/PlayingItems/{itemId}` 上报链路已增加用户访问校验，普通用户只能写自己的播放进度，管理员才可代写其他用户，避免污染他人的继续观看和 UserData。
- `ActiveEncodings` 删除端点现在会解析 `TranscodingJobId` / `EncodingJobId` / `PlaySessionId`，并结合当前用户和 `DeviceId` 停止转码会话；客户端停止播放或切换播放源时，后端会同步取消对应 ffmpeg/HLS 任务。
- 转码并发控制已修复为把 semaphore permit 绑定到 ffmpeg 后台任务生命周期，直到进程完成或会话取消后才释放；取消转码时会发送取消信号并 kill 等待中的 ffmpeg 子进程，避免并发限制形同虚设和残留进程占用资源。
- 验证通过：
  - `cargo check --manifest-path backend\Cargo.toml`
  - `cargo test --manifest-path backend\Cargo.toml playback_info -- --nocapture`
  - `cargo test --manifest-path backend\Cargo.toml api_router_builds_without_route_conflicts -- --nocapture`

### 2026-04-23 转码设置页补齐

- 对照 Emby 模板 `encodingsettings.html` / `encodingsettings.js`，在 `class="settings-nav"` 中新增管理员栏目“转码”，路由为 `/settings/transcoding`。
- 新增前端转码设置页，字段覆盖 Emby 模板核心内容：硬件加速类型、VAAPI 设备、转码线程数、转码限速、ffmpeg 版本/路径、转码临时目录、音频下混增益、H264 编码预设、H264 CRF；同时补充当前后端必需的“启用视频转码”和“最大并发转码数”。
- 转码页的 ffmpeg 路径和转码临时目录支持复用服务端目录浏览弹窗，避免手动填写路径。
- 后端新增 Emby 风格配置接口：
  - `GET /System/Configuration/encoding`
  - `POST /System/Configuration/encoding`
  - `POST /System/MediaEncoder/Path`
- `EncodingOptions` 会持久化到 `system_settings` 的 `encoding` key；缺省值会从现有环境配置回退，包括 `FFMPEG_PATH`、`TRANSCODE_DIR`、`TRANSCODE_THREADS`、`ENABLE_TRANSCODING` 和 `MAX_TRANSCODE_SESSIONS`。
- 播放链路已改为读取持久化后的转码设置：`PlaybackInfo`、普通流转码入口和 HLS playlist 转码入口都会使用新的 `EncodingOptions`，保存页面后即可影响后续转码请求。
- 转码器现在会使用页面保存的 ffmpeg 路径、临时目录、线程数、H264 preset/CRF 和硬件加速 encoder 映射；`nvenc/qsv/vaapi/h264_omx` 会分别映射到对应的 H264 ffmpeg encoder。
- 验证通过：
  - `cargo check --manifest-path backend\Cargo.toml`
  - `cargo test --manifest-path backend\Cargo.toml api_router_builds_without_route_conflicts -- --nocapture`
  - `npm.cmd run build`

### 2026-04-23 Hills 本地播放器 PlaybackInfo 播放修复

- 对照本地播放器模板 `PlaybackSourceBuilder` 与 UHD/Emby-like adapter 后确认：播放器优先使用 `MediaSources[].DirectStreamUrl`，原生 Emby 响应使用 `/videos/{播放项}/original.{container}?MediaSourceId=...&PlaySessionId=...&api_key=...`，而不是项目此前返回的 `/emby/Videos/{版本项}/stream.{container}`。
- `PlaybackInfo` 的直连 URL 已改为 Emby 原生 `original` 形式，并固定使用本次请求的播放项 ID 作为 URL 路径；多版本媒体通过 `MediaSourceId` 指向实际版本，避免客户端选中版本后 URL 跳到另一个 ItemId 导致播放链路不一致。
- `AddApiKeyToDirectStreamUrl` 改回 `false`，直连 URL 仅携带 Emby 原生常见的 `DeviceId`、`MediaSourceId`、`PlaySessionId`、`api_key`，不再额外塞入 `X-Emby-Token` / `X-MediaBrowser-Token` 查询参数。
- 新增 `/Videos|videos|Video|video/{itemId}/original.{container}` 和带 `{mediaSourceId}` 的 `original.{container}` 路由，复用现有 `MediaSourceId -> 实际媒体版本` 解析与字节流响应，适配 Hills/EmbySDK 的原生播放入口。
- `PlaybackInfo.MediaStreams` 现在会过滤 ffprobe 识别出的内嵌 MJPEG 封面流，避免播放器把附件图当作可选视频轨；PGS 字幕 codec 在响应中归一为 Emby 常见的 `PGSSUB`。
- 当服务端关闭转码但设备 profile 触发转码条件时，`PlaybackInfo` 不再返回 `TranscodingDisabled` 错误响应，而是保留直连媒体源供客户端继续尝试播放，避免本地播放器拿到 200 但误判播放信息异常。
- 验证通过：
  - `cargo check --manifest-path backend\Cargo.toml`
  - `cargo test --manifest-path backend\Cargo.toml playback_info -- --nocapture`
  - `cargo test --manifest-path backend\Cargo.toml api_router_builds_without_route_conflicts -- --nocapture`
