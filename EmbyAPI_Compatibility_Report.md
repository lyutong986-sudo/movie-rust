# Emby API 兼容性审计报告

**项目**: Movie Rust  
**更新时间**: 2026-04-22  
**审计目标**: 对照本地播放器模板、Emby 模板和 EmbySDK/OpenAPI，持续确认后端端点、请求参数、响应字段和真实数据来源是否足够兼容 Emby 客户端。  
**本轮结论**: 电影、电视剧、播放、筛选、图片、用户数据主链路继续保持可用。本轮重点修补了 `PlaybackInfo` 对 EmbySDK `DeviceProfile` 对象数组的兼容、`RemoteImages` 语言过滤、`Shows/NextUp`/`Shows/Missing`/`Shows/Upcoming` 的长尾查询过滤和分页语义。仍未宣称完整 Emby：真实转码生命周期、完整 WebSocket 推送、非影视域模型、直播/频道/同步等仍需要独立数据模型和功能链路。

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

状态: 已覆盖核心。`Shows/NextUp` 已按剧集归属分组返回每部剧下一集。`Shows/Missing` 使用 `series_episode_catalog` 的真实缺集目录。`Shows/Upcoming` 使用实际 `premiere_date`。本轮已补充三个列表端点的 SDK 查询过滤和分页语义，包括媒体类型、图片类型、类型筛选、年份、评分、用户播放状态、收藏、HD、字幕、日期范围和搜索词。

仍需补强: 更完整的 `Fields` 字段裁剪、`EnableImages`/`ImageTypeLimit` 精细图片裁剪、`EnableUserData=false` 时的响应裁剪。

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

本轮修复: `DeviceProfile.ContainerProfiles`、`CodecProfiles`、`ResponseProfiles`、`SubtitleProfiles` 已改为保留 EmbySDK 对象数组，避免客户端 POST `/PlaybackInfo` 时请求体反序列化失败并忽略设备能力配置。已增加单元测试。

仍需补强: 完整 `ContainerProfiles`/`CodecProfiles` 条件判定、HDR/Dolby Vision、profile/level、字幕 burn-in 策略、真实转码任务生命周期、实时进度、硬件加速状态、失败原因和关闭清理。

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

- 修复 `DeviceProfile.ContainerProfiles`、`CodecProfiles`、`ResponseProfiles`、`SubtitleProfiles` 的对象数组反序列化，避免 EmbySDK 客户端 POST `/PlaybackInfo` 被忽略。
- 为 `PlaybackInfo` 增加 `playback_info_accepts_emby_sdk_profile_object_arrays` 单元测试。
- 为 `RemoteImages` 增加 `Language`/`language` 参数过滤，支持语言前缀匹配。
- 为 `Shows/NextUp`、`Shows/Missing`、`Shows/Upcoming` 补充路由层 SDK 查询过滤、排序和分页，避免这些端点只返回核心查询结果而忽略客户端筛选条件。
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
| 电视剧 | 已覆盖核心 | 中高 | Season/Episode/NextUp/Missing/Upcoming 可用；本轮补充列表过滤分页 |
| PlaybackInfo | 已覆盖核心 | 中 | DirectStream/Transcoding 基础可用；本轮修复 EmbySDK profile 对象数组解析 |
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

- `TranscodingInfo` 仍缺少真实转码任务生命周期、实时进度、硬件加速状态、失败原因、转码会话关闭清理等完整链路。
- HLS playlist 当前是兼容入口，仍需要和实际转码器输出、segment 缓存、Range/seek 行为做更深整合。

### P1: 影响筛选、剧集和详情完整度的缺口

- `DeviceProfile` 已能完整接收 EmbySDK 对象数组，但 `ContainerProfiles`、`CodecProfiles`、`ResponseProfiles` 的条件判定仍需继续深化，包括 HDR/DV、profile、level、bit depth、字幕格式与 burn-in 策略。
- `ProjectToMedia` 已处理虚拟目录排除，但还未做 Emby 全量字段裁剪投影。
- `Shows/Missing`、`Shows/Upcoming`、`Shows/NextUp` 已补常见过滤分页，但 `Fields`、`EnableImages`、`ImageTypeLimit`、`EnableUserData=false` 的精细裁剪仍待补。
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
2. 深化 `DeviceProfile` 条件判定: `ContainerProfiles`、`CodecProfiles`、`ResponseProfiles`、HDR/DV、profile/level、字幕格式与 burn-in。
3. 继续复核 `Users/{userId}/Items` 的 `ProjectToMedia` 字段投影和 BoxSet/Collection 行为。
4. 继续补 `Shows/Missing/Upcoming/NextUp` 的 `Fields`、图片裁剪、用户数据裁剪。
5. 扩展 `RemoteImages` 更多 provider 和排序策略。
6. 为本地播放器强依赖端点补路由级集成测试: 认证、Views、Items、Filters、Seasons/Episodes、PlaybackInfo、Videos stream、UserData、HideFromResume、FavoriteItems、Images、IntroTimestamps。
7. 推进完整 Emby 管理后台: Library options、任务队列、扫描状态、远程图片管理、DisplayPreferences 模板。
8. 最后展开非影视域: 音乐、直播、频道、录制、BoxSet。

## 验证记录

本轮修复后已通过:

```text
cargo check --manifest-path backend/Cargo.toml
cargo test --manifest-path backend/Cargo.toml playback_info_accepts_emby_sdk_profile_object_arrays -- --nocapture
```

当前仍存在一批既有 Rust warning，主要是未使用 import、未使用字段、未使用辅助函数和部分未来扩展模型；它们不阻塞构建，但建议后续在功能稳定后统一清理。
