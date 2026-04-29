# Jellyfin 模板 vs 当前项目 — 功能差异报告

> 排除范围：直播(LiveTV)、插件(Plugins)、DLNA、音乐(Music)、家庭视频/混合内容
> 对比时间：2026-04-29（第六轮功能优化更新）

---

## 一、已有且基本完整的功能 ✅

| 功能 | Jellyfin 模板 | 当前项目 | 状态 |
|------|-------------|---------|------|
| 首页 Hero 轮播 | ✅ homesections | ✅ HeroCarousel | 完整 |
| 继续观看 | ✅ resume row | ✅ MediaRow | 完整 |
| 下一集 (Next Up) 首页区块 | ✅ nextUp row | ✅ nextUpItems MediaRow | 完整 |
| 最近添加 | ✅ latest | ✅ latest + latestByLibrary | 完整 |
| 收藏列表 | ✅ favorites tab | ✅ favorites MediaRow | 完整 |
| 首页区块自定义 | ✅ MyPreferencesHome | ✅ HomeLayoutSettings（拖拽排序+开关） | 完整 |
| 媒体库浏览 | ✅ /list | ✅ /library/:id | 完整 |
| 筛选（类型、年份、类型标签、收藏、4K、HDR、字幕） | ✅ filterdialog | ✅ UPopover 筛选面板 | 完整 |
| 排序（名称、日期、年份、评分、随机、集数） | ✅ sortmenu | ✅ USelect | 完整 |
| 无限滚动加载 | ✅ cardbuilder | ✅ IntersectionObserver | 完整 |
| 视图切换（网格/列表/详细列表） | ✅ viewSettings | ✅ libraryLayout toggle + MediaListItem | 完整 |
| 字母索引跳转 | ✅ alphaPicker | ✅ AlphaPicker + NameStartsWith | 完整 |
| 播放全部/随机播放 | ✅ PlayArrowIconButton | ✅ playAll/shufflePlay | 完整 |
| 多选批量操作 | ✅ multiSelect | ✅ selectedItems + 浮动工具栏 | 完整 |
| 电影详情页 | ✅ /details | ✅ /item/:id | 完整 |
| 剧集详情页（季/集 Tab） | ✅ /details | ✅ /series/:id | 完整 |
| 分集进度条 | ✅ | ✅ episodeProgress 进度条 | 完整 |
| 分集已看标记 | ✅ | ✅ check-circle 图标 | 完整 |
| 下一集高亮 | ✅ | ✅ 边框高亮 + "下一集" badge | 完整 |
| 剧集状态（连载中/已完结） | ✅ | ✅ metaChips 显示 | 完整 |
| 外部链接（TMDB/IMDB/豆瓣/TVDB） | ✅ | ✅ externalLinks 可点击链接 | 完整 |
| **制片工作室详情页** | ✅ Studios 实体页 | ✅ **/studio/:name StudioPage**（工作室信息+作品列表） | **完整** |
| **演员详情页** | ✅ | ✅ **/person/:id PersonPage**（演员信息+参演作品） | **完整** |
| **花絮/特别内容** | ✅ renderSpecials | ✅ **specialFeatures 区块**（SpecialFeatureCount + getSpecialFeatures） | **完整** |
| 首映日期显示 | ✅ | ✅ metaChips 含 PremiereDate | 完整 |
| 相似推荐 | ✅ similar | ✅ similar | 完整 |
| 章节标记（详情页+播放器） | ✅ chapters | ✅ chapters | 完整 |
| 预告片嵌入 | ✅ trailer | ✅ trailerEmbed | 完整 |
| 多媒体源切换 | ✅ MediaSources tab | ✅ sourceTabs | 完整 |
| 视频播放器（HLS + 直链） | ✅ htmlVideoPlayer | ✅ video.js + hls.js | 完整 |
| 播放进度上报 | ✅ playback* | ✅ reportProgress/stopPlayback | 完整 |
| 跳过片头/片尾 (MediaSegments) | ✅ skipIntro | ✅ activeSkipSegment + skipSegment | 完整 |
| Trickplay 缩略图 | ✅ trickplay | ✅ trickplayInfo + trickplayThumbUrl | 完整 |
| 下一集自动播放 | ✅ upnext | ✅ nextUpEpisode + UpNextDialog 弹窗 | 完整 |
| Up Next 倒计时弹窗 | ✅ upnextdialog | ✅ UpNextDialog 组件 | 完整 |
| 字幕搜索 & 下载 | ✅ subtitleeditor | ✅ searchSubtitles/downloadSubtitle | 完整 |
| 字幕同步调整 | ✅ subtitlesync | ✅ subtitleOffset ±0.5s UI | 完整 |
| 图像管理（列表/远程/上传/删除） | ✅ imageeditor | ✅ 详情页图像管理区 | 完整 |
| 刷新元数据 | ✅ refreshdialog | ✅ refreshItemMetadata | 完整 |
| 元数据编辑器 | ✅ metadataEditor | ✅ MetadataEditorDialog | 完整 |
| 条目识别 (Identify) | ✅ itemidentifier | ✅ IdentifyDialog | 完整 |
| 媒体信息弹窗 | ✅ itemMediaInfo | ✅ MediaInfoDialog | 完整 |
| 合集编辑器 | ✅ collectionEditor | ✅ CollectionEditorDialog | 完整 |
| 媒体文件下载 | ✅ useGetDownload | ✅ downloadFile（更多菜单） | 完整 |
| 播放列表 CRUD | ✅ playlisteditor | ✅ /playlists + /playlist/:id | 完整 |
| 播放队列 | ✅ /queue | ✅ /queue | 完整 |
| 稍后观看 | ❌ (Jellyfin 无此概念) | ✅ watchLater (本地) | 当前项目额外功能 |
| 收藏/已播放 toggle | ✅ userdatabuttons | ✅ toggleFavorite/togglePlayed | 完整 |
| 未播放计数角标 | ✅ | ✅ UnplayedItemCount badge | 完整 |
| 右键上下文菜单 | ✅ ItemMenu | ✅ ContextMenu + MediaCard | 完整 |
| 搜索（全局 + 快速面板） | ✅ /search | ✅ /search + CommandPalette | 完整 |
| NowPlayingBar 底部播放条 | ✅ nowPlayingBar | ✅ MiniPlayer（进度条/控制/封面/音量） | 完整 |
| 播放速率选择 | ✅ | ✅ rateMenu dropdown | 完整 |
| 画中画 | ✅ | ✅ togglePip | 完整 |
| **可配置快进快退步长** | ✅ skipBackLength/skipForwardLength | ✅ **skipSteps store + AccountSettings 配置** | **完整** |
| **服务器重启/关机** | ✅ restartServer/shutdownServer | ✅ **DashboardHome 重启/关机按钮 + 确认弹窗** | **完整** |
| 向导 | ✅ /wizard/* | ✅ /wizard | 完整 |
| 登录/多服务器 | ✅ login/selectserver | ✅ login/select/add | 完整 |
| 忘记密码 | ✅ forgotpassword | ✅ /server/forgot-password | 完整 |
| 键盘快捷键 | ✅ 部分 | ✅ ShortcutsDialog | 完整 |
| 设置：账户/密码 | ✅ userprofile | ✅ /settings/account（含音频/字幕/播放/显示偏好） | 完整 |
| 设置：服务器 | ✅ /dashboard/settings | ✅ /settings/server | 完整 |
| 设置：媒体库管理 | ✅ /dashboard/libraries | ✅ /settings/libraries | 完整 |
| 设置：用户管理 | ✅ /dashboard/users | ✅ /settings/users | 完整 |
| 设置：单个用户详情 | ✅ /dashboard/users/:id | ✅ /settings/users/:userId（个人资料/权限/活动Tab） | 完整 |
| 设置：Dashboard 管理面板 | ✅ /dashboard | ✅ /settings/dashboard（服务器/会话/库/任务/活动+重启/关机） | 完整 |
| 设置：转码 | ✅ /dashboard/transcoding | ✅ /settings/transcoding | 完整 |
| 设置：网络 | ✅ /dashboard/networking | ✅ /settings/network | 完整 |
| 设置：设备/会话 | ✅ /dashboard/devices | ✅ /settings/devices | 完整 |
| 设置：API Key | ✅ /dashboard/keys | ✅ /settings/apikeys | 完整 |
| 设置：计划任务（含触发器编辑） | ✅ /dashboard/tasks | ✅ /settings/scheduled-tasks（含详情+CRUD触发器） | 完整 |
| 设置：日志（含日志文件查看器） | ✅ /dashboard/logs | ✅ /settings/logs-and-activity（含日志内容弹窗+下载） | 完整 |
| 设置：品牌化 | ✅ /dashboard/branding | ✅ /settings/branding | 完整 |
| 设置：播放 | ✅ /dashboard/playback | ✅ /settings/playback | 完整 |
| 设置：字幕样式 | ✅ 部分 | ✅ /settings/subtitles（含实时预览） | 完整 |
| 设置：字幕下载 | ✅ 部分 | ✅ /settings/subtitle-download | 完整 |
| 远端 Emby 中转 | ❌ | ✅ /settings/remote-emby | 当前项目独有 |

---

## 二、本地播放器兼容性验证 ✅

> 对照 `本地播放器模板/packages/lin_player_server_api/lib/services/emby_api.dart`

| 播放器调用 | 后端端点 | 状态 |
|-----------|---------|------|
| `fetchItemCounts()` | `GET /Items/Counts` + `GET /Users/{id}/Items/Counts` | ✅ 已存在 |
| `fetchAvailableFilters()` | `GET /Items/Filters` + `GET /Users/{id}/Items/Filters` | ✅ 已存在 |
| `updatePlaybackPosition()` | `POST /Users/{id}/Items/{id}/UserData` + `POST /UserItems/{id}/UserData` | ✅ 已存在 |
| `hideFromResume()` | `POST /Users/{id}/Items/{id}/HideFromResume` | ✅ 已存在 |
| `fetchChapters()` | `GET /Items/{id}/Chapters` | ✅ 已存在 |
| `fetchIntroTimestamps()` | `GET /Episodes/{id}/IntroTimestamps` + `GET /Items/{id}/IntroTimestamps` + `GET /Videos/{id}/IntroTimestamps` | ✅ 三路径均已注册 |
| `fetchDomains()` | `GET /System/Ext/ServerDomains` | ✅ 已存在 |
| `authenticateByName()` | `POST /Users/AuthenticateByName` | ✅ 已存在 |
| `fetchUserInfo()` | `GET /Users/Me` | ✅ 已存在 |
| `fetchServerInfo()` | `GET /System/Info/Public` + `GET /System/Info` | ✅ 已存在 |
| `fetchUserViews()` | `GET /Users/{id}/Views` | ✅ 已存在 |
| `fetchItems()` | `GET /Users/{id}/Items` (全参数) | ✅ 已存在 |
| `fetchItemDetails()` | `GET /Users/{id}/Items/{id}` | ✅ 已存在 |
| `fetchSimilarItems()` | `GET /Users/{id}/Items/{id}/Similar` | ✅ 已存在 |
| `fetchNextUp()` | `GET /Shows/NextUp` | ✅ 已存在 |
| `fetchPlaybackInfo()` | `POST /Items/{id}/PlaybackInfo` + `GET /Items/{id}/PlaybackInfo` | ✅ 已存在 |
| 播放上报 | `POST /Sessions/Playing` + `/Progress` + `/Stopped` | ✅ 已存在 |
| 收藏 | `POST/DELETE /Users/{id}/FavoriteItems/{id}` | ✅ 已存在 |
| 图片 | `GET /Items/{id}/Images/{type}` | ✅ 已存在 |
| 剧季 | `GET /Shows/{id}/Seasons` | ✅ 已存在 |

---

## 三、后端修复记录

| 修复 | 说明 | 状态 |
|------|------|------|
| ActivityLog userId 筛选 | `GET /System/ActivityLog/Entries?UserId=` 支持按用户筛选活动日志 | ✅ 已修复 |
| ScheduledTasks PUT 路由 | `PUT /ScheduledTasks/{id}/Triggers` 与 POST 并行注册，兼容 Emby 客户端 | ✅ 已修复 |
| 日志/活动端点 admin 权限 | `server_logs`、`server_log_content`、`server_log_lines`、`activity_log_entries` 添加 `require_admin` | ✅ 已修复 |
| `thumb_image_tag` 字段 | BaseItemDto 添加 ThumbImageTag，播放器缩略图兼容 | ✅ 已修复 |
| `MediaSourceId` serde 重命名 | PlaybackReport/LegacyPlaybackQuery 的 MediaSourceId 正确反序列化 | ✅ 已修复 |
| PlaybackReport 扩展字段 | 添加 `can_seek`、`event_name` 兼容播放器上报 | ✅ 已修复 |

---

## 四、仍缺失或可继续优化的功能 ❌/⚠️

### A. 可选特性（低优先级）

| 优先级 | 缺失功能 | Jellyfin 对应 | 说明 |
|--------|---------|-------------|------|
| 🟢 低 | **Quick Connect** | `/quickconnect` | 用手机/设备码快速登录（Emby/Jellyfin 特性） |
| 🟢 低 | **远程控制** | `remotecontrol/` | 从浏览器控制另一台设备上的播放 |
| 🟢 低 | **播放器统计/调试面板** | `playerstats/` | 实时显示播放帧率、码率、丢帧等信息 |
| 🟢 低 | **背景/Logo 屏保** | `backdropScreensaver` | 空闲时的背景切换屏保 |
| 🟢 低 | **Trickplay 配置页** | `/dashboard/trickplay` | 管理员配置 trickplay 生成参数 |
| 🟢 低 | **备份管理** | `/dashboard/backups` | 服务器备份创建/恢复 |
| 🟢 低 | **亮度/手势控制** | `touchHelper` | 移动端滑动调节音量/亮度 |
| 🟢 低 | **WebSocket 实时推送** | `serverNotifications.js` | UserDataChanged 等事件推送免手动刷新 |
| 🟢 低 | **用户头像上传** | `userprofile.tsx` | 头像上传/删除 |
| 🟢 低 | **每库视图偏好** | `getSettingsKey + viewSettings` | 按媒体库/文件夹保存视图+排序+筛选偏好 |

### B. UI 细节优化（可选）

| 细节 | 当前状态 | 改进空间 |
|------|---------|---------|
| 加载骨架屏 | 使用 UProgress carousel | 可替换为骨架屏提升感知速度 |
| 库入口可隐藏 | 未实现 MyMediaExcludes | 可让用户隐藏不想看的库 |
| 最近添加可排除某库 | 未实现 LatestItemsExcludes | 可让用户排除某些库的最新内容 |

### C. 后端已知限制

| 限制 | 说明 |
|------|------|
| `update_item` 仅写入白名单字段 | `LockedFields` 被忽略；`taglines`、`custom_rating`、`air_days` 等未持久化 |
| Session `remote_end_point` 始终为 None | 无法显示客户端 IP |
| ActivityLog 仅含 playback_events | 不含登录、配置变更等审计日志 |
| Collections 存储在 settings JSON | 非独立 media_items 行，可能影响库浏览合并 |

---

## 五、已实施的全部功能清单

### 第一批（核心体验提升）— ✅ 全部完成
1. ✅ 元数据编辑器 — MetadataEditorDialog + updateItem API
2. ✅ Up Next 弹窗 — UpNextDialog 倒计时弹窗
3. ✅ 播放全部 / 随机播放 — playAll/shufflePlay store helpers
4. ✅ 视图切换（网格/列表/详细列表） — libraryLayout + MediaListItem

### 第二批（管理功能完善）— ✅ 全部完成
5. ✅ Dashboard 管理面板首页 — DashboardHome 聚合小部件
6. ✅ 条目识别 (Identify) — IdentifyDialog + RemoteSearch API
7. ✅ 字母索引跳转 — AlphaPicker + NameStartsWith 筛选
8. ✅ 多选批量操作 — selectedItems + 浮动批量工具栏
9. ✅ 任务触发器编辑 — 计划任务详情面板 + 触发器 CRUD
10. ✅ 单个用户详情页 — UserDetailPage（个人资料/权限/活动 Tab）

### 第三批（体验细节打磨）— ✅ 全部完成
11. ✅ 首页区块自定义 — HomeLayoutSettings 拖拽排序
12. ✅ 合集编辑器 — CollectionEditorDialog + Collections API
13. ✅ 字幕同步调整 — subtitleOffset ±0.5s UI
14. ✅ NowPlayingBar 增强 — 进度条/控制/封面/音量
15. ✅ 日志文件查看器 — 日志内容弹窗 + 下载
16. ✅ 媒体信息弹窗 — MediaInfoDialog 详细流信息

### 第四批（功能细节审计）— ✅ 全部完成
17. ✅ 外部链接（TMDB/IMDB/豆瓣/TVDB） — 详情页/剧集页可点击链接
18. ✅ 制片工作室显示 — 详情页/剧集页 Studios 区块
19. ✅ 演员卡片可点击 — 点击跳转搜索该演员作品
20. ✅ 分集播放进度条 — 缩略图上显示观看进度
21. ✅ 分集已看/下一集标记 — check-circle + "下一集" badge + 边框高亮
22. ✅ 剧集状态显示 — 连载中/已完结 badge
23. ✅ 首映日期显示 — metaChips 展示 PremiereDate
24. ✅ 未播放计数角标 — MediaCard 显示 UnplayedItemCount
25. ✅ 媒体文件下载 — 更多菜单中的下载按钮
26. ✅ 账户设置重组 — 音频字幕/播放行为/显示偏好分卡片
27. ✅ 列表视图进度条 — MediaListItem 显示播放进度 + 已看/收藏标记

### 第五批（全模板审计）— ✅ 全部完成
28. ✅ 本地播放器全部 20+ 端点兼容性验证 — 全部已存在
29. ✅ 演员详情页 PersonPage — `/person/:id` 显示演员信息+参演作品列表
30. ✅ 演员点击跳转升级 — ItemPage/SeriesPage 演员卡片点击跳转到 PersonPage

### 第六批（功能优化）— ✅ 全部完成
31. ✅ 播放列表播放修复 — PlaylistDetailPage 修正 `query.id` → `playbackRoute(item)`
32. ✅ 服务器重启/关机 — DashboardHome 添加重启/关机按钮 + 确认弹窗 + restartServer/shutdownServer API
33. ✅ 花絮/特别内容 — ItemPage 添加 specialFeatures 区块（SpecialFeatureCount + getSpecialFeatures API）
34. ✅ 制片工作室详情页 — `/studio/:name` StudioPage（工作室信息+作品列表） + getStudio/getStudioItems API
35. ✅ 工作室链接升级 — ItemPage/SeriesPage 工作室按钮跳转到 StudioPage 而非搜索
36. ✅ GenrePage/SearchPage 防御性修复 — `Items ?? []` 防空响应崩溃
37. ✅ AccountSettings 加载错误提示 — 偏好加载失败时显示错误告警而非静默失败
38. ✅ 可配置快进快退步长 — skipSteps store + AccountSettings 配置面板 + VideoPlaybackPage 使用

### 后端修复 — ✅ 全部完成
39. ✅ ActivityLog userId 筛选 — 支持按用户过滤活动日志
40. ✅ ScheduledTasks PUT 路由 — Emby 客户端 PUT 兼容
41. ✅ 日志/活动端点 admin 权限 — 防止非管理员访问敏感信息
42. ✅ thumb_image_tag — BaseItemDto 添加 ThumbImageTag 字段
43. ✅ MediaSourceId serde — PlaybackReport/LegacyPlaybackQuery 正确反序列化
44. ✅ PlaybackReport 扩展 — 添加 can_seek/event_name 字段
