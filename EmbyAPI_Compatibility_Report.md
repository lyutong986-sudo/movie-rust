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

本轮修复后已通过:

```text
cargo check --manifest-path backend/Cargo.toml
cargo test --manifest-path backend/Cargo.toml playback_info_accepts_emby_sdk_profile_object_arrays -- --nocapture
cargo test --manifest-path backend/Cargo.toml device_profile_conditions_evaluate_stream_properties -- --nocapture
cargo test --manifest-path backend/Cargo.toml transcoding_info_reports_real_reasons_and_sdk_fields -- --nocapture
```

当前仍存在一批既有 Rust warning，主要是未使用 import、未使用字段、未使用辅助函数和部分未来扩展模型；它们不阻塞构建，但建议后续在功能稳定后统一清理。
## 2026-04-22 前端适配补充

- 已开始按“前端适配当前 backend”路线推进，而不是继续假设后端完全等同官方 Jellyfin 服务发现行为。
- `frontend/packages/frontend/src/plugins/remote/auth.ts`
  已移除连接服务器时对 Jellyfin SDK discovery/version 结果的强依赖，改为优先探测当前后端真实可用的 `/System/Info/Public`。
- 登录前拉取服务器资料时，`Branding/Configuration` 和 `Users/Public` 现在按可选能力处理；即使 branding 未开放匿名访问，也不会阻塞添加服务器和进入登录页。
- 前端内部 `ServerInfo` 已改为显式字段模型，稳定保存 `Id`、`ServerName`、`Version`、`StartupWizardCompleted`、`PublicAddress` 等后续页面真正依赖的值。
- `frontend/packages/frontend/src/components/Wizard/WizardMetadata.vue`
  已修复 `PreferredMetadataLanguage` 与 `MetadataCountryCode` 前后映射写反的问题。
- 首次向导元数据页已增加后端兼容兜底：若当前后端尚未提供 `Localization/Countries`，前端会用已有配置值构造最小国家选项，避免流程中断。
- 本轮尚未在当前终端补跑前端类型检查；原因是当前环境里 `pnpm`/`npm`/`corepack` 不在 PATH，且可见 Node 入口不可直接执行。后续需要在具备前端包管理器的环境补跑：

```text
pnpm --filter @jellyfin-vue/frontend check:types
```

## 2026-04-22 前端适配补充（二）

- `frontend/packages/frontend/src/plugins/router/middlewares/validate.ts`
  已把路由 `itemId` 校验从“仅接受 32 位 MD5”改为同时接受 Emby/Jellyfin 常见 UUID 形式，避免前端自己拦截库页、详情页、剧集页跳转。
- `frontend/packages/frontend/src/pages/search.vue`
  已将人物搜索参数从 `searchTerm` 调整为当前后端 `Persons` 路由实际兼容的 `nameStartsWith`，修复搜索页人物结果为空的问题。
- 这一轮优先修复的是“前端自身校验/参数名导致的兼容问题”；后续仍需继续对照首页、库页、剧集页、播放页逐条核对 SDK 调用与后端真实响应。

## 2026-04-22 CI/CD 适配补充

- 已修复 `Dockerfile` 与当前前端 monorepo 结构不匹配的问题：
  前端构建阶段从旧的 `npm + frontend/src` 单项目结构切换为 `node:24 + corepack + pnpm workspace`。
- `Dockerfile` 现在会在 `frontend/` 目录执行 `pnpm install --frozen-lockfile`，并使用 `pnpm --filter @jellyfin-vue/frontend build` 构建真实前端入口。
- 运行时静态资源复制路径已更新为 `frontend/packages/frontend/dist`，与当前 Vite 包输出目录对齐。
- GitHub Actions 工作流 `.github/workflows/docker-image.yml` 已补充前置校验阶段：
  先跑后端 `cargo check`，再跑前端 `pnpm` 安装与构建，最后才执行 Docker 镜像构建/推送。
- 当前已在本地确认 `cargo check --manifest-path backend/Cargo.toml` 可通过；但由于本地终端缺少可直接使用的 Node/pnpm 运行环境，本轮无法在本机复现前端和 Docker 阶段，只能保证工作流与仓库结构已对齐。

## 2026-04-22 前端适配补充（三）

- `frontend/packages/frontend/src/pages/item/[itemId].vue`
  已修复错误的路由类型声明：从 `/genre/[itemId]` 改为 `/item/[itemId]`，避免详情页参数推断与真实页面路由不一致。
- `frontend/packages/frontend/src/pages/item/[itemId].vue`
  已避免在普通电影/视频详情页没有 `SeriesId` 时继续发起“当前剧集”请求，减少页面挂起风险。
- `frontend/packages/frontend/src/components/Buttons/FilterButton.vue`
  已让筛选弹层在首次进入页面时立即加载筛选项，不再依赖后续 prop 变化才触发。
- `frontend/packages/frontend/src/components/Buttons/FilterButton.vue`
  已针对媒体库 `CollectionType` 映射更合理的 `includeItemTypes`，避免把库根节点误当成 `CollectionFolder` 去请求筛选器，导致筛选结果偏空。

## 2026-04-22 前端适配补充（四）
- 本轮按“修 frontend 时结合 backend 和 EmbySDK，若冲突以 EmbySDK 为准”的规则，把人物搜索链路从“前端迁就当前后端”改回“项目按 EmbySDK 语义工作”。
- `模板项目/EmbySDK/SampleCode/RestApi/TypeScript/api.ts`
  已复核 EmbySDK 对 `/Persons` 的实际定义：明确支持 `SearchTerm`，`NameStartsWith` 只是并行存在的补充过滤条件。
- `backend/src/routes/persons.rs`
  已为 `/Persons` 增加 `SearchTerm/searchTerm` 查询参数解析，并继续兼容 `NameStartsWith/nameStartsWith`。
- `backend/src/repository.rs`
  已把人物查询调整为优先使用 EmbySDK 语义：传入 `SearchTerm` 时执行包含匹配；只有在未传入 `SearchTerm` 时才回退到 `NameStartsWith` 的前缀匹配；同时对空字符串做裁剪，避免无效查询污染结果。
- `frontend/packages/frontend/src/pages/search.vue`
  已把人物搜索参数从上一轮的临时兼容写法 `nameStartsWith` 切回 EmbySDK 标准 `searchTerm`，确保 frontend、backend 和本地播放器的调用语义一致。

## 2026-04-22 前端适配补充（五）
- 登录入口不再要求用户手动“添加服务器”。部署在 `https://test.emby.yun:4443` 这类单后端场景时，frontend 现在会默认把当前站点对应的后端作为服务器使用。
- `frontend/packages/frontend/src/utils/external-config.ts`
  已新增默认服务器解析逻辑：优先读取 `config.json` 的 `defaultServerURLs`；若为空，则自动回退到浏览器当前站点 `window.location.origin`。
- `frontend/packages/frontend/src/plugins/remote/index.ts`
  已把默认服务器初始化改为使用运行时解析后的默认服务器列表，而不是只依赖静态 `config.json`。
- `frontend/packages/frontend/src/plugins/router/middlewares/login.ts`
  已把登录守卫改为基于运行时默认服务器列表判断流程，避免在 `defaultServerURLs` 为空时把用户重定向到 `#/server/add`；并在禁用服务器选择时直接停留登录页，不再回落到“添加服务器”流程。
- `frontend/packages/frontend/public/config.json`
  已将 `allowServerSelection` 设为 `false`，关闭手动选择/添加服务器入口，使单后端部署默认走当前 backend。

## 2026-04-22 前端适配补充（六）
- 本轮继续按 EmbySDK 真实链路清首页和系列详情页，优先核对 `Views / Resume / Latest / NextUp` 以及详情页依赖的 `Seasons / Episodes` 调用方式。
- `模板项目/Emby模板/MediaBrowser.Api/UserLibrary/UserLibraryService.cs`
  已复核 Emby 模板里首页“最近加入”走的是 `Users/{UserId}/Items/Latest`，且支持 `ParentId` 与 `IncludeItemTypes`。
- `backend/src/routes/items.rs`
  已把 `Items/Latest` 的默认类型从固定 `Movie,Series` 调整为更接近 Emby 客户端首页期望的行为：
  根首页默认取 `Movie,Episode`；
  电视剧库默认取 `Episode`；
  电影库默认取 `Movie`；
  其余库按各自集合类型映射常见媒体类型。
- `frontend/packages/frontend/src/utils/items.ts`
  首页聚合请求现在会按库 `CollectionType` 主动传递更贴近 Emby 语义的 `includeItemTypes` 给 `getLatestMedia`，并显式按 `DateCreated Descending` 取数，减少前后端默认值分歧。
- `frontend/packages/frontend/src/pages/series/[itemId].vue`
  系列详情页已从泛用 `getItems(parentId=seasonId)` 切换为 EmbySDK 标准 `getEpisodes(seriesId, seasonId)`，并显式按 `IndexNumber Ascending` 获取每季剧集，后续补字段时可直接沿 `Shows/{seriesId}/Episodes` 标准链路继续适配。

## 2026-04-22 前端适配补充（七）
- 本轮继续按“若 frontend / backend / EmbySDK 冲突，以 EmbySDK 为准”的规则核对详情页字段，确认 `Taglines` 是 EmbySDK `BaseItemDto` 的标准字段，不应由前端回避。
- `模板项目/EmbySDK/SampleCode/RestApi/TypeScript/api.ts`
  已再次复核 `BaseItemDto` 与相关查询字段定义，`Taglines`、`People`、`GenreItems`、`MediaStreams`、`MediaSources` 都属于标准返回字段。
- `模板项目/EmbySDK/SampleCode/RestApi/Emby.ApiClient/Emby.ApiClient/Model/BaseItemDto.cs`
  已复核服务端 DTO 模型，确认 `Taglines` 是与 `People`、`GenreItems`、`MediaStreams` 同级的标准属性。
- `backend/src/scanner.rs`
  已为本地 NFO 扫描补充 `tagline` 读取，并在电影、剧集、季度、剧集导入时一起写入条目，避免本地元数据中的标语被丢弃。
- `backend/src/metadata/models.rs` 与 `backend/src/metadata/tmdb.rs`
  已把 TMDB 详情里的 `tagline` 接入外部元数据模型；后端刷新电影/剧集远程元数据时会把 TMDB `tagline` 规范化为 Emby 风格的 `Taglines: string[]`。
- `backend/src/models.rs`、`backend/src/repository.rs`、`backend/migrations/0013_emby_taglines.sql`
  已为 `media_items` 增加持久化 `taglines` 字段，并打通扫描入库、远程元数据刷新、`DbMediaItem -> BaseItemDto` 输出链路；详情页现在会返回真实 `Taglines`，而不是固定空数组。
- 本轮未额外修改 frontend 详情页调用代码：
  因为现有前端读取 `item.Taglines[0]` 的方式本身就符合 EmbySDK 语义，真正缺口在 backend 字段未落库。

## 2026-04-22 前端适配补充（八）
- 已对照 frontend 首启向导流程与 EmbySDK 启动阶段 API，确认新部署卡在 `/#/` 的核心原因不是首页数据，而是首启路由守卫与向导 API 的前端状态处理不一致。
- `frontend/packages/frontend/src/plugins/router/middlewares/login.ts`
  已修复单后端部署下的首启路由死锁：此前我们关闭了 `allowServerSelection`，但守卫把 `/wizard` 也一起拦掉，导致未完成首启时从根页尝试跳向导却被取消，页面就停在 `/#/`。现在仅继续拦截 `/server/add` 与 `/server/select`，保留 `/wizard` 与 `/server/login`。
- `frontend/packages/frontend/src/plugins/router/middlewares/login.ts`
  还为默认服务器等待补了超时兜底，避免当前端启动时默认服务器探测失败或过慢，守卫长期等待 `currentServer` 导致首屏挂起。
- `frontend/packages/frontend/src/components/Wizard/WizardAdminAccount.vue`
  已把管理员创建从错误的 `remote.sdk.api` 切回 EmbySDK 首启专用 `oneTimeSetup(...) + StartupApi.updateStartupUser(...)`。这一步在首启阶段本来就不应依赖已登录用户，否则新部署时会因为没有认证 API 实例而卡住。
- `frontend/packages/frontend/src/pages/wizard.vue`
  已在 `completeWizard()` 成功后同步更新当前服务器的 `StartupWizardCompleted` 本地状态，再跳转到登录页，避免前端因为缓存的旧状态把已完成向导的实例继续判定为“必须回向导”。
- `frontend/packages/frontend/src/components/Wizard/WizardAdminAccount.vue`
  已在成功创建首个管理员后同步刷新当前服务器的 `PublicUsers`，让后续登录页与首启后的服务器状态更一致。
- 本轮未新增 backend 路由：
  因为当前 backend 已具备 `/Startup/Configuration`、`/Startup/User`、`/Startup/RemoteAccess`、`/Startup/Complete` 这组首启接口；这次暴露出来的是 frontend 与 EmbySDK 首启调用方式不一致，而不是 backend 缺少同名能力。

## 2026-04-22 前端适配补充（九）
- 本轮继续往“首启后的登录页与首页首屏”收口，优先处理 `UserDto` 密码状态字段、`Items/Latest` 默认行为，以及首页聚合请求的失败兜底。
- `backend/src/repository.rs`
  已修复 `UserDto.HasPassword / HasConfiguredPassword` 之前一律返回 `true` 的问题。现在会根据当前用户密码是否等价于“空密码”真实推导，和 Emby 客户端的无密码公共用户登录语义更一致。
- `backend/src/routes/items.rs`
  已把 `Users/{UserId}/Items/Latest` 的默认 `IncludeItemTypes` 改成复用前面补好的 Emby 风格推断逻辑，不再退回旧的固定 `Movie,Series` 默认值。
- `frontend/packages/frontend/src/utils/items.ts`
  已给首页 `fetchIndexPage()` 增加空结果兜底：`Views / Resume / Latest / NextUp` 中任意一个请求失败时，不再让整页 Promise 直接失败，而是对该分区回退为空数组，避免登录后首屏看起来像“又卡死”。
- 本轮 backend 已重新执行：
  `cargo check --manifest-path backend/Cargo.toml`
  结果通过，仍只有既有 warning。

## 2026-04-22 前端适配补充（十）
- 本轮继续核对 `Users/{UserId}/Views` 返回的 `CollectionFolder/UserView` 字段，重点补齐 frontend 导航抽屉、首页库卡片和库页会直接用到的图片信息。
- `backend/src/repository.rs`
  已为 `library_to_item_dto()` 增加库级封面/背景推导：优先从媒体库根目录复用现有扫描逻辑查找 `folder.*` 与 `backdrop/fanart/background/landscape.*`，并把结果映射到 Emby 风格的 `PrimaryImageTag / ImageTags.Primary / BackdropImageTags`。
- 这次没有额外修改 frontend：
  因为前端图片选择逻辑本来就按 EmbySDK 的 `ImageTags / BackdropImageTags / PrimaryImageTag` 工作，之前显示不出来的根因是 backend 给库 DTO 返回了空图片字段。
- 本轮 backend 已重新执行：
  `cargo check --manifest-path backend/Cargo.toml`
  结果通过，仍只有既有 warning。

## 2026-04-22 前端适配补充（十一）
- 本轮开始核对管理页面链路，优先覆盖前端设置页实际调用的 `Auth/Keys`、`Devices`、`System/Configuration`、`System/Configuration/{Key}` 与日志下载路径；冲突时按本地 `模板项目/EmbySDK/SampleCode/RestApi/TypeScript/api.ts` 的路径语义处理。
- `backend/src/routes/devices.rs` 新增 EmbySDK 兼容设备管理路由：`GET /Devices` 返回 `QueryResult<DeviceInfo>`，字段包含 `Id`、`Name`、`AppName`、`AppVersion`、`LastUserId`、`LastUserName`、`DateLastActivity`；`DELETE /Devices?Id=...` 与 `DELETE /Devices/{id}` 会按 `device_id` 删除该设备关联 sessions。
- `backend/src/routes/sessions.rs` 与 `backend/src/repository.rs` 已给 Auth Keys 管理页补齐 `DateCreated`，来源为 `sessions.created_at`，避免 API Key 列表日期列为空；同时新增按设备删除 sessions 的 repository 能力。
- `backend/src/routes/system.rs` 与 `backend/src/repository.rs` 新增 `GET/POST /System/Configuration`，返回并持久化管理页实际依赖的 `ServerName`、`UICulture`、`QuickConnectAvailable`、`CachePath`、`MetadataPath`、`LibraryScanFanoutConcurrency`、`ParallelImageEncodingLimit` 等字段；保存时会同步更新 startup 配置与 remote access 配置。
- `backend/src/routes/system.rs` 新增 `GET/POST /System/Configuration/{Key}`，其中 `branding` 命名配置会读写 `BrandingConfiguration`，兼容前端 `updateNamedConfiguration({ key: 'branding' })`；同时新增 `/System/Logs/Log?name=...` 日志下载别名，适配管理页直接打开日志链接的行为。
- `frontend/packages/frontend/src/pages/settings/devices.vue` 修复“删除全部设备”条件：现在只删除存在 `Id` 且不是当前前端 `deviceInfo.id` 的设备，避免批量清理时把当前正在使用的设备会话一起删掉。
- `backend/src/models.rs` 已给 `UserPolicyDto` 补齐 EmbySDK/Jellyfin 管理页父母控制实际使用的 `BlockUnratedItems` 字段，避免保存未分级内容拦截项时被后端模型丢弃。
- `frontend/packages/frontend/src/pages/settings/users/new.vue` 现在会在创建用户后把新建页填写的密码通过 `updateUserPassword(NewPw)` 提交给后端，不再出现“输入了密码但新用户仍为空密码”的管理页行为。
- `frontend/packages/frontend/src/pages/settings/users/[id].vue` 修复用户详情页密码修改参数：`CurrentPw` 现在使用当前密码输入框，而不是误用确认密码；父母控制保存时也会同时提交 `BlockedTags`。
- 本轮 backend 已重新执行：`cargo check --manifest-path backend/Cargo.toml`，结果通过；仍只有既有 warning。前端类型检查本轮未执行，原因仍是当前本地环境未提供可直接运行的 `pnpm` 前端工具链。

## 2026-04-22 前端适配补充（十二）
- 根据部署日志复核登录前链路，确认 `GET /Branding/Configuration` 和 `GET /Localization/Options` 会在无 token 的启动/登录前阶段被 frontend 调用；这类端点如果由 `AuthSession` 提取器保护，会直接 401 并导致语言下拉为空、向导按钮长时间 loading。
- `backend/src/routes/system.rs` 已将 `Branding/Configuration` 与 `Branding/Css` 调整为匿名可读；写入品牌配置仍通过管理页使用 `System/Configuration/{Key}`，继续要求管理员认证。
- `backend/src/routes/compat.rs` 已将 `Localization/Options` 与 `Localization/Cultures` 调整为匿名可读，保证首次启动向导语言页、元数据语言页在未登录状态下也能加载选项。
- `backend/src/routes/compat.rs` 新增 EmbySDK 路径 `GET /Localization/Countries` 与 `GET /Localization/ParentalRatings`，覆盖启动向导国家下拉和用户管理父母控制页实际依赖的 localization 端点。
- 本轮已重新执行：`cargo check --manifest-path backend/Cargo.toml` 与 `cargo test --manifest-path backend/Cargo.toml api_router_builds_without_route_conflicts -- --nocapture`，结果均通过；仍只有既有 warning。前端类型检查仍受限于本地缺少可直接运行的 `pnpm`。
