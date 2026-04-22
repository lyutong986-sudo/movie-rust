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
