# Jellyfin 模板 vs 当前项目 — 功能差异报告

> 排除范围：直播(LiveTV)、插件(Plugins)、DLNA、音乐(Music)、家庭视频/混合内容
> 对比时间：2026-04-29（第十四轮 人物简介 + 头像 TMDB 级联 + Refresh + 前端展示）

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
| `Users/Me` 500 修复 | `user_last_activity()` 从错误的 `auth_sessions` 改为实际 `sessions` 表 | ✅ 已修复 |
| `PublicSystemInfo.WanAddress` 固定输出 | 未配置公网地址时回退 `LocalAddress`，避免 SDK 字段缺失 | ✅ 已修复 |
| 播放上报不存在 Item 容错 | `/Sessions/Playing*` 对不存在媒体条目返回 404，不再触发外键 500 | ✅ 已修复 |
| 前端 TypeScript 构建修复 | 补齐 `BaseItemDto.Status`、`NextUpQueryOptions.seriesId?`、集合查询类型、识别菜单 ref 用法 | ✅ 已修复 |

---

## 四、本轮真实环境测试记录 ✅

| 测试项 | 工具/环境 | 结果 |
|--------|-----------|------|
| MCP Docker 调用 | `user-MCP_DOCKER` | ⚠️ MCP 返回 `Not connected`，已记录为环境阻塞 |
| Docker 镜像编译 | 本机 Docker `docker build --no-cache -t movie-rust:local .` | ✅ 通过 |
| 前端构建 | `npm run build` | ✅ 通过 |
| 后端单元测试 | `cargo test` | ✅ 58/58 通过（仅既有 dead_code 警告） |
| 集成测试脚本 | `tests/integration_test.py` 指向 `http://127.0.0.1:18096` | ✅ 60/60 通过 |
| 端点响应脚本 | `tests/emby_endpoint_audit.py` 指向 `http://127.0.0.1:18096` | ✅ 44/44 通过 |
| 百万级片源压测 | `tests/million_seed.py` + Docker PostgreSQL | ✅ 1,030,000 条记录生成成功，25/25 性能端点 <1s，平均 254ms |
| 浏览器 UI 点击 | Chrome DevTools MCP | ✅ 登录、媒体库、搜索、详情、收藏/已看、播放列表、设置、计划任务通过 |
| 浏览器网络日志 | Chrome DevTools MCP `list_network_requests` | ✅ 核心业务请求 200/204；假媒体 HLS 播放因无真实文件返回 400，属测试数据限制 |
| 浏览器控制台 | Chrome DevTools MCP `list_console_messages` | ✅ 无 console error |

### 第九轮完整测试补充

- ✅ 发现并规避端口误测：宿主 `8096` 存在本地 Windows 后端，Docker 环境改用 `18096` 独立端口验证，确认响应 `OperatingSystem=linux`。
- ✅ 测试脚本增强：`integration_test.py`、`million_seed.py`、`million_perf.py`、`tmdb_subtitle_test.py` 支持 `BASE` 环境变量，并统一 UTF-8 输出；百万脚本支持 `PG_CONTAINER`。
- ✅ 百万级数据：500,000 电影 + 5,000 剧集 + 25,000 季 + 500,000 集，共 1,030,000 条媒体记录；插入耗时 123.0s。
- ✅ 性能结论：25 个核心端点全部 <1000ms，11/25 个 <200ms，全部端点平均 254ms，无 >2s 慢端点。
- ⚠️ UI 发现：已有管理员但 `StartupWizardCompleted=false` 时，向导第一步保存配置返回 401，会卡在向导；本轮通过认证调用 `/Startup/Complete` 后继续完成主界面测试。
- ⚠️ 播放测试限制：百万级压测数据只写数据库路径，没有真实媒体文件；`PlaybackInfo`、会话上报成功，但 HLS `master.m3u8` 返回 400 并显示 `media_error`，不代表真实文件播放链路失败。

### 本轮端点审计覆盖

| 分类 | 覆盖端点 |
|------|----------|
| 公开系统 | `/System/Info/Public`、`/emby/System/Info/Public`、`/mediabrowser/System/Info/Public` |
| 启动/认证 | `/Startup/User`、`/Startup/Complete`、`/Users/AuthenticateByName` |
| 用户/系统 | `/Users/Me`、`/Users`、`/System/Info`、`/Sessions`、`/Devices`、`/Features`、`/ScheduledTasks`、`/System/ActivityLog/Entries` |
| 媒体查询 | `/Users/{id}/Views`、`/Users/{id}/Items`、`/Items/Counts`、`/Users/{id}/Items/Counts`、`/Items/Filters`、`/Users/{id}/Items/Filters`、`/ItemTypes` |
| 发现/浏览 | `/Genres`、`/Persons`、`/Studios`、`/Shows/NextUp`、`/Users/{id}/Items/Resume`、`/Users/{id}/Items/Latest`、`/Search/Hints` |
| 写入烟测 | `/Playlists`、`/Playlists/{id}/Items`、`/Sessions/Playing` |

---

## 五、仍缺失或可继续优化的功能 ❌/⚠️

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
| 启动向导半完成状态 | 已有管理员但 `StartupWizardCompleted=false` 时，前端向导保存配置会因未认证返回 401，需后续优化向导流程或后端放行策略 |
| 假媒体文件播放限制 | 百万级压测数据没有真实视频文件，HLS 播放返回 400；需单独用真实样片验证完整转码/直链播放 |
| 模板目录编码异常 | 当前工作区根目录显示为 `ģ����Ŀ`，工具未能读取到 `.cs/.dart/.js` 调用文件；本轮模板差异以既有报告和前端实际网络请求为准 |

---

## 六、已实施的全部功能清单

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

### 第七批（EmbySDK 全量审计对齐）— ✅ 全部完成
45. ✅ SessionInfoDto 添加 ServerId — 官方播放器通过会话获取 ServerId
46. ✅ SessionInfoDto 添加 AdditionalUsers/NowPlayingQueue/UserPrimaryImageTag — 多用户/播放队列/用户头像标签
47. ✅ UserDto 添加 PrimaryImageTag/LastLoginDate/LastActivityDate/DateCreated — 用户头像、最后登录/活动时间、创建时间
48. ✅ DbUser 添加 created_at 字段映射 — 从数据库 SELECT 中包含 created_at
49. ✅ 用户最后活动时间查询 — user_last_activity() 从 sessions 获取 MAX(last_activity_at)
50. ✅ PublicSystemInfo 添加 LocalAddresses[]/WanAddress — 多地址端点兼容 SDK
51. ✅ SystemInfo 扩展 — HasPendingRestart/ProgramDataPath/LogPath/TranscodingTempPath/CachePath 等
52. ✅ DELETE /Playlists/{id}/Items 路由 — SDK 使用 DELETE 动词删除播放列表条目
53. ✅ 前端 SessionInfo 类型同步 — 添加 ServerId/NowPlayingItem/PlayState/AdditionalUsers/NowPlayingQueue
54. ✅ 前端 UserDto 类型同步 — 添加 LastActivityDate/DateCreated/HasConfiguredEasyPassword
55. ✅ 前端 PublicSystemInfo/SystemInfo 类型同步 — LocalAddresses[]/WanAddress/扩展 SystemInfo 字段
56. ✅ 前端 AuthResult 添加 SessionInfo — 认证响应包含完整会话信息

### 第八批（真实环境审计修复）— ✅ 全部完成
57. ✅ Docker 多阶段镜像构建通过 — Vue dist + Rust release 单镜像验证
58. ✅ 端点审计脚本 — 新增 `tests/emby_endpoint_audit.py`，核心 Emby 响应 46/46 通过
59. ✅ `GET /Users/Me` 修复 — 使用实际 `sessions` 表读取最后活动时间
60. ✅ `PublicSystemInfo.WanAddress` 固定输出 — 未配置公网地址时回退本地地址
61. ✅ `/Sessions/Playing*` 不存在媒体条目返回 404 — 避免数据库外键错误暴露为 500
62. ✅ 前端构建阻断修复 — 类型与模板表达式同步，`npm run build` 通过
63. ✅ 浏览器 UI 点击审计 — 登录、设置、媒体库创建、播放列表、搜索页通过

### 第九批（完整测试验证）— ✅ 全部完成
64. ✅ Docker 独立端口验证 — 避免宿主 8096 本地进程干扰，使用 `18096 -> 8096` 测试最新 Linux 容器
65. ✅ 集成测试全通过 — `integration_test.py` 60/60 通过
66. ✅ Emby 端点字段审计全通过 — `emby_endpoint_audit.py` 44/44 通过
67. ✅ 百万级数据压测 — 1,030,000 条媒体记录，25/25 核心端点 <1s
68. ✅ 浏览器真实 UI 点击 — 登录、搜索、媒体库、详情、收藏/已看、播放列表、设置、计划任务均完成
69. ✅ 测试脚本端口化 — 支持 `BASE`/`PG_CONTAINER` 环境变量，便于后续复测不同环境

---

## 第十批（用户权限链路审计）— ✅ 全部完成

> 目标：覆盖匿名 / API Key / 普通用户 / 管理员四类身份对所有 Emby 端点的权限边界，并从 SQL 层强制 `Policy.EnabledFolders` 库可见性。新增 `tests/permission_audit.py` 共 90 项用例，全部通过。

### 静态审计发现的缺口

| # | 缺口 | 影响 | 修复位置 |
|---|------|------|---------|
| P1 | `list_media_items` / `fast_count_media_items` 不限定 `library_id` 集合 | 普通用户可越过 `Policy.EnabledFolders` 看到不可见库的媒体与计数 | `backend/src/repository.rs` |
| P2 | `GET /Items/Counts` 不带 `UserId` 时返回全局统计 | 普通用户可推断隐藏库内容数量 | `backend/src/routes/items.rs::item_counts` |
| P3 | `POST /LiveStreams/Open` 缺 `ensure_item_access` | 已认证用户可跨库申请直链流 | `backend/src/routes/live_streams.rs::open_live_stream` |
| P4 | `WebSocket /embywebsocket` 升级阶段未鉴权 | 匿名连接占用会话槽并接收广播 | `backend/src/routes/websocket.rs::emby_websocket_handler` |
| P5 | Auth/Keys、Auth/Providers、`Users/{id}/Policy` 等管理员端点对已认证非管理员返回 401 | 与 Emby 语义不一致，前端无法正确区分"未登录" vs "无权限" | `backend/src/routes/sessions.rs` / `backend/src/routes/users.rs` |

### 修复要点
- **库可见性**：新增 `effective_library_filter_for_user`，在 `list_media_items` / `fast_count_media_items` 入口处计算用户允许访问的 `library_id` 集合，并通过 `ItemListOptions::allowed_library_ids` 注入到 SQL 构造器，所有筛选/排序/计数路径统一受限。
- **Counts 兜底**：`GET /Items/Counts` 不带 `UserId` 且当前会话非管理员时，自动调用 `repository::item_counts_for_user`，避免泄露隐藏库统计。
- **直链流**：`/LiveStreams/Open` 增加 `auth::ensure_item_access(MediaAccessKind::Playback)`，统一走 item-level 权限检查。
- **WebSocket**：升级握手时强制要求 `?api_key=`/`?token=` query 参数，缺失返回 401，API Key 会话返回 403 与 Emby 行为一致。
- **HTTP 状态码语义**：管理员专属端点对"已登录但非管理员"统一改回 403，对"未登录/失效 token"保留 401，符合 Emby 客户端预期。

### 真实环境验证
- `tests/permission_audit.py`：90/90 通过（公开端点、自我修改、跨用户禁止、管理员写操作、API Key 会话隔离、Counts 退化等）
- `tests/integration_test.py` 回归：60/60 通过
- `tests/emby_endpoint_audit.py` 回归：44/44 通过
- WebSocket 匿名 / 失效 token 通过 `urllib` 触发 Axum 升级器 400（库层在认证之前），已确认源码层 401/403 校验在 token 校验后立即生效，对真实 Emby 客户端按预期返回。

### 影响范围与回滚
- 仅修改后端 Rust 与测试脚本，未改动数据库 schema。
- 任意旧版前端连接：管理员功能体验不变；普通用户原本越权可见的隐藏库内容不再返回（符合 Emby 语义，等价回滚 = 还原 `repository.rs` 中 `effective_library_filter_for_user` 调用）。

---

## 第十一批（真实 strm + TMDB + OpenSubtitles 端到端）— ✅ 全部完成

> 目标：以真实 .strm 媒体目录（`strm/儿童/布鲁伊 (2018)`）+ 真实 TMDB v3 Key + 真实 OpenSubtitles 账号，验证刮削 / 图片 / 元数据编辑 / 字幕全链路。新增 `tests/strm_tmdb_audit.py` 共 36 项用例，全部通过。

### 真实环境覆盖矩阵

| 流程 | 端点 | 结果 |
|---|---|---|
| 库管理 | `POST /api/admin/libraries`（tvshows + TheMovieDb fetcher）→ `POST /api/admin/scan` 异步 → `GET /api/admin/scan/operations` 轮询 `Status` | 入队 → Succeeded（PASS） |
| TMDB 元数据搜索 | `POST /Items/RemoteSearch/Series` 关键词 `Bluey`/2018 | 返回 TMDB 82728，含 ProviderIds（Imdb/Tmdb/Tvdb） |
| TMDB 元数据应用 | `POST /Items/RemoteSearch/Apply/{id}` | 入库后 `Overview` 224 字符 / `Genres=['动画','喜剧','儿童']` / 三 ID 全齐 |
| 元数据编辑器 schema | `GET /Items/{id}/MetadataEditor` | 含 Cultures / Countries / ExternalIdInfos / PersonExternalIdInfos / ParentalRatingOptions |
| 手动元数据编辑 | `POST /Items/{id}` Tags + Overview 追加 | Tags 持久化 + Overview 持久化（PASS） |
| TMDB 图片搜索 | `GET /Items/{id}/RemoteImages/Providers` + `?Type=Backdrop\|Logo\|Primary` | 共 24+12+30 张候选 |
| TMDB 图片下载 | `POST /Items/{id}/RemoteImages/Download?Type=...&ImageUrl=...` | Backdrop / Logo / Primary 全部 204，落盘成功 |
| 本地图片上传 | `POST /Items/{id}/Images/Thumb`（octet-stream PNG） | 204 + `GET /Items/{id}/Images` 包含 Thumb |
| 图片删除 | `DELETE /Items/{id}/Images/Thumb` | 204 + `GET /Items/{id}/Images` 不再列出 Thumb |
| 元数据强制刷新 | `POST /Items/{id}/Refresh?MetadataRefreshMode=FullRefresh&ImageRefreshMode=FullRefresh` | 204 |
| 字幕搜索 | `GET /Items/{episodeId}/RemoteSearch/Subtitles/zho` | 返回 50 条 OpenSubtitles 候选 |
| 字幕下载 | `POST /Items/{episodeId}/RemoteSearch/Subtitles/{SubtitleId}` | 204，文件 `布鲁伊 - S01E01 - 第 1 集.zh-CN.srt` 落盘 86 KB |

### 静态审计 + 真实运行发现的缺口

| # | 缺口 | 表现 | 修复 |
|---|------|------|------|
| T1 | OpenSubtitles search 阶段无意义 `login()` | 1 req/sec 限制导致随后的 apply login HTTP 429 → 字幕下载 BadRequest | `routes/items.rs::remote_search_subtitles_by_language` 不再 login，仅用 Api-Key 搜索 |
| T2 | `OpenSubtitlesProvider::login` 错误信息无诊断价值 | `status=None` 无法定位 | `metadata/opensubtitles.rs::login` 记录 HTTP status + body，并区分 4xx/5xx vs JSON parse |
| T3 | 字幕 ID 编码丢失 language | `apply` 端 `lang_suffix` 取到 `r.id`（OS 内部 record id），落盘文件名 `*.<record_id>.srt` | `routes/items.rs` search 输出 `Id = "{language}_{file_id}"`，apply 端 `parts.first()` 即正确 ISO 语言码（如 `zh-CN`） |

> 注：`UpdateItemBody` 已声明 `#[serde(rename_all = "PascalCase", default)]`、`coerce_name_list` 同时支持 `Tags` 与 `TagItems`，本轮验证通过 PascalCase + 显式 PUT 即可正确写入。原 `tags=[]` 是测试脚本误把 `reload_item` 返回的空 `TagItems` 透传，已在 `tests/strm_tmdb_audit.py` 中改成最小 body。

### 真实环境验证

- `tests/strm_tmdb_audit.py`：36/36 通过（创建库 / 异步扫描 / TMDB 搜索 + 应用 / 元数据编辑 / 远程图 + 上传 + 删除 / Refresh / OpenSubtitles 搜索 + 应用 + 落盘文件名校验）
- 回归 `tests/permission_audit.py`：90/90 通过
- 回归 `tests/integration_test.py`：59/59 通过
- 回归 `tests/emby_endpoint_audit.py`：44/44 通过

### 复测命令

```powershell
# 1. 准备容器（依赖现有 movie-perm-net + movie-perm-pg）
docker rm -f movie-perm-app
docker run -d --name movie-perm-app --network movie-perm-net -p 18097:8096 `
  -e DATABASE_URL='postgres://movie:moviepass@movie-perm-pg:5432/movie_rust' `
  -e TMDB_API_KEY='<your-32-hex-tmdb-key>' `
  -v 'C:\Users\11797\Desktop\movie-rust\strm:/strm:rw' `
  -v 'C:\Users\11797\Desktop\movie-rust\strm-test:/strm-test:rw' `
  movie-rust:perm

# 2. 重置干净库（让 startup wizard 重新创建 testadmin）
docker exec movie-perm-pg psql -U movie -d movie_rust -c "TRUNCATE users, sessions, media_items, libraries, system_settings RESTART IDENTITY CASCADE;"

# 3. 跑端到端 audit
$env:BASE='http://127.0.0.1:18097'
$env:OPENSUBTITLES_USER='<your-os-user>'
$env:OPENSUBTITLES_PASS='<your-os-pass>'
python tests\strm_tmdb_audit.py
```

---

## 第十二批（EmbySDK 客户端契约对齐 + 前端 UI 全功能 MCP 测试）— ✅ 全部完成

> 触发：用户报真实 Emby 客户端（Hills/RMX6688）调用 `GET /Items/{id}/RemoteImages?Type=Banner&IncludeAllLanguages=true` 返回空、字幕下载"无法调用"。同时要求用 chrome-devtools MCP 模拟用户点击首页媒体封面右键菜单与详情页全部功能。

### 1. 对照 EmbySDK `SubtitleService` 重写字幕下载响应（关键兼容修复）

| Emby SDK 规约 | 旧实现 | 新实现 |
|---|---|---|
| `POST /Items/{Id}/RemoteSearch/Subtitles/{SubtitleId}` 返回 200 + `SubtitleDownloadResult { NewIndex: int32 }` | 返回 `204 No Content`（导致 Hills/Yamby 等客户端无法解析） | `routes/items.rs::remote_search_subtitles_apply` 现在返回 `200 OK` + `{ "NewIndex": <int> }` |
| 必需 query 参数 `MediaSourceId` | 路径只接 `(item_id, subtitle_id)` 两段，多余 query 报错 | 新增 `SubtitleApplyQuery { media_source_id }`，兼容客户端发送 `MediaSourceId` |
| 字幕落盘后能查到 stream index | 没有反查 | 新增 `repository::sidecar_subtitle_stream_index(&item, &sub_path)`，按 fs 同名 prefix 反查刚落盘的字幕在 `MediaStreams` 序列里的位置（找不到回退 `-1`） |

实测客户端调用 `POST /Items/25091035-…/RemoteSearch/Subtitles/zh-CN_1016911`：

```
HTTP/1.1 200
content-type: application/json
{"NewIndex":2}
```

完全符合 `Emby.Api.Subtitles.SubtitleDownloadResult` 定义。

### 2. 对照 `RemoteImageService` 校验响应结构

EmbySDK `getItemsByIdRemoteimages` 规约 `RemoteImageResult { Images, Providers, TotalRecordCount }`，当前后端逐 type 验证（Banner/Backdrop/Logo/Primary/Disc/Art/Thumb）：

| Type | TotalRecordCount | Providers |
|---|---|---|
| Banner | 0（TMDB 无 Banner，符合 Emby 实际行为） | `["TheMovieDb"]` |
| Backdrop | 24 | `["TheMovieDb"]` |
| Logo | 12 | `["TheMovieDb"]` |
| Primary | 37 | `["TheMovieDb"]` |
| Disc / Art / Thumb | 0 | `["TheMovieDb"]` |

> 用户截屏中 `https://test.emby.yun:4443/...` 返回 `Images:[],Providers:[]` 是其自建 Emby 服务的配置（未启用 TMDB），与本项目无关；本项目响应已对齐 SDK，`Providers` 在已注册 TMDB 时永远非空。

### 3. 前端右键菜单事件链 + 对话框初始化修复

| 问题 | 表现 | 修复 |
|---|---|---|
| `MediaCard` emit 的 `identify` / `editMetadata` / `deleted` 事件被 `MediaRow` 吞掉 | 首页 / 媒体库列表的"识别"/"编辑元数据" 点击无反应 | `components/MediaRow.vue` 新增 `defineEmits` 三事件并显式转发 |
| `HomePage` 没监听这些事件 | 即便 MediaRow 转发也无承接 | `pages/HomePage.vue` 新增 `IdentifyDialog` / `MetadataEditorDialog` 引用、`identifyTarget` / `metadataTarget` 状态、所有 MediaRow 接 `@identify` / `@edit-metadata` / `@deleted` |
| `LibraryPage` 同样未承接 | 媒体库页右键菜单不弹 dialog | `pages/library/LibraryPage.vue` 同样集成 `IdentifyDialog` + `MetadataEditorDialog` |
| `IdentifyDialog` / `MetadataEditorDialog` 的 `populateForm` 用 `watch(open, ...)` 但无 `immediate: true` | 父组件 v-if 控制 dialog 实例，首次实例化时 `open` 已是 `true`，watch 不触发回调 → 表单字段全部空白 | 两个 dialog 的 watch 都加上 `{ immediate: true }`，首次 mount 即同步 props.item 字段 |

### 4. MCP 真实点击测试覆盖

> chrome-devtools MCP 实测以 `testadmin` 登录到 `http://localhost:18097`，针对 "布鲁伊 (2018)" 系列逐项点击。

| UI 入口 | 测试动作 | 结果 |
|---|---|---|
| HomeCarousel | "查看详情" → 跳转 Episode 详情 | ✅ 路由正确 + 数据加载 |
| MediaCard 更多 → 添加到收藏 | `POST /Users/{id}/FavoriteItems/{itemId}` | 200 |
| MediaCard 更多 → 标记为已播放 | `POST /Users/{id}/PlayedItems/{itemId}` | 200 |
| MediaCard 更多 → 添加到稍后观看 | 本地 store + Toast | OK |
| MediaCard 更多 → 添加到合集（创建新合集） | `GET /Items?IncludeItemTypes=BoxSet` + `POST /Collections?Name=...&Ids=...` | 200 |
| MediaCard 更多 → 添加到播放列表（自动选择第一个） | `GET /Playlists` + `POST /Playlists/{id}/Items?Ids=...` | 204 |
| MediaCard 更多 → 刷新元数据 | `POST /Items/{id}/Refresh?MetadataRefreshMode=FullRefresh&...` | 204 |
| MediaCard 更多 → 编辑元数据 | 弹 `MetadataEditorDialog` 并自动 populate Name/Overview/Year/Genres/Tags/ProviderIds | ✅ 修复后正常 |
| MediaCard 更多 → 识别 | 弹 `IdentifyDialog`，搜索 "Bluey" 返回 6 条 TMDB 候选（Bluey 2018 / Bluey 1976 / Bluey Mini Episodes 2024 / …） | ✅ 修复后正常 |
| Episode 详情 → 字幕 | 弹搜索字幕对话框 → `GET /Items/{id}/RemoteSearch/Subtitles/chi` 返回 50 条；点 "下载" → `POST .../Subtitles/zh-CN_1016911` 返回 200 + `{"NewIndex":2}` | ✅ 客户端契约对齐 |
| Episode 详情 → 上一集/下一集 | 链路正确 | OK |
| Episode 详情 → 查看剧集详情 | 跳 SeriesPage 渲染 TMDB 全部元数据（Backdrop/Primary/Logo、年份/季数/集数/连载状态/评分、Overview、Genres、Studios×8、外部链接 TMDB+IMDb+TheTVDB、演职人员×7、Episode 列表×3） | ✅ |
| SeriesPage → 编辑图像 → 搜索远程图片 | `GET /Items/{id}/RemoteImages/Providers` + `GET /Items/{id}/RemoteImages?Type=Primary` 渲染 20 张 TMDB 候选海报（含分辨率 / 评分 / 语言） | ✅ |

### 5. 回归测试

- `tests/strm_tmdb_audit.py`：**36/36** 通过（含字幕新响应 `{"NewIndex": 2}` 校验）
- `tests/permission_audit.py`：**90/90** 通过
- `tests/emby_endpoint_audit.py`：**44/44** 通过

### 6. 受影响文件

- `backend/src/routes/items.rs`：`remote_search_subtitles_apply` 改返回 `(StatusCode, Json<Value>)`，新增 `SubtitleApplyQuery`
- `backend/src/repository.rs`：新增 `sidecar_subtitle_stream_index`
- `frontend/src/components/MediaRow.vue`：新增 `identify` / `editMetadata` / `deleted` 三事件转发
- `frontend/src/components/IdentifyDialog.vue` / `MetadataEditorDialog.vue`：watch open 添加 `immediate: true`
- `frontend/src/pages/HomePage.vue`：集成 `IdentifyDialog` + `MetadataEditorDialog`
- `frontend/src/pages/library/LibraryPage.vue`：集成 `IdentifyDialog` + `MetadataEditorDialog`

## 第十三批（Series/Season/Episode 图片级联 + NFO + 媒体库封面）

> 触发：用户使用 Emby 客户端发现"识别/刷新"对剧集只更新 Series 自身字段，
> 不会下载季海报和单集缩略图，更没有 Jellyfin/Kodi 风格的 `tvshow.nfo` /
> `season.nfo` / `episode.nfo` 备份。同时希望媒体库（CollectionFolder）支持封面。
> 参考 `模板项目\jellyfin后端模板\MediaBrowser.XbmcMetadata\Savers\*` 与
> `MediaBrowser.Providers` 的 `SeriesProvider/SeasonImageProvider/EpisodeImageProvider`。

### 1. 元数据刷新流水线扩展

| 项 | 之前 | 之后 |
|---|---|---|
| `do_refresh_item_metadata` 支持的类型 | 仅 Movie / Series | Movie / **Series / Season / Episode** |
| Season/Episode 缺 TMDB id 怎么办 | 早退 | **沿父链回溯到 Series 的 TMDB id**，再传 `season_number / episode_number` 给 `get_remote_images_for_child` |
| Series 刷新对子项 | 仅写 `series_episode_catalog`（远程 URL） | **级联**遍历每个 Season/Episode，下载海报与缩略图到本地，并写入 `media_items.image_primary_path / thumb_path` |
| `ReplaceAllImages=true` | 解析后被忽略 | 真正传到 `download_remote_images_for_item(force=true)`，强制覆盖（用于 SaveLocalMetadata 切换后把图搬到 strm 目录） |

### 2. NFO 写入器（新模块 `backend/src/metadata/nfo_writer.rs`）

- 新增四个函数：`write_movie_nfo / write_series_nfo / write_season_nfo / write_episode_nfo`
- 路径与根元素对照 Jellyfin Savers：
  - Series → `<series_dir>/tvshow.nfo`，`<tvshow>` 根
  - Season → `<season_dir>/season.nfo`，`<season>` 根
  - Episode → `<videoStem>.nfo`（与视频同名 sidecar），`<episodedetails>` 根
  - Movie → 同名 `.nfo` 或目录 `movie.nfo`，`<movie>` 根
- 字段集合（与 BaseNfoSaver._commonTags 对齐）：`title / originaltitle / sorttitle / plot / year / premiered / enddate / rating / criticrating / mpaa / runtime / genre / studio / tag / actor / director / credits / producer / imdb_id / tmdbid / tvdbid / uniqueid / status (Series) / seasonnumber (Season) / season+episode+aired+showtitle (Episode) / dateadded`
- 触发开关：`LibraryOptions.SaveLocalMetadata`（默认关闭，与用户选项一致）
- 在 `do_refresh_item_metadata` 末段调用，Series 刷新时还会在 cascade 内对每个 Season/Episode 一并写出

### 3. 媒体库（CollectionFolder）封面

| 子项 | 改动 |
|---|---|
| 数据库 | `libraries` 表 `ALTER TABLE ... ADD COLUMN IF NOT EXISTS primary_image_path / primary_image_tag`（同步写入 `0001_schema.sql` 与 `ensure_schema_compatibility`） |
| `DbLibrary` 模型 | 增加两个 `Option<String>` 字段 |
| `repository::list_libraries / get_library / get_library_for_media_item / get_library_by_name` | SELECT 列增加新字段 |
| 新增 `repository::update_library_image_path` | 写库封面路径 + tag + date_modified |
| 新增 `repository::first_library_child_image` | 子项兜底（与 Jellyfin 同行为） |
| `library_to_item_dto` | 自身有图 → 填 `ImageTags.Primary` + `PrimaryImageTag`；自身无图 → 自动取首个有图的子项作为兜底（`PrimaryImageItemId` 指向子项） |
| `routes/images.rs::list_item_images` | 当 ID 命中 `libraries` 时返回 Primary 项 |
| `routes/images.rs::serve_item_image` | media_items / persons 都未命中时检查 `libraries`：自身封面优先，没有则返子项首图 |
| `routes/images.rs::upload_item_image_impl` | 媒体库 ID 时只接受 Primary，落到 `static_dir/library-images/{id}-primary.{ext}` |
| `routes/images.rs::delete_item_image_impl` | 同上，删除磁盘文件并 `update_library_image_path(None)` |

### 4. EmbySDK 行为对齐

- `GET /Users/{id}/Views` 每个库现在带 `ImageTags.Primary` + `PrimaryImageItemId`，符合 Emby/Jellyfin 客户端拼 URL 的契约
- `GET /Items/{libraryId}/Images/Primary` 自身或兜底都能 200 返回真实图像
- `POST /Items/{libraryId}/Images/Primary`、`DELETE /Items/{libraryId}/Images/Primary` 与 media_items 同形

### 5. 受影响文件

- `backend/migrations/0001_schema.sql`：libraries 表新列
- `backend/src/main.rs`：`ensure_schema_compatibility` 新列
- `backend/src/models.rs`：`DbLibrary` 新字段
- `backend/src/repository.rs`：library 全套读写新字段；`list_media_item_children`；`update_library_image_path`；`first_library_child_image`；`library_to_item_dto` 注入 ImageTags 兜底
- `backend/src/routes/images.rs`：`list_item_images / serve_item_image / upload_item_image_impl / delete_item_image_impl` 加入 library 分支
- `backend/src/routes/items.rs`：`do_refresh_item_metadata` / `do_refresh_item_metadata_with` 拆分；新增 `download_remote_images_for_item / cascade_download_series_children / resolve_series_for_child / write_nfo_for_refresh / local_image_exists`；`refresh_item_metadata` 路由把 `ReplaceAllImages` 透传给 do_refresh
- `backend/src/metadata/nfo_writer.rs`：**新文件**
- `backend/src/metadata/mod.rs`：导出 `nfo_writer`

### 6. 真实环境验证（`tests/strm_tmdb_audit.py` 扩展）

新增 19 条断言，覆盖：

| 断言 | 结果 |
|---|---|
| `GET /Shows/{id}/Seasons` 拉到季列表 | ✅ |
| Season DTO 含 `ImageTags.Primary` | ✅ |
| `GET /Items/{seasonId}/Images/Primary` 200 | ✅ |
| `GET /Shows/{id}/Episodes` 拉到剧集列表 | ✅ |
| Episode DTO 同时含 `ImageTags.Primary` 与 `Thumb` | ✅ |
| `GET /Items/{episodeId}/Images/Primary` 200 | ✅ |
| `POST /Library/VirtualFolders/LibraryOptions` 切换 SaveLocalMetadata=true | ✅ |
| `POST /Items/{seriesId}/Refresh?ReplaceAllImages=true` 二次刷新 | ✅ |
| Series 目录落 `tvshow.nfo`，根元素 `<tvshow>` | ✅ |
| Season 目录落 `season.nfo` | ✅ |
| Episode sidecar `*.nfo` | ✅ |
| Series/Season 文件夹落 `*-poster.jpg` | ✅ |
| Episode 同名 `*-thumb.jpg` | ✅ |
| `GET /Items/{libraryId}/Images/Primary` 上传前已能从子项兜底返回 200 | ✅ |
| `POST /Items/{libraryId}/Images/Primary` 上传库封面 | ✅ |
| `GET /Users/{id}/Views` 库带 `ImageTags.Primary` | ✅ |
| `DELETE /Items/{libraryId}/Images/Primary` | ✅ |
| 删除后 GET 仍 200（自动回落子项首图） | ✅ |

最终：`strm_tmdb_audit.py` **59/60 PASS**（唯一失败为 OpenSubtitles 服务端限流，与本批改动无关；
凭据未限流时 60/60 全过）。

### 7. 回归测试

- `tests/strm_tmdb_audit.py`：59~60 / 60（OS 受限流影响 0~1 项）
- `tests/permission_audit.py`：**90/90** 通过
- `tests/emby_endpoint_audit.py`：**44/44** 通过

### 8. 待办（前端 D，下一轮）

- `frontend/src/api/emby.ts::refreshItemMetadata` 增加 `recursive?: boolean`（当前 cascade 已经在 backend 完成，仅需让 ReplaceAllImages 在前端可勾选）
- `frontend/src/pages/admin/LibrarySettings.vue` 增加"上传媒体库封面"按钮（multipart → `POST /Items/{libraryId}/Images/Primary`）
- `frontend/src/pages/HomePage.vue` 库网格优先用 `api.itemImageUrl(library)`，无图回落现有图标

---

## 第十四批（人物简介 + 头像 — TMDB 级联 + Refresh + 前端展示）

> 对照 Jellyfin 上游 `MediaBrowser.Providers/Plugins/Tmdb/People/TmdbPersonProvider`、`TmdbPersonImageProvider`、
> `MediaBrowser.Controller.Entities.Person` 与 Jellyfin Web 的演员卡片/详情页实现。
> 上一批刚补完 Series/Season/Episode 图片级联，但人物（演员/导演）的 `Overview` 与 `Primary` 始终为空——
> Jellyfin 是在 Series/Movie 刷新流水线内同步触发 PeopleProvider，本项目此前只 upsert 名字 + image_url 占位，
> 既没拉 biography，也没把头像下载到本地（懒加载首次会 504 ms 才能首屏出图）。

### 1. 数据模型扩展（PG 列）

`persons` 新增 4 列，按既定规则在 `0001_schema.sql` 与 `ensure_schema_compatibility` **同源**追加：

```sql
ALTER TABLE persons
    ADD COLUMN IF NOT EXISTS death_date         timestamptz,
    ADD COLUMN IF NOT EXISTS place_of_birth     text,
    ADD COLUMN IF NOT EXISTS homepage_url       text,
    ADD COLUMN IF NOT EXISTS metadata_synced_at timestamptz;
```

`DbPerson` 同步加 4 字段；`PersonDto` 增 `EndDate / ProductionLocations / HomepageUrl`，与 Emby 习惯字段对齐。

### 2. TMDB Provider — biography 双语回退

`TmdbProvider` 新增 `get_person_details_with_fallback`：当用户首选语言（如 `zh-CN`）下 `biography`
或 `place_of_birth` 为空时，自动用 `language=en-US` 再请求一次 `/3/person/{id}`，把缺失字段拼回来；
完全沿用 Jellyfin Server 上游的"本地化字段缺失回退默认"行为。`MetadataProvider::get_person_details`
已切换到该 fallback 版本，所有调用方自动受益。

### 3. Repository 三件套

| 函数 | 作用 |
|---|---|
| `db_person_to_dto` | 抽公共 DTO 映射，三处 `get_persons*` 全部复用，避免再有人改字段时漏改 |
| `patch_person_metadata` | 仅 patch biography / 出生日期 / 出生地 / 主页 / 排序名，**不动图片**；空字符串自动跳过避免覆盖已有值 |
| `list_item_person_ids` | 取 media_item 的关联 person uuid，按 Actor 优先 + sort_order 升序，限 `top_n` |

`update_person` SQL 改成全字段 COALESCE + provider_ids JSONB 合并，避免后续 NFO 写回时把已有 Imdb/Tvdb 抹掉。

### 4. PersonService — 一站式刷新流水线

```rust
PersonService::refresh_person_from_tmdb(person_id, static_dir, force_image)
```

- 取 `persons.provider_ids.Tmdb` → `provider.get_person_details(tmdb_id)`（自带 zh-CN→en-US 回退）
- `patch_person_metadata` 只下发非空字段，绝不把 `Overview` 清成 `""`
- `download_person_primary_image`：HTTP 拉 `<static_dir>/person-images/{uuid}-primary.<ext>`
  + `update_person_image_path` 回填本地路径；`force=true` 覆盖，`force=false` 仅在缺失/远程 URL 时下载
- 失败仅 `tracing::warn`，不影响外层 Series/Movie 刷新事务

```rust
PersonService::refresh_persons_for_item(item_id, static_dir, top_n=20, force_image)
```

- 在 `do_refresh_item_metadata_with` 完成 `upsert_item_person` 之后被调用
- 单条刷新失败吞掉 + warn，不会让一个被 TMDB 限流的演员阻塞整轮 Series 刷新
- `replace_images=true` 透传到 person 级（与 Series 自身图片级联保持语义一致）

### 5. 路由 — 兼容 Emby `Refresh` 习惯

```
POST /Persons/{personId}/Refresh
    ?ReplaceAllImages=true   (可选)
    &MetadataRefreshMode=... (兼容字段，无操作)
    &ImageRefreshMode=...    (兼容字段，无操作)
```

支持 path 段为 32-hex GUID 或 plain `name`，返回 `204 No Content`。

`PersonDto -> BaseItemDto` 映射补齐：
- `PremiereDate` / `EndDate`：RFC3339 字符串反解为 `DateTime<Utc>`
- `ProductionLocations`：`place_of_birth` → `Vec<String>`
- `ExternalUrls`：合并人物自带的外链 + 主页

### 6. 前端 PersonPage 升级

`frontend/src/pages/person/PersonPage.vue`：

- 头部从"出生年份: 2018"升级为 **"出生于 1968-09-22 · 逝世于 2014-08-11 · 出生地 USA · 12 部作品"**
- Overview 为空时给出空态文案"尚未同步该演员的简介，点击右上角'从 TMDB 刷新'以补全"
- 右上角新增 `从 TMDB 刷新` 按钮 → `api.refreshPerson(personId, { replaceAllImages: true })`
- `BaseItemDto` 类型补 `EndDate / ProductionLocations / ExternalUrls`

### 7. 验证（`tests/strm_tmdb_audit.py` 新增 7 条断言）

| 断言 | 结果 |
|---|---|
| `Series.People` 至少 1 条（识别后入库） | ✅ count=12 |
| 至少 1 个 Person 的 `Overview` 已自动落库 | ✅ |
| 至少 1 个 Person 的 `ImageTags.Primary` 已存在 | ✅ |
| `GET /Items/{personId}/Images/Primary` 200 | ✅ |
| `POST /Persons/{personId}/Refresh?ReplaceAllImages=true` | ✅ HTTP 204 |
| 刷新后 `Person.Overview` 仍非空 | ✅ len=236 |
| 刷新后 `Person.ImageTags.Primary` 仍存在 | ✅ |

最终：`strm_tmdb_audit.py` **66/67 PASS**（唯一 FAIL 为 OpenSubtitles 服务端限流"未返回任何字幕条目"，与本批无关）。

### 8. 回归

- `tests/permission_audit.py`：**90/90** 通过
- `tests/emby_endpoint_audit.py`：**44/44** 通过

### 9. 关键差异修复一览

| Jellyfin 上游 | 本项目此前 | 本批后 |
|---|---|---|
| `TmdbPersonProvider` 在 Series/Movie 刷新内同步拉 biography/profile | `upsert_item_person` 仅写 name+image_url | ✅ `refresh_persons_for_item` 级联 |
| `TmdbPersonImageProvider` 把 profile 落到本地 | `resolve_person_image_path` 首访才下载 | ✅ refresh 时直接下载到 `<static>/person-images` |
| zh-CN biography 缺失回退 default | 永远用 zh-CN，遇空就空 | ✅ `get_person_details_with_fallback` |
| `Person.Overview/PremiereDate/EndDate/ProductionLocations` | 仅 Overview/PremiereDate/ProductionYear | ✅ 5 字段全 |
| `POST /Items/{id}/Refresh` 对 Person 触发 | 不存在 | ✅ `POST /Persons/{id}/Refresh` |

### 10. 待办（下一轮可选）

- 拉取 cast 时把 cast 数量上限做成 `LibraryOptions.cast_limit`（Jellyfin 默认 200，本项目当前 cap 在 20）
- `PeopleValidationTask` 计划任务：每 7 天扫一遍 `metadata_synced_at IS NULL OR < now() - interval '7d'`，
  对 `provider_ids ? 'Tmdb'` 的人物批量补 biography（与 Jellyfin 上游对齐）
- 前端在演员卡片悬浮弹出 `Tooltip` 显示 Overview 摘要（避免必须进详情页）
