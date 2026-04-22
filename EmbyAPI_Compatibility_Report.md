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

## 2026-04-23 WebDashboard 基线切换

- 已把 `frontend` 原有 Vue SPA 内容整体移除，并将 `模板项目/Emby模板/MediaBrowser.WebDashboard` 复制到 `frontend/`，后续前端兼容性改为以 Emby WebDashboard 为基线。
- `APP_STATIC_DIR` 默认值已从 `frontend/dist` 改为 `frontend`，以适配新的 dashboard 目录结构。
- 新增 `backend/src/routes/dashboard.rs`，先接通最小可用的 dashboard 托管层。
- 已接通 `GET /`、`GET /web`、`GET /favicon.ico`、`GET /robots.txt`、`GET /web/*`、`GET /web/ConfigurationPages` 和 `GET /web/ConfigurationPage?Name=...` 的首轮路由。
- `GET /web/ConfigurationPages` 当前暂返回空数组，`GET /web/ConfigurationPage?Name=...` 暂返回 404，属于启动保底 stub，后续再按 WebDashboard 实际需求补齐。
- 已通过 `cargo check --manifest-path backend/Cargo.toml`。

### 新基线的当前缺口

- P0: 还没有按 `dashboard-ui/scripts/site.js` 和其他页面脚本的实际 API 调用做全量缺口审计。
- P0: `ConfigurationPages` / `ConfigurationPage` 目前只是启动保底 stub，不是 Emby 真实 dashboard plugin/page 模型。
- P1: 后续需要以 WebDashboard 的真实请求为准，重新截取并补齐 `System`、`Users`、`Library`、`Items`、`DisplayPreferences`、`Sessions`、`Devices`、`ScheduledTasks` 等端点。
- P1: 后续 `EmbyAPI_Compatibility_Report.md` 应以 WebDashboard 基线缺口为主，不再以已删除的 Vue SPA 前端行为为主。

## 2026-04-23 WebDashboard 适配进展（二）

### 本轮已补齐的管理接口
- `GET/POST /System/Configuration`
  - 已接入数据库持久化，默认返回服务器名、UI 语言、元数据国家/语言、远程访问开关等基础配置。
- `GET/POST /System/Configuration/{name}`
  - 已支持按名称读取与保存命名配置，先作为 WebDashboard 配置页的通用存储层。
- `GET /System/Configuration/devices`
  - 已返回设备配置占位结构，避免设备设置页直接报错。
- `GET /System/WakeOnLanInfo`
  - 已返回空数组兼容响应。
- `GET /Localization/Countries`
  - 已返回国家列表对象，包含 `DisplayName`、`TwoLetterISORegionName`、`ThreeLetterISORegionName`。
- `GET /Localization/ParentalRatings`
  - 已返回基础分级列表，支撑用户家长控制页面加载。
- `GET /Environment/DefaultDirectoryBrowser`
- `GET /Environment/Drives`
- `GET /Environment/DirectoryContents`
- `GET /Environment/ParentPath`
- `GET /Environment/NetworkDevices`
- `POST /Environment/ValidatePath`
  - 已实现目录浏览、父路径、驱动器、路径校验等文件系统浏览能力，供目录选择器与媒体库路径编辑器使用。
- `GET /Devices`
- `DELETE /Devices/{id}`
- `POST /Devices/{id}/Delete`
- `GET /Devices/CameraUploads`
  - 已提供基于会话聚合的设备列表和空的相机上传历史响应。
- `GET /Channels`
  - 已返回空的 `Items` 列表兼容结构。
- `GET /ScheduledTasks`
- `POST /ScheduledTasks/Running/{id}`
- `DELETE /ScheduledTasks/Running/{id}`
- `POST /ScheduledTasks/Running/{id}/Delete`
- `POST /ScheduledTasks/{id}/Triggers`
  - 已提供可展示的任务列表和可保存触发器的兼容接口。
- `POST /Users/{id}`
  - 已支持管理员更新用户基础资料中的 `Name`。
- `POST /Users/{id}/Configuration`
  - 已补齐用户配置写入接口。
- `POST /Users/{id}/EasyPassword`
  - 已补齐简单密码接口，内部先复用普通密码逻辑。
- `POST /Users/{id}/Password`
  - 已支持 `ResetPassword=true` 的重置流程，不再直接拒绝。

### 涉及文件
- `backend/src/repository.rs`
- `backend/src/routes/system.rs`
- `backend/src/routes/compat.rs`
- `backend/src/routes/users.rs`
- `backend/src/routes/management.rs`
- `backend/src/routes/mod.rs`

### 当前验证
- `cargo check --manifest-path backend/Cargo.toml` 已通过。

### 下一批优先缺口
- 继续按 WebDashboard 页面真实调用补 `Plugins/*`、`LiveTv/*`、`Connect/*`、更多系统配置页命名配置。
- 实测 `useredit`、`userlibraryaccess`、`userparentalcontrol`、目录浏览器、任务页、设备页对应调用，继续修正字段细节。

### 2026-04-23 WebDashboard 适配进展（三）
- 新增 `backend/src/routes/integrations.rs`，补齐以下外围兼容接口：
  - `GET /Plugins`
  - `GET/POST /Plugins/SecurityInfo`
  - `GET/POST /Plugins/{id}/Configuration`
  - `GET/DELETE /Connect/Pending`
  - `GET /News/Product`
  - `GET /Packages/{id}/Reviews`
- `Users` 路由已补 `DELETE /Users/{id}`，兼容 WebDashboard 用户页直接删除用户的调用方式。
- `cargo check --manifest-path backend/Cargo.toml` 再次通过。

### 2026-04-23 WebDashboard 适配进展（四）
- 修正 `GET /Environment/ParentPath` 为纯文本响应，匹配目录浏览器实际读取方式，不再返回 JSON。
- 新增 `GET /Environment/NetworkShares` 空兼容响应。
- 新增 `backend/src/routes/livetv.rs`，补齐一整组 LiveTV 兼容路由：
  - `LiveTv/Info`
  - `LiveTv/GuideInfo`
  - `LiveTv/Channels/*`
  - `LiveTv/Programs*`
  - `LiveTv/Recordings*`
  - `LiveTv/Timers*`
  - `LiveTv/SeriesTimers*`
  - `LiveTv/Tuners/{id}/Reset`
  - `LiveTv/TunerHosts`
  - `LiveTv/ListingProviders`
- `System/Configuration/livetv` 默认命名配置已提供基础结构，支持 `TunerHosts` 与 `ListingProviders` 持久化更新。
- 新增系统/插件外围兼容接口：
  - `GET /Packages/{name}`
  - `GET /Packages/Updates`
  - `POST /Packages/Installed/{name}`
  - `DELETE /Packages/Installing/{id}`
  - `GET /Registrations/{feature}`
  - `POST /System/Restart`
  - `POST /System/Shutdown`
- `ScheduledTasks` 已补 `RefreshGuide` 任务键，兼容 LiveTV 状态页任务按钮。
- `cargo check --manifest-path backend/Cargo.toml` 已再次通过。

### 2026-04-23 WebDashboard 适配进展（五）
- 新增 `backend/src/routes/client_compat.rs`，补齐客户端/播放器兼容接口：
  - `Notifications/Types`
  - `Notifications/Services`
  - `Notifications/{userId}`
  - `Notifications/{userId}/Summary`
  - `Notifications/{userId}/Read`
  - `Notifications/{userId}/Unread`
  - `Search/Hints`
  - `Playback/BitrateTest`
  - `LiveStreams/MediaInfo`
  - `Sync/OfflineActions`
  - `Sync/Data`
  - `Sync/Items/Ready`
  - `Sync/JobItems/{id}/Transferred`
  - `DELETE /Sync/{targetId}/Items`
- 新增用户 Emby Connect 兼容路由：
  - `POST /Users/{id}/Connect/Link`
  - `DELETE /Users/{id}/Connect/Link`
  - 先用本地持久化方式保存 `ConnectUsername` / `ConnectUserName` / `ConnectUserId` / `ConnectLinkType`。
- 新增 `POST /Packages/Reviews/{id}` 兼容响应。
- `cargo check --manifest-path backend/Cargo.toml` 已再次通过。

## 2026-04-23 WebDashboard 适配进展（六）

### 本轮完成
- 扩展 `UserDto`，补齐 WebDashboard 用户页真实依赖的字段：`ConnectUserName`、`ConnectUserId`、`ConnectLinkType`、`PrimaryImageTag`、`LastActivityDate`。
- 扩展 `UserPolicyDto`，补齐用户编辑与家长控制页会读取/提交的字段：`EnableContentDeletionFromFolders`、`BlockUnratedItems`、`AccessSchedules`。
- 新增 `repository::user_to_dto_with_context`，统一从用户 Connect 关联配置、用户头像路径、会话最后活动时间装配增强版 `UserDto`。
- `Users/Public`、`Users`、`Users/{id}`、`Users/Me`、`Users/New`、`Users/AuthenticateByName`、`Users/{id}` 更新返回、`Startup/User`、`UserSettings` 等入口改为返回增强版 `UserDto`，减少 WebDashboard 用户管理页和登录后页面的字段缺失。
- 扩展 `System/Info` 与 `System/Info/Public`，补齐 dashboard/general、dashboardpage、encoding settings 常读字段：`CanSelfUpdate`、`SupportsAutoRunAtStartup`、`CanLaunchWebBrowser`、`SupportsHttps`、`HasPendingRestart`、`HttpServerPortNumber`、`HttpsPortNumber`、`PackageName`、`SystemUpdateLevel`、`EncoderLocationType`。
- 扩展默认 `System/Configuration` 结构，补齐 hosting/general/library/metadata/streaming 设置页常见字段：端口、HTTPS、UPnP、远程访问、缓存路径、自动更新、匿名统计、远端码率、转码临时目录、元数据路径等，避免页面读取后保存时把字段洗掉。

### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。

### 仍待继续
- 继续按 WebDashboard 页面把 `System/Info`、`System/Configuration/*` 的细节字段补深，尤其是 dashboard 首页、编码页、插件页和高级设置页。
- 继续补 WebDashboard 会探测但当前仍较弱的系统任务、插件安装、Connect 邀请、活动日志与媒体编码相关接口细节。

## 2026-04-23 WebDashboard 适配进展（七）

### 本轮完成
- 扩展 `SystemInfo`，补齐 dashboard 首页实际读取的字段：`WanAddress`、`CachePath`、`LogPath`、`InternalMetadataPath`、`TranscodingTempPath`、`CompletedInstallations`、`IsShuttingDown`。
- `System/Info` 现在会结合 `System/Configuration` 与命名配置 `encoding` 返回 dashboard 首页、编码页所需的缓存路径、元数据路径、转码临时路径等信息。
- 新增 `POST /System/MediaEncoder/Path`，兼容编码设置页保存自定义 FFmpeg/编码器路径的动作，并把结果写回命名配置 `encoding`。
- 为 `encoding` 增加默认命名配置结构：`EncoderAppPath`、`TranscodingTempPath`、`HardwareAccelerationType`、`HardwareDecodingCodecs`、`EnableHardwareEncoding`、`EnableSubtitleExtraction` 等字段。
- 新增 `POST /Connect/Invite`，兼容 guest inviter 组件邀请 Emby Connect 用户的最小流程返回。
- 新增 `GET /System/Logs/Log?name=...`，兼容日志页直接按查询参数打开日志文件内容。
- 扩展活动日志条目字段，补上 `Overview`、`UserId`、`UserPrimaryImageTag`，提升 dashboard 活动组件显示完整度。
- 扩展 `Startup/User`：支持 WebDashboard 启动向导使用 `application/x-www-form-urlencoded` 提交，支持 `ConnectUserName`，并在返回中附带 `UserLinkResult`，兼容 `wizarduserpage.js` 的向导流。

### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。

### 仍待继续
- 继续补 dashboard 首页的插件更新/应用更新链路细节，尤其是 `Packages/Updates` 不同分类对象的字段形状。
- 继续核对活动日志、计划任务、插件安装和 Connect 相关消息流与 WebSocket 推送细节。

## 2026-04-23 WebDashboard 适配进展（八）

### 本轮完成
- 修复 Startup 公开接口安全问题：启动向导未完成时仍允许初始化；一旦 `startup_wizard_completed=true`，`Startup/Configuration`、`Startup/User`、`Startup/RemoteAccess`、`Startup/Complete` 都要求有效管理员认证。
- 修复首次启动向导空用户返回：`GET /Startup/User` 在无用户时返回包含 `Name`、`ConnectUserName`、`Policy`、`Configuration` 等字段的空用户对象，避免 WebDashboard `wizarduserpage.js` 直接访问 `user.Name` 时崩溃。
- 修复旧系统配置升级兼容：`system_configuration` 会把数据库已有配置与当前默认配置做缺字段合并，避免旧部署缺少新增 WebDashboard 字段。
- 修复命名配置升级兼容：`System/Configuration/{name}` 返回时会把已保存配置叠加到默认命名配置上，当前覆盖 `encoding`、`livetv` 等默认结构。
- 修复 Docker 场景的系统地址：`System/Info` 和 `System/Info/Public` 优先使用 `APP_PUBLIC_URL`，否则把 `0.0.0.0` / `::` 转为 `localhost`，避免返回不可访问的 `http://0.0.0.0:8096`。

### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。

## 2026-04-23 WebDashboard 适配进展（九）

### 本轮完成
- 按 WebDashboard 实际 `ApiClient.getUrl(...)` 调用继续补齐一批页面探测接口，覆盖设备、DLNA、媒体库选项、同步、合集、播放列表、LiveTV 辅助配置与忘记密码流程。
- 新增设备详情与设备选项接口：
  - `GET /Devices/Info`
  - `GET/POST /Devices/Options`
  - 设备选项会按设备 ID 写入命名配置，兼容 `devices/device.js` 保存自定义设备名。
- 新增 DLNA 配置接口：
  - `GET /Dlna/ProfileInfos`
  - `GET/POST /Dlna/Profiles`
  - `GET/POST/DELETE /Dlna/Profiles/{id}`
  - 自定义 profile 会写入命名配置，避免 DLNA profile 页面直接 404。
- 新增媒体库与列表类接口：
  - `GET /Libraries/AvailableOptions`
  - `GET /Items/Filters2`
  - `GET /Movies/Recommendations`
  - `GET/POST /Collections`
  - `POST /Collections/{id}/Items`
  - `POST /Collections/{id}/Items/Delete`
  - `GET/POST /Playlists`
  - `POST /Playlists/{id}/Items`
- 新增同步管理接口：
  - `GET /Sync/Options`
  - `GET/POST /Sync/Jobs`
  - `GET/POST/DELETE /Sync/Jobs/{id}`
  - `GET /Sync/JobItems`
  - `DELETE /Sync/JobItems/{id}`
  - `POST /Sync/JobItems/{id}/Enable`
  - `POST /LiveStreams/Open`
- 补齐 LiveTV 设置页辅助接口：
  - `GET /LiveTv/Tuners`
  - `GET /LiveTv/Tuners/Discvover` 与 `GET /LiveTv/Tuners/Discover`
  - `GET /LiveTv/TunerHosts`
  - `GET /LiveTv/TunerHosts/Types`
  - `GET /LiveTv/ChannelMappingOptions`
  - `GET/POST /LiveTv/ChannelMappings`
  - `GET /LiveTv/ListingProviders`
  - `GET /LiveTv/ListingProviders/Default`
  - `GET /LiveTv/ListingProviders/Lineups`
  - `GET /LiveTv/ListingProviders/SchedulesDirect/Countries`
- 新增登录页忘记密码兼容流程：
  - `POST /Users/ForgotPassword`
  - `POST /Users/ForgotPassword/Pin`
  - 当前按本地部署安全默认返回联系管理员，不开放匿名重置密码。
- 新增 `POST /Videos/MergeVersions`，兼容版本合并按钮的提交动作。

### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。
- `cargo test --manifest-path backend/Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过，新增路由没有和现有 Axum 路由冲突。

### 仍待继续
- `Collections` / `Playlists` 当前已兼容前端交互入口，后续还需要落到真实媒体项关系表，支持创建后可在库中长期展示。
- `Sync` 当前完成 WebDashboard 表单/列表接口形状，后续需要接入真实离线同步任务队列。
- 继续按前端实际调用补 `Dlna` profile 全量字段校验、LiveTV 频道映射持久化、播放列表/合集实体化。

## 2026-04-23 WebDashboard 适配进展（十）

### 本轮完成
- 修复 Dashboard 静态路由启动失败：移除精确 `/web` 路由注册，避免与 `ServeDir` 的 `/web` 静态挂载冲突导致 Axum panic。
- 保留 `/` 到 `/web/index.html`、`/web/index.html` 到 `/web/` 的跳转，`/web` 由静态服务接管。
- 新增统一日志过滤函数，默认和 `RUST_LOG` 环境配置都会强制追加以下降噪规则：
  - `sqlx=warn`
  - `sqlx::query=warn`
  - `sqlx::postgres::notice=warn`
  - `sqlx::migrate=warn`
- 将“已应用迁移文件被修改，继续执行兼容性补齐 SQL”的启动提示从 `WARN` 降为 `DEBUG`，避免每次启动刷屏。
- 顺手移除本轮相关路由文件中几个 unused import，减少新引入的编译输出噪声。

### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。
- `cargo test --manifest-path backend/Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过，确认 `/web` 路由冲突已消除。

### 仍待继续
- 项目仍有一批历史 Rust warning，本轮只清理了与最近新增接口相关的明显 unused import；后续可单独做一次 warning cleanup。

## 2026-04-23 WebDashboard 适配进展（十一）

### 本轮完成
- 将 `Collections` / `Playlists` 从临时兼容响应推进为真实持久化功能。
- 新增数据库迁移 `0021_collection_items.sql`，创建 `collection_items` 关系表保存合集/播放列表成员。
- 启动兼容补齐 SQL 同步增加 `collection_items` 建表和索引，兼容已部署实例自动补表。
- 新增 repository 能力：
  - 创建虚拟合集/播放列表媒体项，写入 `media_items`。
  - 自动创建/复用 `Collections` 虚拟媒体库，路径为 `virtual://collections`。
  - 持久化添加/移除成员关系。
  - 查询合集/播放列表列表与子项，并复用 `media_item_to_dto` 返回标准 `BaseItemDto`。
- 扩展接口：
  - `GET /Collections` 返回持久化 `BoxSet` 列表。
  - `POST /Collections?Name=...&Ids=...` 创建合集并保存成员。
  - `GET /Collections/{id}/Items` 返回合集成员。
  - `POST /Collections/{id}/Items?Ids=...` 添加成员。
  - `POST /Collections/{id}/Items/Delete?Ids=...` 移除成员。
  - `GET /Playlists` 返回持久化 `Playlist` 列表。
  - `POST /Playlists?Name=...&Ids=...` 创建播放列表并保存成员。
  - `GET /Playlists/{id}/Items` 返回播放列表成员。
  - `POST /Playlists/{id}/Items?Ids=...` 添加成员。

### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。
- `cargo test --manifest-path backend/Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过。

### 仍待继续
- 播放列表/合集还需要继续补排序、重命名、删除、图片刷新等更完整的 Emby 管理接口。
- `Sync` 仍是下一块需要从接口形状推进到真实任务队列的功能。

## 2026-04-23 WebDashboard 适配进展（十二）
### 本轮完成
- 继续把 `Collections` / `Playlists` 从基础成员关系推进到可管理实体：
  - `GET /Collections/{id}` / `GET /Playlists/{id}` 返回单个合集或播放列表 `BaseItemDto`。
  - `POST /Collections/{id}` / `POST /Playlists/{id}` 支持按 Emby WebDashboard 提交体重命名。
  - `DELETE /Collections/{id}` / `DELETE /Playlists/{id}` 和 `POST /Collections/{id}/Delete` / `POST /Playlists/{id}/Delete` 支持删除虚拟合集/播放列表实体。
  - `DELETE /Collections/{id}/Items?Ids=...` 和 `DELETE/POST /Playlists/{id}/Items/Delete?Ids=...` 支持成员移除。
- 新增 repository 能力：
  - `rename_collection_item` 更新 `media_items.name`、`sort_name`、`date_modified`。
  - `delete_collection_item` 仅允许删除 `BoxSet` / `Playlist`，并同步清理 `collection_items` 关系，避免误删真实媒体。
- 将 `Sync` 从纯空响应推进到可持久化的轻量任务队列：
  - `GET /Sync/Jobs` 支持按 `UserId`、`TargetId` 过滤。
  - `POST /Sync/Jobs` 保存任务到命名配置 `sync_jobs`，保留前端提交字段并规范化 `Id`、`Name`、`TargetId`、`UserId`、`RequestedItemIds`、`Status`、`Profile`、`Quality`。
  - `GET/POST/DELETE /Sync/Jobs/{id}` 支持读取、更新和删除任务。
  - `GET /Sync/JobItems` 根据保存任务返回子任务列表，可按 `JobId` 过滤。
  - `DELETE /Sync/JobItems/{id}`、`POST /Sync/JobItems/{id}/Enable`、`POST /Sync/JobItems/{id}/Transferred` 会更新子任务状态。
  - `GET /Sync/Items/Ready` 返回目标设备可传输子任务。
  - `POST /Sync/Data` 返回当前 `SyncJobs` 与 `SyncJobItems`，兼容离线同步入口刷新。
  - `DELETE /Sync/{targetId}/Items?ItemIds=...` 会按目标设备和媒体项取消对应子任务。
### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。
- `cargo test --manifest-path backend/Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过。
### 仍待继续
- `Sync` 当前是配置持久化任务队列，下一步需要接入真实离线文件准备/转码/传输状态，而不仅是 WebDashboard 管理状态。
- 合集/播放列表还可继续补成员排序、移动、封面刷新、按用户权限过滤等 Emby 细节。

## 2026-04-23 WebDashboard 适配进展（十三）
### 本轮完成
- 按 WebDashboard `editorsidebar.js` 与 Emby ApiClient 的真实调用补齐 `GET /Items/{itemId}/Ancestors`，现在元数据编辑器左侧树会按真实 `parent_id` 回溯父级并返回标准 `BaseItemDto`，不再因缺少祖先接口导致编辑页初始化失败。
- 按 EmbySDK `MetadataEditorInfo` 模型补齐 `GET /Items/{itemId}/MetadataEditor`，返回 `ParentalRatingOptions`、`Countries`、`Cultures`、`ExternalIdInfos`、`PersonExternalIdInfos` 与内容类型选项，支撑 WebDashboard 元数据编辑页下拉框和外部 ID 区域渲染。
- 新增 `GET /Items/{itemId}/ExternalIdInfos`，按媒体类型返回 TMDb、IMDb、TVDB、MusicBrainz 等 Emby 风格 `ExternalIdInfo` 字段：`Name`、`Key`、`Website`、`UrlFormatString`、`IsSupportedAsIdentifier`。
- 新增 `GET /Items/{itemId}/ThemeMedia`，按 EmbySDK `ThemeMediaResult` 返回 `OwnerId`、`Items`、`TotalRecordCount`，并真实查询当前条目下的 `ThemeSong` / `ThemeVideo` 子项；未配置主题媒体时返回空集合而不是 404。
### 验证
- `cargo check --manifest-path backend\Cargo.toml` 通过。
- `cargo test --manifest-path backend\Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过，新增路由没有 Axum 冲突。
### 仍待继续
- 继续沿 WebDashboard 元数据编辑器补齐图片管理、外部图片搜索、人员编辑、锁定字段、删除条目等管理端点。
- 继续对照 `bower_components/emby-apiclient/apiclient.js` 抽取剩余实际调用，优先修复会造成页面白屏、按钮无响应或保存失败的接口。

## 2026-04-23 WebDashboard 适配进展（十四）
### 本轮完成
- 新增 `DELETE /Items/{itemId}` 与 `POST /Items/{itemId}/Delete`，对齐 WebDashboard `deletehelper.js` / Emby ApiClient 的删除入口；删除时同步清理 `collection_items` 中该条目作为合集/播放列表本体或成员的关系，避免遗留孤儿关系。
- 新增 repository 通用删除能力 `delete_media_item`，当前先做数据库真实删除并依赖外键级联清理相关子表，后续可继续补文件系统删除策略。
- 新增 `GET /Items/{itemId}/CriticReviews`，返回 Emby `QueryResult` 形状的空评论集合，避免详情页或编辑页探测影评时 404。
- 修复图片重排端点兼容：`POST /Items/{itemId}/Images/{type}/{index}/Index?newIndex=...` 现在会校验管理员和条目存在后返回 `204`，不再被通配上传路由误判成空图片上传。
### 验证
- `cargo check --manifest-path backend\Cargo.toml` 通过。
- `cargo test --manifest-path backend\Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过。
### 仍待继续
- 通用删除目前只删除数据库记录，后续需要按 Emby 删除策略补可选文件删除、回收站/删除确认、库刷新事件和 WebSocket 通知。
- 图片管理还可继续补多 Backdrop 排序、按 index 删除/替换、多图元数据持久化等完整 Emby 细节。

## 2026-04-23 WebDashboard 适配进展（十五）
### 本轮完成
- 对照 Emby ApiClient 与音乐/电视分类页面，确认 `Genres` / `MusicGenres` / `GameGenres` 已由现有 `backend/src/routes/genres.rs` 提供，避免在 `items.rs` 重复注册造成 Axum 路由冲突。
- 新增 `GET /Artists/AlbumArtists`，复用当前真实艺术家聚合结果，兼容音乐页 `TabAlbumArtists` 通过 Emby ApiClient 探测专辑艺术家入口。
- 新增 `GET /Games/SystemSummaries`，按 Emby 查询结果形状返回空集合，避免客户端在无游戏库实现时访问游戏系统汇总直接 404。
- 修复一次新增分类路由导致的 `/GameGenres/{genreName}/Items` 与 `/GameGenres/{name}/Items` 冲突，保留已有 `genres.rs` 路由作为唯一实现。
### 验证
- `cargo check --manifest-path backend\Cargo.toml` 通过。
- `cargo test --manifest-path backend\Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过。
### 仍待继续
- `Games/SystemSummaries` 当前是兼容空结果，后续若引入游戏库模型，需要接入真实游戏系统统计。
- `Artists/AlbumArtists` 当前复用艺术家聚合，后续需要区分 AlbumArtist / Artist 角色来源。
## 2026-04-23 WebDashboard 适配进展（十六）
### 本轮完成
- 修复 dashboard 首页黑屏的两个直接根因：
  - [frontend/dashboard-ui/index.html](/C:/Users/11797/Desktop/movie-rust/frontend/dashboard-ui/index.html:1) 补回 `scripts/apploader.js` 启动脚本，避免 `/web/` 虽能访问但页面完全不执行 Emby 前端初始化。
  - [backend/src/routes/dashboard.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/routes/dashboard.rs:1) 将 `/web` 静态托管改为 `ServeDir + index.html fallback`，让 `/web/emby` 这类 dashboard 前端路由回落到入口页，而不是 404 后黑屏。
- 把数据库演进从“启动时兼容补丁 SQL”收敛回正式迁移：
  - [backend/src/main.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/main.rs:1) 移除 `ensure_schema_compatibility(...)` 启动补字段逻辑，启动阶段只执行 `sqlx` 正式迁移。
  - 现有 Emby/WebDashboard 所需字段已分别落在正式迁移里：`0011_media_stream_emby_fields.sql`、`0013_media_chapters.sql`、`0015_user_image_fields.sql`、`0016_series_episode_catalog.sql`、`0017_media_items_critic_rating.sql`、`0021_collection_items.sql`。
  - 将重复版本号的 `0012_emby_images_and_trailers.sql` 重编号为 [backend/migrations/0022_emby_images_and_trailers.sql](/C:/Users/11797/Desktop/movie-rust/backend/migrations/0022_emby_images_and_trailers.sql:1)，避免继续污染 `sqlx` 迁移版本序列。
### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。
- `cargo test --manifest-path backend/Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过。
### 仍待继续
- 这次已去掉运行时补字段机制，但如果某个已部署数据库历史上写入过错误的 `sqlx` checksum，仍可能需要一次性手工修复 `_sqlx_migrations` 记录后再启动；当前仓库层面的迁移编号冲突已经修正。
- HTTP request trace 目前仍按 `INFO` 输出访问日志；如果后续还嫌噪声大，可以再单独把 `tower_http` 请求跟踪收敛到更低级别。
## 2026-04-23 WebDashboard 适配进展（十七）
### 本轮完成
- 将 [backend/src/bin/fix_migration.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/bin/fix_migration.rs:1) 从写死旧版本号的临时脚本升级为通用迁移修复工具：
  - 自动读取本地 `./migrations` 并计算当前 checksum。
  - 自动清理 `_sqlx_migrations` 中 `success = false` 的 dirty 记录。
  - 自动修正数据库里“已应用但 checksum 与本地迁移不一致”的版本记录，覆盖当前这类 `migration 12 was previously applied but has been modified` 的历史库场景。
  - 保留 `update_updated_at_column` 函数补齐逻辑，避免旧库缺触发器时继续卡迁移。
  - 支持通过环境变量 `MIGRATION_FIX_DRY_RUN=1` 先做只读预演。
### 验证
- `cargo check --manifest-path backend/Cargo.toml --bin fix_migration` 通过。
- `cargo check --manifest-path backend/Cargo.toml` 通过。
### 仍待继续
- 当前已具备仓库级修复工具，但尚未在你提供的真实数据库上实跑，所以真实库里若还存在“本地已不存在的迁移版本”记录，仍需要下一步结合实际 `_sqlx_migrations` 内容继续清理。
## 2026-04-23 WebDashboard 适配进展（十八）
### 本轮完成
- 继续围绕 WebDashboard 插件页、插件目录页、计划任务页补全后端接口，避免前端再拿到纯空壳响应：
  - [backend/src/routes/integrations.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/routes/integrations.rs:1) 为 `GET /Plugins`、`DELETE /Plugins/{id}`、`GET /Packages`、`GET /Packages/{name}`、`GET /Packages/Updates`、`POST /Packages/Installed/{name}` 补了可直接被 Emby WebDashboard 消费的 Emby 风格数据结构。
  - 插件目录现在提供一组内置可安装包，包含 `name`、`guid`、`category`、`targetSystem`、`type`、`versions`、`shortDescription`、`overview`、`owner` 等字段，兼容 `plugincatalogpage.js` 与 `addpluginpage.js` 的读取方式。
  - 已安装插件列表与安装/卸载动作改为持久化到数据库命名配置 `installed_plugins`，刷新页面后不会重新变回空列表。
- [backend/src/routes/dashboard.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/routes/dashboard.rs:1) 补齐了 `GET /web/ConfigurationPages` 与 `GET /web/ConfigurationPage?Name=...`：
  - 会根据已安装插件动态生成 `PluginConfiguration` 页面描述。
  - 配置页内容不再返回 404 文本，而是返回可渲染的 HTML 片段，避免插件设置入口直接失败。
- [backend/src/routes/management.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/routes/management.rs:1) 将计划任务从静态空壳推进为可读可写的轻量实体：
  - 新增 `GET /ScheduledTasks/{id}`，兼容 `scheduledtaskpage.js` 读取单个任务详情与触发器。
  - `GET /ScheduledTasks` 现在返回带 `Triggers`、`LastExecutionResult`、`IsEnabled`、`CurrentProgressPercentage` 的任务对象。
  - `POST/DELETE /ScheduledTasks/Running/{id}` 会把任务运行状态持久化到 `scheduled_task_states`。
  - `POST /ScheduledTasks/{id}/Triggers` 改为持久化整组触发器到 `scheduled_task_triggers`，前端新增/删除触发器后刷新仍能看到结果。
### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。
- `cargo test --manifest-path backend/Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过。
### 仍待继续
- 当前插件目录与插件配置页已经能驱动 WebDashboard 页面，但仍属于本地兼容目录，后续还可以继续接入更真实的插件源、版本升级策略与插件专属配置表单。
- 计划任务目前是轻量持久化状态机，下一步仍需继续对接真实库扫描、元数据刷新、任务进度推送和 WebSocket 广播，进一步贴近 Emby 原生行为。
## 2026-04-23 WebDashboard 适配进展（十九）
### 本轮完成
- 使用浏览器实测 `https://test.emby.yun:4443/web/`，定位到首页不是 JS 崩溃，而是 Emby 前端在连接阶段主动拒绝当前服务端版本：
  - 控制台明确出现 `minServerVersion requirement not met. Server version: 0.1.0`。
  - 根因是 [backend/src/routes/system.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/routes/system.rs:1) 的 `GET /System/Info/Public` 与 `GET /System/Info` 直接返回了 Cargo 包版本 `0.1.0`，低于 Emby 前端 `connectionmanager.js` 内置的最小服务端版本 `3.2.33`。
- 已修复系统信息版本兼容：
  - `System/Info/Public` 与 `System/Info` 现在统一通过 `emby_compatible_server_version()` 返回 Emby 兼容版本号。
  - 默认兼容版本设为 `4.8.10.0`，并支持通过环境变量 `EMBY_COMPAT_VERSION` 覆盖，方便后续按客户端行为继续微调。
- 同时清理了匿名首页阶段的样式噪声：
  - [backend/src/routes/system.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/routes/system.rs:1) 的 `GET /Branding/Css.css` 不再要求登录态，避免前端启动期额外报 `401`。
### 验证
- 使用 MCP 浏览器实测线上页面，确认当前线上根因是版本门槛拦截与匿名 Branding CSS `401`。
- `cargo check --manifest-path backend/Cargo.toml` 通过。
### 仍待继续
- 本轮修复已经落到仓库代码，但远端 `https://test.emby.yun:4443/` 仍需重新部署新后端后，浏览器中的 `ServerUpdateNeeded` 弹窗才会真正消失。
- 远端重新部署后，还需要继续用浏览器复测登录流程、首页路由和启动后的管理页接口，收下一轮真实前端报错。
## 2026-04-23 WebDashboard 适配进展（二十）
### 本轮完成
- 修复命名配置链路里“前端能保存、后端又读不到”的关键别名问题：
  - [backend/src/routes/system.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/routes/system.rs:1) 为 `/System/Configuration/{name}` 增加命名配置 key 归一化逻辑，当前先把前端常用的 `branding` 统一映射到真正用于品牌接口的 `branding_configuration`。
  - 这样 `dashboardgeneral.js` 通过 `ApiClient.getNamedConfiguration("branding")` / `updateNamedConfiguration("branding", ...)` 保存的 `LoginDisclaimer`、`CustomCss`，现在会被 `/Branding/Configuration` 与 `/Branding/Css.css` 真实读取到。
- 补齐多组 WebDashboard 设置页依赖的默认命名配置结构，避免初始返回 `{}` 导致前端访问 `config.Options`、`config.EnablePlayTo`、`config.ReleaseDateFormat` 等字段时为空或保存后字段丢失：
  - `branding_configuration`
  - `dlna`
  - `sync`
  - `notifications`
  - `fanart`
  - `metadata`
  - `xbmcmetadata`
- [backend/src/repository.rs](/C:/Users/11797/Desktop/movie-rust/backend/src/repository.rs:1) 的品牌配置读取也做了兼容：
  - 优先读取 `branding_configuration`
  - 同时向后兼容历史上可能已经写入的 `branding`
  - 读取时会合并缺失默认字段，避免升级后旧值缺字段
### 验证
- `cargo check --manifest-path backend/Cargo.toml` 通过。
- `cargo test --manifest-path backend/Cargo.toml api_router_builds_without_route_conflicts -- --nocapture` 通过。
### 仍待继续
- 这轮先解决了设置页“结构为空”和品牌配置读写分裂，后续仍要继续沿着 WebDashboard 实际页面，把 `DLNA Profiles`、`Notifications` 单项配置页、`App Services`、`Dashboard` 首页的剩余真实操作接口继续补全到 Emby 形状。
