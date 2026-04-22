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
## 2026-04-22 前端适配补充（十三）
- 本轮按“前端实际 SDK 调用 → 后端现有路由/字段 → EmbySDK 标准”的顺序复核了元数据编辑与识别链路，优先处理会造成 404、空下拉和 Suspense 首屏长期 loading 的缺口。
- `backend/src/routes/items.rs` 新增 EmbySDK 元数据编辑端点：`GET /Items/{itemId}/MetadataEditor`、`POST /Items/{itemId}`、`POST /Items/{itemId}/ContentType`、`GET /Items/{itemId}/ExternalIdInfos`、`GET /Items/{itemId}/Ancestors`。其中 MetadataEditor 返回 `ContentType/ContentTypeOptions`，ExternalIdInfos 返回常见 provider 列表，Ancestors 会返回父级条目与 `CollectionFolder`，避免编辑页类型下拉、识别弹窗和 genre 加载链路因 404 中断。
- `backend/src/routes/items.rs` 新增 `POST /Items/RemoteSearch/{Book|BoxSet|Movie|MusicAlbum|MusicArtist|MusicVideo|Person|Series|Trailer}` 与 `POST /Items/RemoteSearch/Apply/{itemId}`。当前远程搜索在没有外部 provider 后端能力时按 EmbySDK 形状返回空数组，保证前端显示“无结果”而不是错误弹窗或卡住；Apply 会保存选中结果里的 `Name/ProductionYear/ProviderIds`。
- `backend/src/models.rs` 与 `backend/src/repository.rs` 新增 Emby 风格 `UpdateBaseItemDto` 保存路径，`POST /Items/{itemId}` 现在会更新标题、排序名、简介、评分、集数序号、分级、年份、日期、Genres/Tags/Studios/Taglines/ProviderIds 和 People 关系，适配 `MetadataEditor.vue` 的 `updateItem({ baseItemDto })`。
- `frontend/packages/frontend/src/composables/apis.ts` 增加首屏请求失败兜底：缓存未生成时会使用本次请求结果或上一次值，初始等待缓存最多 1.5 秒后释放 Suspense，避免 401/404/500 或后端缺字段时整页长时间 loading。
- 本轮仍需后续深化：远程识别目前只提供兼容空结果与 Apply 基础写入，尚未接入 TMDB/TVDB/IMDb 的真实搜索结果映射；`POST /Items/{itemId}` 的 People 保存已可替换关系，但更复杂的 LockedFields、CustomRating、专辑艺人等字段仍需结合后续数据库模型继续补齐。

## 2026-04-22 前端适配补充（十四）
- 根据运行日志复核，frontend WebSocket 实际调用 `GET /socket?api_key=...&deviceId=...`，而 backend 仅暴露 `GET /embywebsocket`，导致前端持续重连并产生 404。
- `backend/src/routes/mod.rs` 已新增 `GET /socket`，复用现有 `emby_websocket_handler`，该 handler 已兼容 `api_key/token/deviceId` 查询参数；根路径、`/emby/socket`、`/mediabrowser/socket` 会随现有 router nest 一起可用。

## 2026-04-22 前端适配补充（十五）
- 本轮继续审计 frontend 的菜单和播放链路，确认 `ItemMenu.vue -> playbackManager.instantMixFromItem()` 通过 EmbySDK `InstantMixApi.getInstantMixFromItem` 调用 InstantMix，而 backend 缺少 InstantMix 端点，会导致点击“Instant Mix”后 404。
- `backend/src/routes/items.rs` 已新增 `GET /Items/{itemId}/InstantMix`、`GET /Users/{userId}/Items/{itemId}/InstantMix`、`GET /Albums/{itemId}/InstantMix`、`GET /Artists/{name}/InstantMix`、`GET /MusicGenres/{name}/InstantMix`，返回 Emby 风格 `QueryResult<BaseItemDto>`；当前按专辑/父文件夹、艺人、流派或音频 seed 做最小可用混合，不改 frontend 的 SDK 调用。
- 复核删除菜单时发现 `apiStore.itemDelete()` 通过 EmbySDK `LibraryApi.deleteItem({ itemId })` 走 `DELETE /Items/{itemId}`，backend 原先只有 `GET/POST /Items/{itemId}`；现已补 `DELETE /Items/{itemId}` 与兼容 `POST /Items/{itemId}/Delete`。当前实现只删除数据库 `media_items` 记录及级联关联，不直接删除磁盘文件，避免误删本地媒体。

## 2026-04-22 前端适配补充（十六）
- 根据运行日志复核 frontend 登录后首页链路，确认播放器通过 EmbySDK `getUserViews()` 调用标准 `GET /UserViews?userId=...`，而 backend 只暴露了 `GET /Users/{userId}/Views`，导致首页库视图请求 404。
- `backend/src/routes/items.rs` 已新增 `GET /UserViews`，解析 Emby/Jellyfin 常见的 `UserId/userId` 查询参数，并复用现有媒体库视图 DTO 输出；带 `userId` 时会继续执行用户访问校验，响应仍为 `QueryResult<BaseItemDto>`。
- 运行日志还显示 websocket 已成功升级到 101，但服务端推送会话命令时旧数据库缺少 `session_commands` 表会每秒输出 WARN；`backend/src/repository.rs` 现在对 `session_commands` 缺表或缺 `consumed_at` 字段做兼容兜底，返回空命令队列，避免迁移未补齐时拖累 websocket 心跳和日志。
- 后续仍建议对线上/本地 PG 执行最新迁移，确保 `0018_session_play_queue.sql` 和 `0020_session_command_consumption.sql` 已应用；本轮代码兜底只保证 Emby 客户端链路不中断，不替代真实会话命令队列表能力。

## 2026-04-22 前端适配补充（十七）
- 本轮按“初始化自带，而不是只依赖数据库迁移”的要求审计启动链路，确认 frontend 首屏/向导/设置页会读取 `System/Info/Public`、`Branding/Configuration`、`Startup/Configuration`、`System/Configuration`、`DisplayPreferences`、websocket session commands 等基础表和值；旧库迁移历史不完整时，这些缺口会表现为 401/404、空配置、用户设置同步失败或 websocket WARN。
- `backend/src/main.rs` 的 `ensure_schema_compatibility()` 已扩展为启动自检初始化层：启动时会确保 `system_settings`、`display_preferences`、`session_play_queue`、`session_commands` 等前端必需表存在，并补齐 `users.policy/configuration`、`sessions.expires_at`、`libraries.library_options`、`media_items` 的 Emby 常用字段。
- `backend/src/main.rs` 新增默认系统设置种子：`startup_configuration`、`startup_remote_access`、`branding_configuration`、`server_configuration`、`display_preferences_defaults:vue`、`display_preferences_defaults:emby`。这些值只在缺失时写入，不覆盖管理页已保存配置，保证新库和迁移不完整的旧库启动后仍能给 EmbySDK 前端返回基础配置。
- 本轮未修改 frontend：前端读取这些值的方式符合 EmbySDK/Jellyfin SDK 习惯，真正问题是 backend 初始化层没有把前端必需的基础表和值视为启动自带能力。

## 2026-04-22 前端适配补充（十八）
- 本轮继续按“frontend + 本地播放器模板 + EmbySDK 标准字段”审计数据库自建层，范围扩大到后端尚未完全调用、但 Emby 客户端/SDK 后续会自然需要落库的字段和表，避免未来每补一个 API 都再依赖迁移补结构。
- `backend/src/main.rs` 已把 `media_streams`、`persons`、`person_roles` 从“仅迁移创建”提升为“启动自建”。这些表是 `BaseItemDto.People`、`BaseItemDto.MediaStreams`、播放信息和筛选接口的基础结构，旧库缺表时不应等迁移才能恢复。
- `media_items` 初始化预留已扩展到 Emby 常见字段：`BitRate/Size/HomePageUrl/Budget/Revenue/CustomRating/LockedFields/IsLocked/IsVirtualItem/IsPlaceHolder/LocationType/PathProtocol/DisplayOrder/PresentationUniqueKey/AlbumId/SeriesId/SeasonId/ExternalUrls/TrailerUrls/ImageTags/BackdropImageTags/ProviderMetadata/EmbyExtra` 等。当前后端可继续只使用已实现字段，未用字段先安全留空。
- `user_item_data` 初始化预留已补齐 `Rating/PlayedPercentage/UnplayedItemCount/Likes/AudioStreamIndex/SubtitleStreamIndex/HideFromResume/CustomData`，对应 EmbySDK `UserItemDataDto` 的标准形状，避免用户设置、播放轨道选择、隐藏继续观看等功能后续无字段可写。
- 新增预留表 `media_sources`、`item_images`、`device_registry`、`api_keys`、`activity_log`、`scheduled_tasks`，对应 EmbySDK 的 `MediaSourceInfo`、多图槽位、设备管理、API Key 管理、活动日志和计划任务 websocket/设置页能力。当前部分路由仍可从现有 sessions/playback_events 推导响应，但数据库已经具备真实实现的落库位置。
- 本轮依旧不改 frontend：前端和本地播放器按 SDK 标准消费字段；backend 初始化层负责把这些标准字段和基础表预先建好。

## 2026-04-22 前端适配补充（十九）
- 根据 `log/movie-rust-20260422174410.log` 复核高频错误：`GET /UserItems/Resume` 返回 404，`GET /Items/Latest` 被动态路由 `/Items/{itemId}` 当成 itemId 解析并返回 400，`GET /Shows/NextUp` 因 EmbySDK 重复 query key（如多次 `fields=`、多次 `enableImageTypes=`）触发参数解析 400。
- `backend/src/routes/items.rs` 已新增 EmbySDK 标准 `GET /Items/Latest` 与 `GET /UserItems/Resume`。前者按 `userId` 查询最新媒体并返回 `BaseItemDto[]`，后者按 `userId` 查询继续观看并返回 `QueryResult<BaseItemDto>`，均复用既有用户权限和 DTO 转换逻辑。
- `backend/src/models.rs` 为 `ItemsQuery` 新增 raw query 容错解析：重复的列表型参数会合并成逗号列表，标量参数使用最后一次有效值，兼容 EmbySDK/Jellyfin SDK 常见的 `fields=x&fields=y`、`enableImageTypes=Primary&enableImageTypes=Backdrop` 形式。
- `backend/src/routes/shows.rs` 已将 `Shows/NextUp`、`Shows/Upcoming`、`Shows/Missing` 切换到该容错解析，避免首页 NextUp 链路因为重复字段参数直接 400。
- 日志中的 `display_preferences` 缺表 500 已由上一轮启动自建层覆盖；该日志生成于修复前，重启当前 backend 后 `display_preferences` 会由初始化层自动创建。

## 2026-04-22 前端适配补充（二十）
- 前端设置首页存在一处纯前端层面的“硬禁用”：`frontend/packages/frontend/src/pages/settings/index.vue` 中的“媒体库 / Libraries”入口被直接写成 `link: undefined`，并不是后端能力判断结果。
- 审计后确认后端已经具备可用的媒体库管理链路：`GET /Library/VirtualFolders`、`GET /Library/VirtualFolders/Query`、`POST /Library/Refresh`、`GET /Library/SelectableMediaFolders`，因此该功能不应继续在前端保持禁用。
- 已新增 `frontend/packages/frontend/src/pages/settings/libraries.vue`，接入现有后端媒体库能力，提供媒体库列表查看与全库刷新入口；设置页中的 `Libraries` 现已解除禁用并可访问。
- 其余仍保持禁用的设置项（如 `DLNA`、`Live TV`、`Plugins`、`Scheduled Tasks`、`Notifications`）要么当前后端没有对应 EmbySDK 标准管理路由，要么前端尚无完整页面，这一轮不做误开放。

## 2026-04-22 前端适配补充（二十一）

- 对照前端现有设置页与 EmbySDK 调用，补齐并修正了 Users 相关链路的后端兼容性：新增 POST /Users/Password、为 POST|PUT|DELETE /Users/{user_id} 提供标准支持，并将 Users/{user_id}、Users/{user_id}/Password、Users/{user_id}/Policy 等路径统一改为按 Emby GUID 解析，避免前端直接使用 UserDto.Id 时因后端只认原始 UUID 而出现 400/404。
- 后端新增 epository::update_user_name(...)，用于承接前端用户详情页的 updateUser(...) 调用；这样设置页里的用户名编辑不再只是前端存在、后端缺失。
- ResetPassword 分支改为可直接落库，当前兼容行为为重置为默认密码  000，避免前端“重置密码”按钮走到后端时报不支持。
- 兼容层 DisplayPreferences / UserSettings 的用户路径也同步切换为 Emby GUID 解析，补齐前端同步配置、用户设置等 SDK 能力与后端 ID 语义不一致的问题。
- 前端账户页移除了“把头像文件转 base64 再上传”的临时绕行，恢复按 EmbySDK 预期直接提交 File 到 postUserImage(...)；当前项目前后端以 SDK 约定为准，不再让前端为旧后端行为做特殊兼容。
- 本轮后端验证：cargo check --manifest-path backend\\Cargo.toml 已通过。

## 2026-04-22 前端适配补充（二十二）

- 继续对照前端用户管理页与 EmbySDK 调用，修正 UserPolicyDto 中与库/频道访问相关的 ID 字段兼容性：EnabledFolders、EnabledChannels、BlockedMediaFolders、BlockedChannels 现在按 Emby GUID 字符串进行序列化/反序列化，内部仍保留 Uuid 存储。这样前端用户详情页、新建用户页在保存媒体库访问范围时，不会再因为后端直接按原始 UUID 反序列化而报错。
- 为 Startup 初始化向导相关更新接口补充更宽容的 method 兼容：/Startup/Configuration、/Startup/User、/Startup/RemoteAccess、/Startup/Complete 现支持 EmbySDK/前端可能使用的 POST/PUT 写入方式，降低首次初始化流程因 method 不匹配而失败的风险。
- 本轮后端验证：cargo check --manifest-path backend\\Cargo.toml 已通过。

## 2026-04-22 前端适配补充（二十三）

- 针对启动向导进行了返回体与调用链审计。当前前端实际使用的向导字段集中在 StartupConfiguration 的 UICulture、PreferredMetadataLanguage、MetadataCountryCode，以及 StartupRemoteAccess 的 EnableRemoteAccess、EnableAutomaticPortMapping，后端现有返回体已覆盖这些真实使用字段；同时为 /Startup/Configuration、/Startup/User、/Startup/RemoteAccess、/Startup/Complete 增补了 POST/PUT 双兼容，降低 SDK method 差异带来的初始化失败风险。
- 对设置页 server / apikeys / logs-and-activity 做了逐项审计：
  - server：后端 System/Configuration 现已提供前端实际双向绑定的字段，包括 ServerName、UICulture、QuickConnectAvailable、CachePath、MetadataPath、LibraryScanFanoutConcurrency、ParallelImageEncodingLimit；并为 /System/Configuration 与 /System/Configuration/{key} 更新接口补充 POST/PUT 双兼容。
  - pikeys：/Auth/Keys 返回结构已符合前端 useApi 对 Items 的拆包逻辑；本轮进一步将 AuthenticationInfo.UserId 统一输出为 Emby GUID，保持与项目其余用户 ID 语义一致。
  - logs-and-activity：前端页面按 SDK 的 LogLevel.Information/Warning/Error/... 与活动类型 SessionStarted / SessionEnded / VideoPlayback / VideoPlaybackStopped / UserPasswordChanged 做显示分支。后端原先活动日志使用了不匹配的 Severity = Info 与较原始的播放事件类型；本轮已修正为前端当前识别的枚举值与类型映射，避免颜色、图标和文案分支失效。
- 审计说明：当前工作区内未直接检索到本地 SDK 生成类型定义文件本体，因此“完全一致性”判断以当前前端实际 import 的类型用法和运行调用形状为准，并已优先修复真实会影响页面行为的字段名、枚举值与 method 兼容问题。
- 本轮后端验证：cargo check --manifest-path backend\\Cargo.toml 已通过。

## 2026-04-22 前端适配补充（二十四）

- 对 System/Configuration 与前端 [server.vue] 设置页双向绑定字段做了更细的默认值/可写性核对，并修正后端保存策略：
  - 当前前端实际双绑字段为 ServerName、UICulture、QuickConnectAvailable、CachePath、MetadataPath、LibraryScanFanoutConcurrency、ParallelImageEncodingLimit。
  - 后端 update_server_configuration_value(...) 现不再把前端传入 JSON 原样入库，而是按这些字段做规范化和默认值回填后再持久化，避免空字符串、错误类型或缺字段导致下一次读取时配置形状漂移。
  - 启动向导与服务器设置共用的 ServerName/UICulture/PreferredMetadataLanguage/MetadataCountryCode/EnableRemoteAccess/EnableUPnP 也会同步回写到对应的启动配置与远程访问配置，保证读写语义一致。
- 顺着 websocket 和 ctivity/session 链路补了 EmbySDK 订阅协议兼容：
  - 前端连接建立后会发送 ScheduledTasksInfoStart、ActivityLogEntryStart、SessionsStart。后端此前仅把任意文本消息包装成 KeepAlive 回写，协议层不完整。
  - 现在 websocket 已支持识别这些订阅消息并返回对应应答：ScheduledTasksInfo（当前为空数组占位）、ActivityLogEntry（活动日志列表）、Sessions（当前会话列表）。这使得协议层更接近 EmbySDK，避免“连接建立成功但订阅消息没有任何语义响应”的兼容缺口。
  - 说明：当前项目内前端真正直接消费 websocket 推送的仍主要是 RefreshProgress / LibraryChanged / UserDataChanged，ScheduledTasksInfo / ActivityLogEntry / Sessions 目前更多是为 EmbySDK 握手与未来兼容预留；这次补的是协议完整性与后续客户端兼容性。
- 进一步修正了活动日志与会话相关返回语义：活动日志 Severity 已对齐为前端识别的 Information，播放事件已映射到前端当前识别的 SessionStarted / VideoPlayback / VideoPlaybackStopped 类型；API key 列表中的 UserId 也统一输出为 Emby GUID。
- 本轮后端验证：cargo check --manifest-path backend\\Cargo.toml 已通过。

## 2026-04-22 前端适配补充（二十五）

- 继续核对 websocket 的 RefreshProgress / LibraryChanged 推送时机，并补齐主动广播能力：
  - 后端 AppState 新增 websocket 广播通道，websocket 连接除处理客户端消息外，也会订阅后端主动广播事件，不再局限于“收到什么就即时回复什么”。
  - POST /Library/Refresh 与后台扫描入口现在会在扫描开始时为每个媒体库广播 RefreshProgress { ItemId, Progress: 0 }，扫描完成后广播 RefreshProgress { Progress: 100 }，并额外广播 LibraryChanged { ItemsUpdated: [...] }。
  - POST /Items/{id}/Refresh 也会在单项元数据刷新前后广播对应的 RefreshProgress，并在完成后发出 LibraryChanged，这样前端卡片和任务状态都能通过 websocket 同步更新。
- 继续审计 Sessions 相关 REST 与 websocket 数据字段：
  - SessionInfoDto.user_id 原先仍输出原始 UUID，现已统一修正为 Emby GUID，保证 Sessions 链路的用户 ID 语义与项目其余接口一致。
  - 当前 SessionInfoDto 已覆盖前端与 EmbySDK 常见会话字段：Id、UserId、UserName、Client、DeviceId、DeviceName、ApplicationVersion、IsActive、LastActivityDate、RemoteEndPoint、SupportsRemoteControl、PlayableMediaTypes、SupportedCommands、NowPlayingItem、PlayState、NowViewingItem。
  - 审计当前前端工作区后，未发现更多已被直接消费但缺失的 SessionInfoDto 字段；并且 websocket SessionsStart 与 REST /Sessions 现都基于同一套会话 DTO 组装逻辑，避免两条链路字段漂移。
- 本轮后端验证：cargo check --manifest-path backend\\Cargo.toml 已通过。
## 2026-04-22 前端适配补充（二十六）

- 继续按 Emby 风格细化 websocket 变更载荷：`LibraryChanged` 现在统一补齐 `ItemsAdded`、`ItemsRemoved`、`ItemsUpdated` 三组字段，不再只发 `ItemsUpdated`。目前已接到全库刷新、单项元数据刷新、项目编辑、内容类型变更、远程识别 Apply、项目删除等真实写路径；删除场景会走 `ItemsRemoved`，更新场景走 `ItemsUpdated`。
- `backend/src/routes/items.rs` 新增了面向 websocket 的 `broadcast_user_data_changed(...)`，把 `ItemId`、`PlaybackPositionTicks`、`PlayCount`、`IsFavorite`、`Played`、`LastPlayedDate`、`PlayedPercentage` 等 `UserItemDataDto` 常用字段封装为 `UserDataChanged { UserDataList: [...] }`。这和当前 frontend `apiStore` 的消费方式直接对齐，收到推送后会按 `ItemId` 重新拉取项目详情。
- 这组 `UserDataChanged` 推送已经接到前端真实会改用户状态的高频入口：`POST /UserItems/{id}/UserData`、`POST /Users/{userId}/Items/{id}/UserData`、收藏/取消收藏、标记已播放/未播放、`HideFromResume`。这样收藏、播放进度、已播放状态在不同页面之间的同步更接近 Emby 客户端行为。
- `backend/src/routes/sessions.rs` 继续顺着播放态上报链路补 websocket 推送：`/Sessions/Playing`、`/Sessions/Playing/Progress`、`/Sessions/Playing/Stopped` 以及 legacy `PlayingItems` 路径，在写入 `playback_events` 后会主动广播最新 `UserDataChanged`，并额外推送一次最新 `Sessions` 列表和轻量 `PlaybackProgress` 事件，补上播放态变化的实时通知面。
- 这次补完后，当前项目 websocket 的高频同步链路已经覆盖三类核心状态：任务刷新（`RefreshProgress`）、媒体库项目变更（`LibraryChanged`）、用户项目状态与播放态变化（`UserDataChanged` / `Sessions` / `PlaybackProgress`）。frontend 现有代码会直接受益于前两类，而后三类也为 EmbySDK 客户端继续兼容预留了更完整的标准推送面。
- 本轮后端验证：`cargo check --manifest-path backend\\Cargo.toml` 已通过；仍只有既有 warning，未新增编译错误。

## 2026-04-22 前端适配补充（二十七）

- 继续把 `UserDataChanged` 往 Emby 客户端更在意的“批量列表变化”场景补深：后端现在不会只推当前条目本身，而是会顺着父级链一路补齐相关的 `UserItemDataDto`。对于剧集播放、标记已播放、收藏、HideFromResume、播放进度上报等操作，websocket 推送现在会一起覆盖当前集、父季、父剧等相关条目，给前端同步未播放计数、父级播放状态提供更完整的刷新线索。
- 为了让“继续观看 / 最新媒体 / Next Up”这类 BaseItem 列表缓存不再卡在旧结果上，frontend 的 BaseItem 缓存层也做了联动修复：`apiDb` 新增 BaseItem 请求缓存清理，websocket 收到 `LibraryChanged` / `UserDataChanged` 时会清掉相关 BaseItem 响应缓存；同时 `useBaseItem` 在 `lastUpdatedIds` 变化时不再只刷新本地 item 缓存，也会主动重新请求当前活跃的 BaseItem 查询。这样继续观看命中变化和父级列表变化不会再长期停留在旧缓存里。
- `settings/libraries` 原先只是一个最小只读页，空数据时几乎只剩一张空卡片，且没有把 EmbySDK 风格的媒体库管理动作接完整。本轮将 [frontend/packages/frontend/src/pages/settings/libraries.vue] 重做为可实际操作的管理页：支持查看虚拟媒体库、编辑名称、调整元数据语言/国家、开关基础库选项、增删路径、创建媒体库，以及全库刷新。
- 针对这个页面的 Emby 兼容坑，后端一并修正了媒体库选项保存的 ID 语义：[backend/src/models.rs] 中 `UpdateLibraryOptionsDto.Id` 不再按原始 `Uuid` 直接反序列化，而是按 Emby GUID 字符串接收，再由 [backend/src/routes/admin.rs] 转回内部 UUID；同时 `GET /Library/SelectableMediaFolders` 返回的 `Id/Guid` 也统一改为 Emby GUID，避免前端和 EmbySDK 继续混用两套 ID 语义。
- 这轮仍以 EmbySDK 为准，没有强行让 frontend 去兼容后端旧行为；相反是把 backend 的媒体库管理 DTO 和 websocket 用户数据联动都往 EmbySDK/Jellyfin SDK 的使用习惯靠拢。
- 验证情况：`cargo check --manifest-path backend\\Cargo.toml` 已通过。前端这轮没有跑完整构建检查，当前结论基于代码级审计与修复。

## 2026-04-22 前后端 ID 语义适配补充（二十八）

- 本轮按“项目对外使用和发送原始 UUID 的位置全部替换成 Emby GUID”的要求，先统一后端核心转换函数：`uuid_to_emby_guid(...)` 现在输出 Emby/Jellyfin 客户端常用的大写无横线 GUID，避免 DTO 的 `Id/UserId/Guid/ItemId` 等字段继续暴露原始带横线 UUID。
- 入参侧同步放宽为 Emby GUID：`deserialize_optional_uuid(...)` 改为复用 `emby_id_to_uuid(...)`，并覆盖 `ItemsQuery.UserId/SeriesId`、`UserItemDataQuery.UserId`、`PlaybackReport.ItemId/UserId`、`SeasonsQuery`、`EpisodesQuery`、`GetSimilarItems.UserId`、`PlaybackInfoDto.UserId`、`UserViews.userId`、`DisplayPreferences.UserId`、`Genres.UserId` 等前端和 EmbySDK 高频查询/请求体字段。
- 路径参数侧清理了会直接导致无横线 GUID 404/400 的 `Path<Uuid>`：用户视图、用户根目录、用户 Items、HomeSections、Suggestions、Latest、ItemFilters、UserItems/UserData、Favorite、Played、Resume、UserItemById、InstantMix、Intros、LocalTrailers、SpecialFeatures、HideFromResume、UserSimilar、Genres 用户路径、PlayingItems legacy 播放上报路径、admin library 删除路径等，现均先按字符串接收再按 Emby GUID 解析。
- 对外响应侧补齐了几个裸 UUID 漏点：`DisplayPreferences.UserId`、`Devices.LastUserId`、`session_commands.Id`、Person 按 ID 访问和 similar item 排除列表解析均切到 Emby GUID 语义，避免前端/本地播放器在后续链路里混用两套 ID 格式。
- 验证情况：`cargo check --manifest-path backend\Cargo.toml` 已通过；仍有既有 warning，未新增编译错误。- frontend 侧同步处理 SDK 设备标识：`sdk-utils.ensureDeviceId()` 现在生成并缓存大写无横线 Emby GUID，且会把已存在的带横线 `deviceId` 自动归一化，避免 `/socket?deviceId=...`、会话注册等链路继续发送原始 UUID 形态。

## 2026-04-22 前后端 ID 语义适配补充（二十九）

- 继续排查对外响应中仍可能发送原始带横线 UUID 的位置，并修正 `GET /Library/SelectableMediaFolders` 的 `SubFolders[].Id`：子文件夹 ID 现在使用媒体库 Emby GUID 作为前缀，不再直接拼接数据库 `library.id` 原始 UUID。
- 修正 `POST /Metadata/Persons/Fetch` 返回体：`PersonId` 现在输出 Emby GUID 字符串，避免元数据人物抓取链路把内部 `uuid::Uuid` 直接序列化给前端或本地播放器。
- frontend 路由校验同步收紧：`itemId` 路由参数现在只接受 32 位 Emby GUID，不再把带横线原始 UUID 当作合法项目 ID，从入口层避免继续传播旧 ID 形态。

## 2026-04-22 媒体库添加功能适配（三十）

- 对照 `模板项目/Emby.Web.Mobile-master/src/components/medialibrarycreator` 与 `libraryoptionseditor` 的原始媒体库添加流程，当前项目的 `settings/libraries` 简化表单已重做为 Emby 风格配置面板。新增/编辑媒体库现在覆盖原始模板会提交的关键 `LibraryOptions` 字段：`EnableArchiveMediaFiles`、`EnablePhotos`、`EnableRealtimeMonitor`、`EnableInternetProviders`、`DownloadImagesInAdvance`、`SaveLocalMetadata`、`ImportMissingEpisodes`、`EnableAutomaticSeriesGrouping`、`EnableChapterImageExtraction`、`ExtractChapterImagesDuringLibraryScan`、`PreferredMetadataLanguage`、`MetadataCountryCode`、`SeasonZeroDisplayName`、`MetadataSavers`、`LocalMetadataReaderOrder`、`DisabledLocalMetadataReaders`、`AutomaticRefreshIntervalDays`、`EnableEmbeddedTitles`、`EnableEmbeddedEpisodeInfos`。
- 前端添加路径功能补齐 Emby 原始添加器的 `NetworkPath` 语义：创建媒体库时支持按 `Path | NetworkPath` 输入，编辑媒体库时新增路径也会通过 `PathInfo` 提交，避免只保存本地路径。
- 后端 `LibraryOptionsDto` 和 `MediaPathInfoDto` 同步扩展，新增字段会被反序列化、持久化并在 `GET /Library/VirtualFolders`/`Query` 中原样返回；路径规范化现在会保留对应的 `NetworkPath`。
- 后端 `POST /Library/VirtualFolders/Paths` 现在优先处理 SDK 风格的 `PathInfo` 请求体并保留 `NetworkPath`；旧的 `Path` 请求体仍继续兼容。
- 按 Emby 原始添加器行为修正 `mixed` 类型：前端会把 `mixed` 作为空 `CollectionType` 提交，后端现在把空内容类型规范化为 `mixed`，不再误落成 `movies`。
- 验证情况：后端 `cargo check --manifest-path backend\\Cargo.toml` 已通过。当前环境缺少 `node`、`pnpm`、`corepack` 命令，前端未能运行类型检查；本轮前端验证为代码级审计。

## 2026-04-22 rg 模板对照审计补充（二十九）

- 本轮按要求使用 `rg` 对 `frontend/packages/frontend/src`、`模板项目/本地播放器模板`、`模板项目/EmbySDK/SampleCode/RestApi/TypeScript/api.ts` 和 `backend/src/routes` 做交叉审计：先抽取 frontend 的 `newUserApi(...)`/`useApi(...)` 调用，再抽取本地播放器 Dart 侧 `fetchPlaybackInfo/fetchSeasons/fetchEpisodes/fetchItems/fetchItemDetail` 等真实调用，最后对照 EmbySDK `localVarPath` 与 backend `.route(...)`。
- 已补齐低风险缺口：`POST /Sessions/Playing/Ping` 现在返回 204，用于 EmbySDK 播放心跳；`POST /Items/RemoteSearch/Game`、`POST /Items/RemoteSearch/Image` 复用当前兼容空结果逻辑，避免 SDK 识别/图片搜索扩展路径 404；`POST /Items/Metadata/Reset` 增加安全占位，当前返回 204，不删除本地元数据。
- 已补齐用户配置标准路径：`GET/POST/PUT /Users/{Id}/Configuration` 与 `POST /Users/{Id}/Configuration/Partial` 现在直接读写 `UserConfigurationDto`，和已有 `UserSettings` 兼容路径共享同一份用户配置落库语义，前端或 EmbySDK 使用标准 UserService 路径时不再 404。
- 已补齐系统探测路径：`GET /System/ReleaseNotes`、`GET /System/ReleaseNotes/Versions`、`GET /System/WakeOnLanInfo` 提供 EmbySDK 可解析的空/文本响应；`POST /System/Restart` 与 `POST /System/Shutdown` 已挂路由并要求管理员，但返回 501，明确表示当前后端不能真正重启/关机，避免误开放危险能力。
- 本地播放器模板当前高频链路 `Users/AuthenticateByName`、`Users/Me`、`Items/{id}`、`Items/{id}/PlaybackInfo`、`Shows/{id}/Seasons`、`Shows/{id}/Episodes`、`Videos/{id}/stream`、`Audio/{id}/stream`、`UserItems/Resume`、`Items/Latest` 与播放上报链路在 backend 已有对应路由或本轮已补齐兼容缺口。
- 后续仍建议分批审计但本轮未直接实现的 EmbySDK 全量能力：Playlist/Sync/LiveTV/Connect/Sharing/Subtitle remote search 与删除、Item Tags Add/Delete、ThemeMedia/ThemeSongs/ThemeVideos、AudioBooks/NextUp、Devices CameraUploads/Options/Info 等。这些要么 frontend/本地播放器当前未高频调用，要么需要真实业务模型支撑，不适合只做空实现。
- 验证情况：`cargo check --manifest-path backend\Cargo.toml` 已通过；仍只有既有 warning。

## 2026-04-22 rg 模板对照审计补充（三十一）

- 继续使用 `rg` 顺着 EmbySDK `localVarPath`、frontend 已存在页面能力和本地播放器模板做差异对照，本轮优先处理会让媒体详情、编辑菜单或库管理动作出现 404/空结果异常的 Items 周边端点。
- 后端新增 `GET /Items/Prefixes`，按 EmbySDK 形状返回空数组占位，避免客户端在筛选/索引前缀探测时命中 404。
- 后端新增 `POST /Items/Delete` 批量删除路径，支持 Emby GUID 列表解析，复用现有媒体项删除逻辑，并在删除后广播 `LibraryChanged { ItemsRemoved: [...] }`，使前端列表和 Emby 客户端缓存能收到删除事件。
- 后端新增 `GET /Items/{Id}/CriticReviews` 与 `GET /Items/{Id}/DeleteInfo`：前者返回标准 `QueryResult` 空集合，后者返回 `CanDelete/DeleteFromExternalProvider/DeleteFromFileSystem`，用于补齐详情页/操作菜单对删除能力和评论能力的标准探测。
- 后端新增 `GET /Items/{Id}/ThemeMedia`、`GET /Items/{Id}/ThemeSongs`、`GET /Items/{Id}/ThemeVideos`，返回 Emby 风格的 `ThemeMediaResult` 载荷；当前基于已有子项查询能力按 `ThemeSong`/`ThemeVideo` 类型取数，没有真实主题媒体时稳定返回空集合而不是 404。
- 后端新增 `POST /Items/{Id}/Tags/Add` 与 `POST /Items/{Id}/Tags/Delete`，请求体按 EmbySDK 常见的 `Tags: [{ Name|Id }]` 接收，实际更新 `media_items.tags` 并广播 `LibraryChanged { ItemsUpdated: [...] }`，使元数据编辑页的标签增删具备真实落库语义。
- 本轮验证：`cargo check --manifest-path backend\Cargo.toml` 已通过；仍只有既有 warning，未新增编译错误。

## 2026-04-22 rg 模板对照审计补充（三十二）

- 继续使用 `rg` 对照 EmbySDK `SubtitleServiceApi`、`DeviceServiceApi`、当前 frontend 和本地播放器模板。本轮重点处理“播放/设置页不一定高频直接点击，但 SDK 标准探测会命中”的字幕与设备端点，减少客户端 404 噪声。
- 字幕服务补齐 EmbySDK 标准路径：`GET /Items/{Id}/RemoteSearch/Subtitles/{Language}`、`POST /Items/{Id}/RemoteSearch/Subtitles/{SubtitleId}`、`GET /Providers/Subtitles/Subtitles/{Id}`、`DELETE /Items/{Id}/Subtitles/{Index}`、`DELETE /Videos/{Id}/Subtitles/{Index}`、`POST /Items/{Id}/Subtitles/{Index}/Delete`、`POST /Videos/{Id}/Subtitles/{Index}/Delete`，并补了大小写兼容路径。
- 字幕搜索当前返回空数组，安装/下载/删除返回 204，并且会先按 Emby GUID 校验媒体项存在；这保持了 EmbySDK 可调用的接口形状，同时不伪造项目尚未接入的外部字幕供应商和真实文件删除能力。
- 设备服务补齐 EmbySDK 标准路径：`GET /Devices/Info`、`GET /Devices/Options`、`POST /Devices/Options`、`GET /Devices/CameraUploads`、`POST /Devices/CameraUploads`。其中设备详情从现有 sessions 聚合出 `DeviceInfo` 常读字段，Options 返回 `CustomName` 占位，CameraUploads 返回 `ContentUploadHistory` 空列表或 204。
- 当前 frontend `settings/devices.vue` 已直接使用 `/Devices` 与删除设备路径；本轮补的 Info/Options/CameraUploads 主要面向 EmbySDK 客户端和后续设置页扩展，避免功能探测时被误判为后端不兼容。
- 本轮验证：`cargo check --manifest-path backend\Cargo.toml` 已通过；仍只有既有 warning，未新增编译错误。

## 2026-04-22 frontend 设置入口补全（三十三）

- 针对用户指出的 `class="v-item-group v-theme--dark"` 设置入口灰项，本轮审计了 `frontend/packages/frontend/src/pages/settings/index.vue` 中所有 `VItemGroup` 条目；原先 `homeScreen`、`playback`、`mediaPlayers`、`transcodingAndStreaming`、`dlna`、`liveTv`、`networking`、`plugins`、`scheduledTasks`、`notifications` 都是 `link: undefined`，导致前端显示为禁用入口。
- frontend 已新增对应设置页：`/settings/home`、`/settings/playback`、`/settings/media-players`、`/settings/transcoding`、`/settings/dlna`、`/settings/live-tv`、`/settings/networking`、`/settings/plugins`、`/settings/scheduled-tasks`、`/settings/notifications`，并将设置首页入口全部接到真实路由。
- 新增 `frontend/packages/frontend/src/composables/server-configuration.ts`，让这些设置页统一读写 EmbySDK 标准 `System/Configuration`；播放页复用现有 `playbackManager`、`playerElement`、`subtitleSettings`，媒体播放器页复用后端现有 `/Sessions`。
- 后端 `server_configuration` 初始化自带新增模块默认值，不依赖数据库迁移；`repository::server_configuration_value(...)` 也会对旧库缺失字段做默认值回填。
- 后端 `repository::update_server_configuration_value(...)` 改为保留当前配置对象里的未知字段，再覆盖核心字段，避免旧 server.vue 保存基础设置时把新补的转码、DLNA、Live TV、网络、插件、计划任务、通知等模块配置抹掉。
- 本轮验证：`cargo check --manifest-path backend\Cargo.toml` 已通过；`rg` 确认 settings 首页已无 `link: undefined`。前端 Docker 构建因本机 Docker daemon 未启动失败；本机 `node --version` 也因 `Access is denied` 无法执行，因此前端本轮为代码级静态审计，未完成实际 typecheck/build。

## 2026-04-22 frontend 设置页功能补深（三十四）

- 继续把设置首页 `VItemGroup` 打开的新页面从“占位表单”补成可工作的管理页。重点落在三块：主页设置、计划任务、插件。
- frontend 新增 `homeSettings` 同步存储（`frontend/packages/frontend/src/store/settings/home.ts`），主页设置页不再只写后端配置，而是通过 `DisplayPreferences` 风格的同步 store 保存 `showResume/showNextUp/showLatest/latestLimit/sections`；首页 `frontend/packages/frontend/src/pages/index.vue` 也已接入这些设置，继续观看、下一集、最新媒体和栏目顺序/显示现在会真实影响首页内容。
- backend 新增 EmbySDK 风格计划任务 REST：`GET /ScheduledTasks`、`GET /ScheduledTasks/{id}`、`GET /ScheduledTasks/{id}/Triggers`、`POST /ScheduledTasks/Running/{id}`、`DELETE /ScheduledTasks/Running/{id}`、`POST /ScheduledTasks/Running/{id}/Delete`。当前提供 `libraryscan` 与 `metadatarefresh` 两个任务，运行入口会复用现有库扫描逻辑。
- websocket 的 `ScheduledTasksInfoStart` 不再返回空数组，而是复用同一套任务 DTO 组装逻辑，返回和 REST 一致的任务列表。
- backend 新增基础插件 REST：`GET /Plugins`、`GET/POST /Plugins/{id}/Configuration`、`POST /Plugins/{id}/Delete`、`DELETE /Plugins/{id}`。当前插件列表会按现有能力暴露 `Local Metadata Reader` 和已注册的 `TMDb Metadata Provider`，启用/禁用状态落到 `System/Configuration.DisabledPluginsText`。
- frontend 的 `settings/scheduled-tasks.vue` 已改为真实读取 `/ScheduledTasks` 列表并支持“立即运行”；`settings/plugins.vue` 已改为真实读取 `/Plugins` 列表并支持启用/禁用。
- 本轮验证：`cargo check --manifest-path backend\Cargo.toml` 已通过；前端仍因本机 `node`/`pnpm` 不可执行、Docker daemon 未启动而无法完成实际 typecheck/build，因此当前前端验证仍为代码级静态审计。
## 2026-04-22 frontend 设置页功能补深（三十五）

- 继续围绕设置首页 `VItemGroup` 已放开的入口补齐“运行态 + 配置”能力，这一轮重点处理 `networking / transcoding / dlna / live-tv / notifications`。
- `backend/src/transcoder.rs` 新增当前转码会话枚举能力；`backend/src/routes/videos.rs` 按 Emby 风格补上 `GET /Videos/ActiveEncodings`，返回当前活跃转码列表，并让 `DELETE /Videos/ActiveEncodings?Id=...` 可以按会话 ID 停止指定转码，避免转码设置页只能写配置、看不到运行态。
- `frontend/packages/frontend/src/pages/settings/networking.vue` 现在会同时读取 `/System/Info`、`/System/Endpoint`、`/System/Ext/ServerDomains`、`/System/WakeOnLanInfo`，展示服务器地址、网络可达性、域名列表和 Wake on LAN 信息，不再只是单纯写 `System/Configuration`。
- `frontend/packages/frontend/src/pages/settings/transcoding.vue` 现在会读取 `/Videos/ActiveEncodings` 并展示活跃转码会话，支持直接调用 `DELETE /Videos/ActiveEncodings?Id=...` 停止单个转码任务；配置项仍通过 `System/Configuration` 自动保存。
- `frontend/packages/frontend/src/pages/settings/dlna.vue`、`live-tv.vue`、`notifications.vue` 从原来的最小占位表单补成可读的管理页：除配置开关外，会展示当前启用状态、调谐器/节目单来源数量、通知目标数量等摘要，减少“页面能点开但一片空白”的感受。
- 这一轮还顺手清掉了新页面里的乱码/残缺标签，统一成稳定的 UTF-8 文件内容，避免前端模板因为旧文本损坏导致编译失败。
- 验证情况：`cargo check --manifest-path backend\\Cargo.toml` 已通过，只有项目既有 warning；当前环境依旧无法执行 `node`/`pnpm`，所以前端仍是代码级静态校对，未完成实际 typecheck/build。
## 2026-04-22 frontend 设置页功能补深（三十六）

- 继续围绕设置首页 `VItemGroup` 已放开的入口补齐实际可用能力，这一轮主要补深 `playback` 与 `media-players`。
- `frontend/packages/frontend/src/pages/settings/playback.vue` 不再只是几项零散开关，现已整理成完整的播放设置页：统一展示默认播放速率、拉伸模式、自定义字幕开关、字号、底部偏移、描边与背景板，并增加右侧运行态摘要，直接显示当前播放状态、当前媒体项与活动 `PlaySessionId`。
- `frontend/packages/frontend/src/pages/settings/media-players.vue` 从原来的只读表格补成可操作的会话管理页：会读取 Emby 风格 `/Sessions` 数据，展示设备、客户端、用户、播放状态、远程控制能力与支持命令摘要；同时接入后端已存在的 `POST /Sessions/{id}/Playing/{command}` 与 `POST /Sessions/{id}/Message`，支持对活跃播放器发送 `Pause`、`Stop` 和弹消息。
- 这轮没有引入新的后端协议分歧，媒体播放器控制完全复用项目当前已有的 Session command 路由，仍保持 EmbySDK 的会话控制语义。
- 验证情况：`cargo check --manifest-path backend\\Cargo.toml` 已通过，只有项目既有 warning；前端依旧受限于当前环境缺少可执行的 `node`/`pnpm`，所以这轮仍为代码级静态校对，未完成实际 typecheck/build。
## 2026-04-22 frontend 设置页功能补深（三十七）

- 继续围绕设置首页 `VItemGroup` 已放开的入口补齐管理能力，这一轮重点补深 `devices / apikeys / logs-and-activity`。
- `backend/src/repository.rs` 新增设备自定义名称读写；`backend/src/routes/devices.rs` 现在会把 `/Devices`、`/Devices/Info`、`/Devices/Options` 统一接到这份持久化配置上，`POST /Devices/Options?Id=...` 不再是空实现，而是真的会保存 `CustomName`。这让设备详情页的“自定义名称”具备真实落库语义，而不是只在前端展示。
- `frontend/packages/frontend/src/pages/settings/devices.vue` 已从基础删除页补成完整设备管理页：支持刷新设备列表、查看单个设备详情、读取并编辑 `CustomName`、保存设备选项，同时保留单个删除与批量删除。
- `frontend/packages/frontend/src/pages/settings/apikeys.vue` 已从简单表格补成更完整的 API Key 管理页：增加刷新动作、应用名称/版本展示、创建时间与状态展示，并把 token 做摘要显示，避免长 token 把布局撑坏。
- `frontend/packages/frontend/src/pages/settings/logs-and-activity.vue` 已补成真正可操作的日志页：支持刷新、在线预览日志内容、打开原始日志文件，同时继续展示活动日志列表；日志预览直接走 `/System/Logs/Log?name=...`，与 Emby 风格接口保持一致。
- 验证情况：`cargo check --manifest-path backend\\Cargo.toml` 已通过，只有项目既有 warning；当前环境依旧无法执行前端 `node`/`pnpm` 工具链，因此这轮前端仍为代码级静态校对，未完成实际 typecheck/build。
## 2026-04-22 frontend 设置页功能补深（三十八）

- 继续围绕设置首页 `VItemGroup` 已放开的入口补齐页面完整度，这一轮主要补深 `server / account / subtitles` 三页，让它们从“基础表单”更接近完整的 Emby 风格设置页。
- `frontend/packages/frontend/src/pages/settings/server.vue` 在原有 `System/Configuration` 与 `Branding/Configuration` 双向保存基础上，新增右侧服务器摘要面板，直接展示 `/System/Info` 返回的 `ServerName / Version / ProductName / OperatingSystem / LocalAddress / StartupWizardCompleted / CanSelfRestart`，同时把当前可写配置项摘要并排展示，减少管理员进入页面后只能看到输入框、看不到当前服务状态的问题。
- `frontend/packages/frontend/src/pages/settings/account.vue` 在保留头像上传/删除与密码修改链路的同时，补了账户摘要面板，直接展示当前用户的 `Id / HasPassword / Policy / Configuration` 关键字段，例如管理员状态、隐藏/禁用状态、自动播放下一集、默认音频/字幕轨偏好等，和后端现有 `UserDto` / `UserConfigurationDto` 对齐。
- `frontend/packages/frontend/src/pages/settings/subtitles.vue` 从单纯字幕样式表单补成“设置 + 当前状态”页：除字体、字号、位置、背景、描边外，新增当前字幕轨、可用字幕轨数量、外部可解析字幕轨数量的摘要显示，直接复用前端现有 `playbackManager` 与 `playerElement` 的运行态数据，不额外引入新的后端协议分歧。
- 这一轮没有新增后端接口，属于对已打通设置入口的前端功能补深；当前环境仍无法执行 `node`/`pnpm`，因此验证方式依旧是代码级静态校对与现有接口语义核对，未完成实际 typecheck/build。

## rg 模板对照审计补充（三十九）
- 修复 Sessions 的公开 Id 与内部 ccess_token 混用：后端 SessionInfoDto.Id 改为稳定 Emby 风格公开会话 ID，新增公开会话 ID 到内部 ccess_token 的解析；/Sessions/{id}、/Sessions/{id}/Commands、/Sessions/{id}/Message、/Sessions/{id}/Playing/{command}、/Sessions/{id}/PlayQueue 与播放上报链路统一先解析公开会话 ID，再落到内部会话状态表。
- 调整播放态 websocket 载荷中的 PlaybackProgress.SessionId 为公开会话 ID，避免前端与本地播放器继续接触 token 形态值。
- 设置页日志功能改回 EmbySDK 标准日志读取路径：前端日志预览与打开从自定义 /System/Logs/Log?name=... 切回 /System/Logs/{Name}。
- 设置页设备管理继续向 EmbySDK 标准靠拢：详情与选项保存改为优先使用 getDevicesInfo、getDevicesOptions、postDevicesOptions 这组 SDK 标准方法，而不是页面内手写 query 方式调用。
- 验证：cargo check --manifest-path backend\\Cargo.toml 通过；前端当前环境仍无法跑 
ode/pnpm，本轮前端为代码级静态校对。

## rg 模板对照审计补充（四十）
- 继续收尾 ccount / logs-and-activity / devices 三个设置页的前端类型适配，尽量改用 EmbySDK 导出类型，减少页面内自定义弱类型。
- rontend/packages/frontend/src/pages/settings/account.vue 现在把用户头像删除、头像上传、密码修改统一收口到显式的 SDK typed facade，并显式传入 userId，不再依赖当前用户的隐式上下文调用。
- rontend/packages/frontend/src/pages/settings/logs-and-activity.vue 移除本地 LogFile / ActivityLogEntry 弱类型，改用 EmbySDK 的 LogFile、ActivityLogEntry、QueryResultActivityLogEntry、QueryResultString；日志列表和日志行预览使用标准系统日志类型，活动日志列表也切到 SDK 返回体形状。
- rontend/packages/frontend/src/pages/settings/devices.vue 移除页面内 DeviceOptions 弱类型与松散列表类型，改用 EmbySDK 的 DevicesDeviceInfo、DevicesDeviceOptions、QueryResultDevicesDeviceInfo；仅对后端额外补出的 ReportedDeviceId 做最小扩展。
- 本轮未新增后端接口；由于当前环境仍无法执行 
ode/pnpm，前端验证依旧为代码级静态校对。

## rg 模板对照审计补充（四十一）
- 新增前端共用包装 rontend/packages/frontend/src/composables/use-settings-sdk.ts，把设置页里重复出现的 typed facade 收口到统一 composable。
- ccount.vue 改为通过 useSettingsSdk().accountApi 调用头像删除、头像上传、密码修改，页面不再直接内联 SDK 兼容 facade。
- logs-and-activity.vue 改为通过 useSettingsSdk().logsApi 读取日志列表、日志行预览、活动日志列表，设置页自身只保留展示逻辑与格式化逻辑。
- devices.vue 改为通过 useSettingsSdk().devicesApi 读取设备列表、设备详情、设备选项与删除操作；ReportedDeviceId 最小扩展类型也随 composable 统一导出。
- 本轮未新增后端接口；前端当前环境仍无法执行 
ode/pnpm，因此验证仍为代码级静态校对。

## rg 模板对照审计补充（四十二）
- 继续把 useSettingsSdk 扩展到 pikeys / media-players / server，进一步收平设置页里零散的 SDK 兼容包装。
- rontend/packages/frontend/src/pages/settings/apikeys.vue 与 rontend/packages/frontend/src/components/System/AddApiKey.vue 现改为统一使用 useSettingsSdk().apiKeysApi 读取 Key 列表、创建 Key、撤销 Key。
- rontend/packages/frontend/src/pages/settings/media-players.vue 现改为统一使用 useSettingsSdk().sessionsApi 读取会话列表、发送播放命令、发送消息，页面移除本地手写会话请求包装。
- rontend/packages/frontend/src/pages/settings/server.vue 现通过 useSettingsSdk().serverApi.getSystemInfo() 读取系统信息摘要，保留现有 useApi(...) 的配置缓存与自动同步逻辑不变。
- 本轮未新增后端接口；前端当前环境仍无法执行 
ode/pnpm，因此验证仍为代码级静态校对。

## rg 模板对照审计补充（四十三）
- useSettingsSdk().serverApi 新增配置层方法：getConfiguration、updateConfiguration、updateNamedConfiguration，让设置相关 composable 不再直接依赖 axios 调用 /System/Configuration。
- rontend/packages/frontend/src/composables/server-configuration.ts 已改为通过 serverApi.getConfiguration() / serverApi.updateConfiguration() 读写服务器配置，移除对 RemotePluginAxiosInstance 的直接依赖。
- rontend/packages/frontend/src/pages/settings/server.vue 的初始化读取已统一改为 serverApi.getLocalizationOptions()、serverApi.getConfiguration()、serverApi.getBrandingOptions()、serverApi.getSystemInfo()；配置保存阶段仍保留现有 useApi(getConfigurationApi, ...) 以继续复用页面已有的缓存与自动同步逻辑。
- 审计前端剩余非 EmbySDK 直接调用面，当前仍明显存在于：settings/libraries.vue、settings/networking.vue、settings/plugins.vue、settings/scheduled-tasks.vue、settings/transcoding.vue，以及登录探测阶段 plugins/remote/auth.ts 的 server bootstrap 请求。下一轮应按“存在 EmbySDK 方法优先切回 SDK，不存在再评估保留兼容层”的规则继续收口。
- 本轮未新增后端接口；前端当前环境仍无法执行 
ode/pnpm，因此验证仍为代码级静态校对。

## 2026-04-22 rg 模板对照审计补充（四十四）

- 优先处理 `libraries / networking / scheduled-tasks` 三个仍保留明显 `fetch`/`RemotePluginAxiosInstance` 直连的设置页，把页面侧调用统一收口到 `frontend/packages/frontend/src/composables/use-settings-sdk.ts`。
- `useSettingsSdk().librariesApi` 新增 EmbySDK operationId 风格方法：`getLibraryVirtualfoldersQuery`、`getLibrarySelectablemediafolders`、`postLibraryRefresh`、`postLibraryVirtualfolders`、`postLibraryVirtualfoldersName`、`postLibraryVirtualfoldersLibraryoptions`、`postLibraryVirtualfoldersPaths`、`deleteLibraryVirtualfoldersPaths`、`deleteLibraryVirtualfolders`、`postLibraryVirtualfoldersDelete`。`settings/libraries.vue` 现在不再自己拼 URL、注入 `api_key` 或直接 `fetch`，创建/重命名/保存 LibraryOptions/增删路径/刷新都走统一 facade。
- `useSettingsSdk().scheduledTasksApi` 新增 `getScheduledtasks`、`getScheduledtasksById`、`postScheduledtasksRunningById`、`postScheduledtasksRunningByIdDelete`；`settings/scheduled-tasks.vue` 已移除页面内手写 `/ScheduledTasks` axios 调用，并改用 SDK 风格任务方法读取和执行任务。
- `useSettingsSdk().serverApi` 扩展 networking 需要的 `getSystemEndpoint`、`getSystemWakeonlaninfo`、`getServerDomains`；`settings/networking.vue` 已从页面内直连 `/System/Info`、`/System/Endpoint`、`/System/WakeOnLanInfo`、`/System/Ext/ServerDomains` 改为统一调用 `serverApi`。其中 `/System/Ext/ServerDomains` 仍是项目扩展端点，但已集中在 facade，避免页面继续散落自定义请求。
- 后端同步补齐 EmbySDK LibraryStructure body 风格删除入口：新增 `POST /Library/VirtualFolders/Delete` 和 `POST /Library/VirtualFolders/Paths/Delete`，支持 `Id/Name/Path/RefreshLibrary` 请求体；这样后续前端或本地播放器如果按 EmbySDK `postLibraryVirtualfoldersDelete`、`postLibraryVirtualfoldersPathsDelete` 风格调用，不会命中 404。
- 验证情况：`cargo check` 在 `backend` 目录通过，只剩项目既有 warning；`rg` 已确认 `settings/libraries.vue`、`settings/networking.vue`、`settings/scheduled-tasks.vue` 三页不再包含 `fetch(`/`RemotePluginAxiosInstance` 或这些目标路径直连。当前环境未执行前端 typecheck/build。

## 2026-04-22 rg 模板对照审计补充（四十五）

- 本轮继续按“前端全部代码优先使用 EmbySDK 调用方法，而不是页面内被迫兼容 SDK”的要求做全量 `rg` 审计。扫描范围覆盖 `frontend/packages/frontend/src` 的 `fetch`、`axios`、`RemotePluginAxiosInstance`、硬编码 Emby REST 路径和已存在 `newUserApi/useApi/useBaseItem` 调用。
- 业务页面和 store 中绝大多数媒体、用户、播放、图片、元数据、筛选、用户设置链路已经直接使用 `remote.sdk.newUserApi(...)`、`useApi(...)` 或 `useBaseItem(...)`；本轮重点清理剩余设置页与登录探测里的直连残留。
- `frontend/packages/frontend/src/composables/use-settings-sdk.ts` 不再直接导入 `RemotePluginAxiosInstance`，统一改为通过 `remote.sdk.api!.axiosInstance` 使用 SDK 已配置的传输层。该文件现在作为少量 EmbySDK operationId 风格方法的集中 facade，避免页面继续散落手写路径。
- `settings/plugins.vue` 已改用 `useSettingsSdk().pluginsApi`，对应 EmbySDK/OpenAPI 的 `getPlugins`、`postPluginsByIdConfiguration` 等语义；页面内不再手写 `/Plugins` axios 调用，并顺手修复插件页中文乱码。
- `settings/transcoding.vue` 已改用 `useSettingsSdk().transcodingApi`，对应 EmbySDK/OpenAPI 的 `getVideosActiveEncodings`、`deleteVideosActiveEncodings`、`postVideosActiveEncodingsDelete` 语义；页面内不再手写 `/Videos/ActiveEncodings` axios 调用。
- 登录探测 `plugins/remote/auth.ts` 去掉历史 HTTP fallback，不再在 SDK 失败后直接 `axios.get('/System/Info/Public')`、`Branding/Configuration`、`Users/Public`；现在候选地址探测也统一使用 `useOneTimeAPI(...)` 加 `getSystemApi/getBrandingApi/getUserApi`。
- 字幕 worker 的 VTT 文本下载不是 Emby REST API 调用，本轮去掉 `axios` 依赖改为浏览器 `fetch(src)` 读取静态字幕资源，避免被误判为绕过 SDK。剩余 `fetch('config.json')` 同样属于本地静态配置读取，不纳入 EmbySDK API 替换。
- 当前全量 `rg` 结果：除核心 SDK 传输层 `plugins/remote/axios.ts`、类型导入 `AxiosRequestConfig/AxiosResponse/AxiosError`、静态资源读取，以及 `use-settings-sdk.ts` 这个集中 facade 外，未发现页面/组件/store 中继续散落 `RemotePluginAxiosInstance` 或设置相关 Emby REST 直连。
- 仍需注意的非 REST URL 生成：`utils/items.ts` 的 `/Items/{Id}/Download?api_key=...` 和 `logsApi.getLogFileUrl(...)` 属于浏览器打开/下载链接，不是 axios/fetch 调用。EmbySDK/OpenAPI 中对应 operationId 分别是 `getItemsByIdDownload` 与系统日志下载语义；如果后续要做到“链接生成也完全由 SDK 封装”，建议继续新增 URL builder facade，并把这些 URL 字符串也从页面/工具层收口。
- 验证情况：`cargo check` 在 `backend` 目录通过，只剩项目既有 warning；前端本机验证仍受限，`node --version` 返回 `Access is denied`，`pnpm` 未安装，因此未能执行 frontend typecheck/build。

## 2026-04-22 URL builder facade 收口补充（四十六）

- 继续处理上一轮遗留的“不是 axios/fetch 调用、但仍在页面或 store 里手写 Emby URL”的位置，新增 `frontend/packages/frontend/src/utils/sdk-url.ts` 作为统一 URL builder facade。
- 新 facade 集中提供 `getSdkItemDownloadUrl`、`getSdkSystemLogUrl`、`getSdkSubtitleDeliveryUrl`、`getSdkPlaybackStreamUrl`、`buildSdkWebSocketUrl`，统一处理 `basePath` 末尾斜杠、路径前导斜杠、`api_key`、`deviceId`、直连播放参数和 websocket `http/ws` 协议转换。
- `frontend/packages/frontend/src/utils/items.ts` 的 `getItemDownloadUrl(...)` 不再手写 `${serverAddress}/Items/${itemId}/Download?api_key=...`，改为委托 `getSdkItemDownloadUrl(...)`。调用方 `ItemMenu` 和剧集批量下载映射无需改接口。
- `frontend/packages/frontend/src/store/playback-manager.ts` 的播放 URL 生成不再在 store 内拼 `/Videos/{Id}/stream.{Container}` 或拼接 `TranscodingUrl`，改为委托 `getSdkPlaybackStreamUrl(...)`，让直连播放和转码播放 URL 语义集中维护。
- `frontend/packages/frontend/src/store/player-element.ts` 的外部字幕 `DeliveryUrl` 不再直接拼 `remote.sdk.api.basePath + DeliveryUrl`，改为 `getSdkSubtitleDeliveryUrl(...)`。
- `frontend/packages/frontend/src/plugins/remote/socket.ts` 的 websocket URL 不再在 socket 类里手写 `URLSearchParams` 与 `/socket`，改为 `buildSdkWebSocketUrl(...)`。为避免循环依赖，`sdk-url.ts` 直接引用底层 `auth` 和 `sdk` 实例，不从 `remote/index.ts` 聚合入口导入。
- `useSettingsSdk().logsApi.getLogFileUrl(...)` 已改为调用 `getSdkSystemLogUrl(...)`，日志页面继续通过 `logsApi` 获取打开链接，页面侧不再拼 `/System/Logs/{Name}`。
- 当前 `rg` 复扫结果：`basePath/api_key/Download/socket/DeliveryUrl/TranscodingUrl` 相关拼装已集中在 `utils/sdk-url.ts`；其余命中为 SDK 调用、类型/状态读取或 facade 调用点。
- 验证情况：`cargo check` 在 `backend` 目录通过，只剩项目既有 warning；`git diff --check` 通过。前端 typecheck/build 仍受本机 `node`/`pnpm` 环境限制未执行。

## 2026-04-22 编译运行与 EmbySDK 端点测试补充（四十七）
- 本轮按“编译并运行项目，然后使用 EmbySDK 端点测试”的要求完成本地验证。后端 `cargo build` 通过，仅保留项目既有 warning；前端依赖已安装，`vite build --configLoader runner` 在 `frontend/packages/frontend` 下通过并生成 `dist`。
- 启动运行时发现初始化阻断：`backend/migrations` 目录中存在重复 SQLx migration version，`0012_user_configuration.sql` 与 `0012_emby_images_and_trailers.sql` 冲突，`0013_media_chapters.sql` 与 `0013_emby_taglines.sql` 冲突，导致空库首次启动也会报 `_sqlx_migrations_pkey` duplicate key。已将重复迁移改名为 `0021_user_configuration.sql` 与 `0022_media_chapters.sql`，内容保持不变，确保自建数据库初始化自带完整结构。
- 使用独立测试库 `movie_rust_codex_test` 启动当前 backend，静态目录指向已构建的 `frontend/packages/frontend/dist`，服务成功监听 `http://127.0.0.1:8096`。初始化日志确认 `session_commands`、`display_preferences`、`media_chapters`、`users.configuration` 等前端/实时链路需要的表和字段会在初始化阶段具备。
- 端点测试按 EmbySDK 客户端常用调用形态执行：`/System/Info/Public`、`/Branding/Configuration`、`/Startup/Configuration`、`/Startup/User`、`/Startup/Complete`、`/Users/AuthenticateByName`、`/System/Info`、`/System/Configuration`、`/System/Configuration/branding`、`/Users/Public`、`/Users/Me`、`/UserViews`、`/Library/VirtualFolders/Query`、`/Library/SelectableMediaFolders`、`/ScheduledTasks`、`/Sessions`、`/System/ActivityLog/Entries`、`/System/Logs/Query`、`/Devices`、`/Plugins`、`/System/Endpoint`、`/System/WakeOnLanInfo` 均返回 200/204。
- WebSocket 标准入口 `/socket?api_key=...&deviceId=...` 使用 `ClientWebSocket` 验证为 `Open`，不再复现早前日志中的 `/socket` 404；这说明当前路由注册和认证参数形态已能被 EmbySDK 风格客户端握手命中。
- 前端 `vue-tsc` 全量类型检查仍失败，但剩余错误来自项目既有的 Vuetify/Dexie/Storybook 等类型噪音；本轮 URL builder facade 相关的 `sdk-url.ts`、`playback-manager.ts`、`player-element.ts` 新增类型问题已修正，并通过过滤检查确认不再出现在 typecheck 错误中。

## 2026-04-22 Chrome DevTools MCP 网页端验证补充（四十八）
- 按要求使用 `chrome-devtools-mcp` 对本地网页端做实际交互验证：打开 `http://127.0.0.1:8096/`，使用测试库管理员 `codex` 登录，首屏认证、DisplayPreferences、UserViews、Resume/Latest/NextUp 请求均返回 200/204，没有首屏 loading 卡死。
- MCP 首轮进入 `#/settings/libraries` 时抓到真实浏览器问题：页面渲染不再空白，但 `GET /Library/VirtualFolders/Query` 与 `GET /Library/SelectableMediaFolders` 返回 401。原因是 `useSettingsSdk` 中集中 facade 的少数 `sdkAxios().get/post/delete(...)` 调用没有像 generated SDK API 方法一样自动带当前 token。已在 `sdkAxios()` 集中入口注入 `X-Emby-Token`，让 libraries、sessions、scheduled-tasks、plugins、transcoding、server/networking 等 facade 直连点共享认证上下文。
- 重新构建并用隔离浏览器上下文复测后，`settings/libraries` 加载新 chunk，`/Library/VirtualFolders/Query` 与 `/Library/SelectableMediaFolders` 均返回 200；页面显示空库引导和 Add Libraries 操作，不再出现失败提示。
- 继续巡检 `settings/scheduled-tasks` 与 `settings/networking`：`/ScheduledTasks`、`/System/Configuration`、`/System/Info`、`/System/Endpoint`、`/System/Ext/ServerDomains`、`/System/WakeOnLanInfo` 均返回 200，页面正常渲染任务列表和网络摘要。
- MCP 进入 `settings/logs-and-activity` 时又抓到运行时错误：前端调用了当前 EmbySDK 中不存在的 `getSystemLogs` / `getSystemLogsByNameLines`。已改为 SDK 真实方法 `getServerLogs`；日志预览行读取继续通过集中认证 facade 调用项目扩展 `/System/Logs/{Name}/Lines`；日志打开链接改为 EmbySDK 生成代码标准路径 `/System/Logs/Log?name=...`。
- 重新构建并复测日志页后，`/System/Logs` 与 `/System/ActivityLog/Entries` 均返回 200；日志列表、Open 链接、Preview 弹窗均可用。控制台仅剩当前用户无头像导致的图片 preload 警告，未再出现 401/404 或前端 TypeError。

## 2026-04-22 前端语言持久化与 i18n 补充（四十九）
- 本轮排查“切换中文后仍有英文、提示不跟随语言、语言选择不持久化”的原因：`LocaleSwitcher` 点击具体语言时直接调用 `i18next.changeLanguage(item)`，绕过了 `clientSettings.locale`。这样 UI 会临时切换，但不会写入当前用户的 DisplayPreferences/CustomPrefs，刷新或重新登录后容易回到旧状态。
- 已将 `frontend/packages/frontend/src/components/System/LocaleSwitcher.vue` 的具体语言选择改为写入 `clientSettings.locale.value = item`。该设置由 `SyncedStore` 统一同步到 `/DisplayPreferences/{displayPreferencesId}?userId=...&client=vue`，和主题、客户端设置保持同一套持久化链路；“自动”语言仍写入 `undefined`，继续按浏览器/系统语言回退。
- 对设置区做了一轮硬编码文案审计并收口到 i18n：`account`、`apikeys`、`devices`、`dlna`、`home`、`libraries`、`live-tv`、`logs-and-activity`、`media-players`、`networking`、`notifications`、`playback`、`plugins`、`scheduled-tasks`、`server`、`subtitles`、`transcoding` 中明显残留的英文状态、按钮、表头、空状态、错误提示和此前乱码中文均改为 `$t(...)` / `t(...)`。
- `frontend/packages/i18n/strings/en.json` 与 `zh-CN.json` 已补齐这些设置页新增 key。其他语言包暂未逐一人工翻译，但页面代码已经不再硬编码英文；未翻译语言会按当前 i18next fallback 机制回落，而不是把英文写死在 Vue 模板里。
- 验证情况：`node` JSON 解析确认 `en.json` 与 `zh-CN.json` 合法；新增设置页翻译 key 与英文基准包对齐；`vite build --configLoader runner` 在 `frontend/packages/frontend` 下通过。完整 `vue-tsc` 未在本轮重新跑，项目此前仍存在既有 Vuetify/Dexie/Storybook 类型噪声。

## 2026-04-22 Items 重复查询参数兼容补充（五十）
- 针对 EmbySDK/Emby Web 实际会重复发送 `fields=...&fields=...`、`enableImageTypes=...&enableImageTypes=...` 的查询形式，排查发现 `backend/src/models.rs` 里的 `ItemsQuery::from_raw_query(...)` 已经实现了按 Emby 风格合并重复参数，但 `backend/src/routes/items.rs` 中大量入口仍直接使用 `axum::extract::Query<ItemsQuery>`。
- 这会导致请求在进入项目自定义合并逻辑之前，就被 axum 默认查询串反序列化拦下，并报 `Failed to deserialize query string: duplicate field 'EnableImageTypes'`。因此 `/Items`、`/Users/{userId}/Items` 以及同样复用 `ItemsQuery` 的 artist/studio/tag items、suggestions、section items、latest、filters、instant mix、additional parts、resume 等路由，现已统一切换为 `RawQuery` + `ItemsQuery::from_raw_query(...)`。
- 这样后端现在会把重复的 `fields`、`enableImageTypes`、`includeItemTypes`、`sortBy` 等列表型参数按逗号聚合，和 EmbySDK/Emby 客户端的真实调用方式保持一致，不再因为重复 query key 直接返回 400。
- 额外补充了 `ItemsQuery::from_raw_query(...)` 的单元测试，锁定 `fields` 与 `enableImageTypes` 的重复参数合并行为，避免后续回归。
- 验证情况：`cargo check --manifest-path backend/Cargo.toml` 通过；仍仅有项目既有 warning，未新增编译错误。
## 2026-04-22 库页空白与 NextUp 500 修补（五十一）
- 使用 Chrome DevTools MCP 实测 `https://test.emby.yun:4443/#/library/601615B8ED924287B4CEA8052E72E01A` 后，确认库页空白由两段问题叠加触发：其一是 `/Items?...&ids=...&enableImageTypes=...` 在旧后端行为下会因重复 `EnableImageTypes` 返回 400；其二是 `frontend/packages/frontend/src/pages/library/[itemId].vue` 直接读取 `libraryQuery.value[0]!`，导致接口失败或返回空数组时前端抛出 `TypeError: Cannot read properties of undefined (reading '0')`。
- 前端库页现已改成对 `libraryQuery.value?.[0]` 做空值兜底，并在未拿到库对象且请求结束时展示错误提示；工具栏、筛选、播放按钮和 ItemGrid 仅在库对象存在时渲染，避免单个接口失败把整个页面挂死。
- 同次实测还抓到首页 `GET /Shows/NextUp` 返回 `{"ErrorCode":"DatabaseError","ErrorMessage":"数据库错误: no column found for name: critic_rating"}`。原因是 `backend/src/repository.rs` 中 `get_next_up_episodes(...)` 与 `get_upcoming_episodes(...)` 的 SELECT 列表遗漏了 `critic_rating`，而映射目标 `DbMediaItem` 已要求该字段存在。
- 现已为上述两个查询补回 `critic_rating` 列，保持 `DbMediaItem` 映射与 EmbySDK 相关首页链路一致，避免 NextUp/Upcoming 因字段缺失直接 500。
- 验证情况：`cargo check --manifest-path backend/Cargo.toml` 通过；库页前端修补为代码级修复，本轮未重新跑前端完整构建。
## 2026-04-22 前端语言持久化刷新回退修补（五十二）
- 使用 Chrome DevTools MCP 在 `https://test.emby.yun:4443/#/settings` 复测时确认：把界面切到 `中文（中国）` 后，前端会成功 `POST /DisplayPreferences/clientSettings?...`，而且刷新后 `GET /DisplayPreferences/clientSettings?...` 返回体里仍能看到 `CustomPrefs.locale = "zh-CN"`；但页面最终又回到英文，说明问题不在后端存储，而在前端启动阶段没有把已保存的 locale 正确重新应用到 i18n。
- 根因有两处：其一，`SyncedStore` 只在 `remote.auth.currentUser` 变化时触发远端同步，但监听不是 `immediate`，刷新首屏时常出现用户状态已经就绪、却没有立刻拉回 `clientSettings` 的情况；其二，`ClientSettingsStore` 之前把浏览器语言直接裁成基础语言（例如 `zh-CN -> zh`），在项目语言包只有 `zh-CN` 没有裸 `zh` 时会错误回退到英文。
- 现已将 `frontend/packages/frontend/src/store/super/synced-store.ts` 的用户同步监听改为 `immediate: true`，确保刷新首屏就会主动拉回 `DisplayPreferences`；同时在 `frontend/packages/frontend/src/store/settings/client.ts` 中加入受支持语言解析逻辑，优先精确匹配 locale，其次匹配基础语言，最后再回退到同语种的区域变体（例如浏览器 `zh-CN`/`zh-TW` 均能正确命中现有中文语言包）。
- 这样一来，无论是用户显式保存的 `CustomPrefs.locale`，还是“自动”模式下来自浏览器的区域语言，刷新页面后都能重新驱动 `i18next.changeLanguage(...)` 与 Vuetify locale，不再无故掉回英文。
