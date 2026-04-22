# Emby API 兼容性审计报告

**项目**: Movie Rust  
**更新时间**: 2026-04-22  
**对照来源**:
- `模板项目/本地播放器模板/packages/lin_player_server_api/lib/services/emby_api.dart`
- `模板项目/EmbySDK/Documentation/Download/openapi_v2_noversion.json`
- `模板项目/Emby模板`
- 当前 Rust 后端路由、DTO、Repository 实现

## 本次增量修复记录

### 2026-04-22

- 对照本地播放器模板的 `UserData` 调用链，修复 `POST /Users/{userId}/Items/{itemId}/UserData` 和 `/UserItems/{itemId}/UserData` 的已播放语义。
- 当客户端提交 `Played=true` 但没有提供 `LastPlayedDate` 时，后端现在会写入真实 `now()`，保证继续观看、最近播放、已播放排序有真实时间依据。
- 当 `Played=true` 时，`PlaybackPositionTicks` 会归零，避免已播放影片仍出现在继续观看列表。
- 复查 SQL 更新语义后，将归零条件限定为“本次请求明确提交 `Played=true`”，避免历史已播放状态影响后续普通进度更新。
- 当 `Played=false` 时，`LastPlayedDate` 会清空，行为更接近 Emby 的取消已播放。
- 清理 `backend/src/routes/items.rs` 中已经废弃的 `body::to_bytes` import。
- 对照 EmbySDK `BaseItemDto`，补充条目详情的真实长尾字段来源：`PreferredMetadataLanguage`、`PreferredMetadataCountryCode` 从持久化启动配置读取；`CanMakePublic`、`CanManageAccess`、`CanLeaveContent` 按当前项目真实能力返回 `false`。
- 补齐音乐/专辑相关响应字段：当媒体项父级是真实 `Album/MusicAlbum` 时返回 `Album`、`AlbumId`、`AlbumPrimaryImageTag`；`Artists`、`ArtistItems`、`AlbumArtist`、`AlbumArtists`、`Composers` 从真实 `person_roles` 关系生成。
- 未给 `SyncStatus` 写默认值，因为项目尚未实现真实同步任务模型；没有真实同步状态时继续不返回该字段，避免伪造数据。
- 对照本地播放器模板 `fetchNextUp` 和 Emby 的 `Shows/NextUp` 语义，修复 NextUp 查询：现在按真实剧集归属分组，每部剧只返回当前下一集，而不是把同一部剧所有未播放分集都返回给播放器。
- `Shows/NextUp` 的 `SeriesId`/`ParentId` 作用域继续支持剧、季、媒体库三种场景，返回项仍由真实 `media_items`、`user_item_data` 和 DTO 组装。

## 本地播放器实际依赖主链路

本地播放器使用 Emby SDK 风格调用，重点依赖以下端点：

- 认证：`Users/AuthenticateByName`、`Users/Me`
- 服务器信息：`System/Info/Public`、`System/Info`、`System/Ext/ServerDomains`
- 媒体库：首页和媒体库列表 `Users/{userId}/Views`、`Items/Counts`、`Users/{userId}/Items/Counts`
- 列表和筛选：`Users/{userId}/Items`、`Items/Filters`、`Users/{userId}/Items/Filters`、`Genres`、`Users/{userId}/Genres`
- 电视剧：`Shows/{seriesId}/Seasons`、`Shows/{seriesId}/Episodes`、`Seasons/{seasonId}/Episodes`、`Shows/NextUp`
- 详情：`Users/{userId}/Items/{itemId}`
- 播放：`Items/{itemId}/PlaybackInfo` 的 GET 和 POST
- 播放上报：`Sessions/Playing`、`Sessions/Playing/Progress`、`Sessions/Playing/Stopped`
- 用户数据：`Users/{userId}/Items/{itemId}/UserData`、`FavoriteItems`、`HideFromResume`
- 图片：`Items/{itemId}/Images/{type}`、用户头像、远程图片下载
- 章节和片头：`Items/{itemId}/Chapters`、`Episodes|Items|Videos/{id}/IntroTimestamps`
- 相似内容：`Users/{userId}/Items/{itemId}/Similar`

## 当前已完成的兼容修补

- `Users/{userId}/Items` 已覆盖大量播放器和 Emby SDK 常用查询参数，包括 `MediaTypes`、`VideoTypes`、`ImageTypes`、`Genres`、`OfficialRatings`、`Tags`、`Years`、`PersonIds`、`Artists`、`ArtistIds`、`Albums`、`Studios`、`Containers`、`AudioCodecs`、`VideoCodecs`、`SubtitleCodecs`、`NameStartsWith`、`IsPlayed`、`IsFavorite`、`IsHD`、`HasSubtitles`、日期范围等。
- `Items/Filters` 和 `Users/{userId}/Items/Filters` 已返回真实聚合筛选值，并补齐 `Studios`、`Years`、`Tags`、`OfficialRatings`、`Containers`、`AudioCodecs`、`VideoCodecs`、`SubtitleCodecs`、`Artists` 等筛选端点。
- 电影和剧集分集已具备多版本分组逻辑，详情和 `PlaybackInfo` 可返回多个 `MediaSources`。
- `PlaybackInfo` 已支持 GET/POST、`DeviceProfile`、`DirectStreamUrl`、`TranscodingUrl`、`TranscodingContainer`、`TranscodingSubProtocol`、默认音轨/字幕索引、`RequiredHttpHeaders`、`TranscodingInfo`。
- 图片读取、上传、删除已覆盖 `Items/{id}/Images/{type}`、带 index 的图片形态、`Url` 形态、用户头像、`Primary/Backdrop/Logo/Thumb`。
- 电视剧主链路已覆盖 `Shows/{seriesId}/Seasons`、`Shows/{seriesId}/Episodes`、`Seasons/{seasonId}/Episodes`、`Shows/NextUp`、`Shows/Missing`、`Shows/Upcoming`。
- Sessions 已覆盖播放上报、播放队列、`NowPlayingItem`、`PlayState`、远控命令记录和消费队列。
- `DisplayMessage`、`SetAudioStreamIndex`、`SetSubtitleStreamIndex`、`SetVolume`、`SetAdditionalUser` 等命令已落入真实 session 摘要状态。
- `System/Logs`、`System/Logs/Query`、`System/Logs/{name}`、`System/Logs/{name}/Lines` 已返回真实日志目录内容。
- `Startup/RemoteAccess`、品牌配置、Localization、DisplayPreferences、UserSettings 已接入持久化配置或环境配置，不再只返回硬编码壳子。
- `BaseItemDto` 已补充部分 EmbySDK 长尾字段：排序号、权限能力、元数据语言/地区、音乐艺术家/专辑字段、3D 视频格式、CustomRating。

## 仍需继续推进的缺口

- `DeviceProfile` 判定还不是完整 Emby 级别，仍需继续细化容器、音频、字幕、码率、HDR、profile、level、字幕 burn-in 等规则。
- `TranscodingInfo` 字段已存在，但真实转码生命周期、进度、硬件加速状态、失败原因还需要接入实际转码任务。
- `RemoteImages` 已能下载指定远程图片，但候选图片列表和 providers 仍需进一步从 TMDB/远程元数据源聚合。
- `DisplayPreferences`、Localization、UserSettings 已可用，但还未完整复刻所有 Emby 客户端布局偏好。
- `Shows/Missing`、`Shows/Upcoming`、`Shows/NextUp` 仍需继续补齐 EmbySDK 的长尾过滤语义。
- `ProjectToMedia` 参数已接收，但返回投影语义还需要继续核对。
- `BaseItemDto` 长尾字段仍需逐步补齐，例如 `ChannelId`、`ChannelName`、`CurrentProgram`、`ExtraType`、`Subviews`、真实 `SyncStatus`。
- Sessions 远控已经持久化命令队列，但还不是 Emby 原生 WebSocket 实时推送模型。
- Auth Keys 已有兼容路径，但 API Key 权限、过期策略、审计策略还需继续细化。
- 音乐、直播、频道、BoxSet、Artist、MusicAlbum 等非电影电视剧域仍是部分兼容。

## 下一步建议

1. 继续审计本地播放器模板中 `PlaybackInfo` 后续处理逻辑，确认 `MediaSource` 字段是否还缺会导致播放器空指针的字段。
2. 对照 EmbySDK 的 `BaseItemDto`，优先补本地播放器详情页实际读取的长尾字段。
3. 继续把 RemoteImages providers 和候选图列表接到真实 TMDB 数据源。
4. 补充针对 `UserData`、`HideFromResume`、`FavoriteItems`、`PlaybackInfo` 的路由级回归测试。
