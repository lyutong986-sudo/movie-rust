# Jellyfin 模板 vs 当前项目 — 功能差异报告

> 排除范围：直播(LiveTV)、插件(Plugins)、DLNA、音乐(Music)、家庭视频/混合内容
> 对比时间：2026-04-29

---

## 一、已有且基本完整的功能 ✅

| 功能 | Jellyfin 模板 | 当前项目 | 状态 |
|------|-------------|---------|------|
| 首页 Hero 轮播 | ✅ homesections | ✅ HeroCarousel | 完整 |
| 继续观看 | ✅ resume row | ✅ MediaRow | 完整 |
| 最近添加 | ✅ latest | ✅ latest + latestByLibrary | 完整 |
| 收藏列表 | ✅ favorites tab | ✅ favorites MediaRow | 完整 |
| 媒体库浏览 | ✅ /list | ✅ /library/:id | 完整 |
| 筛选（类型、年份、类型标签、收藏、4K、HDR、字幕） | ✅ filterdialog | ✅ UPopover 筛选面板 | 完整 |
| 排序（名称、日期、年份、评分、随机、集数） | ✅ sortmenu | ✅ USelect | 完整 |
| 无限滚动加载 | ✅ cardbuilder | ✅ IntersectionObserver | 完整 |
| 电影详情页 | ✅ /details | ✅ /item/:id | 完整 |
| 剧集详情页（季/集 Tab） | ✅ /details | ✅ /series/:id | 完整 |
| 相似推荐 | ✅ similar | ✅ similar | 完整 |
| 章节标记（详情页+播放器） | ✅ chapters | ✅ chapters | 完整 |
| 预告片嵌入 | ✅ trailer | ✅ trailerEmbed | 完整 |
| 多媒体源切换 | ✅ MediaSources tab | ✅ sourceTabs | 完整 |
| 视频播放器（HLS + 直链） | ✅ htmlVideoPlayer | ✅ video.js + hls.js | 完整 |
| 播放进度上报 | ✅ playback* | ✅ reportProgress/stopPlayback | 完整 |
| 跳过片头/片尾 (MediaSegments) | ✅ skipIntro | ✅ activeSkipSegment + skipSegment | 完整 |
| Trickplay 缩略图 | ✅ trickplay | ✅ trickplayInfo + trickplayThumbUrl | 完整 |
| 下一集自动播放 | ✅ upnext | ✅ nextUpEpisode | 完整 |
| 字幕搜索 & 下载 | ✅ subtitleeditor | ✅ searchSubtitles/downloadSubtitle | 完整 |
| 图像管理（列表/远程/上传/删除） | ✅ imageeditor | ✅ 详情页图像管理区 | 完整 |
| 刷新元数据 | ✅ refreshdialog | ✅ refreshItemMetadata | 完整 |
| 播放列表 CRUD | ✅ playlisteditor | ✅ /playlists + /playlist/:id | 完整 |
| 播放队列 | ✅ /queue | ✅ /queue | 完整 |
| 稍后观看 | ❌ (Jellyfin 无此概念) | ✅ watchLater (本地) | 当前项目额外功能 |
| 收藏/已播放 toggle | ✅ userdatabuttons | ✅ toggleFavorite/togglePlayed | 完整 |
| 右键上下文菜单 | ✅ ItemMenu | ✅ ContextMenu + MediaCard | 完整 |
| 搜索（全局 + 快速面板） | ✅ /search | ✅ /search + CommandPalette | 完整 |
| 向导 | ✅ /wizard/* | ✅ /wizard | 完整 |
| 登录/多服务器 | ✅ login/selectserver | ✅ login/select/add | 完整 |
| 忘记密码 | ✅ forgotpassword | ✅ /server/forgot-password | 完整 |
| 键盘快捷键 | ✅ 部分 | ✅ ShortcutsDialog | 完整 |
| 设置：账户/密码 | ✅ userprofile | ✅ /settings/account | 完整 |
| 设置：服务器 | ✅ /dashboard/settings | ✅ /settings/server | 完整 |
| 设置：媒体库管理 | ✅ /dashboard/libraries | ✅ /settings/libraries | 完整 |
| 设置：用户管理 | ✅ /dashboard/users | ✅ /settings/users | 完整 |
| 设置：转码 | ✅ /dashboard/transcoding | ✅ /settings/transcoding | 完整 |
| 设置：网络 | ✅ /dashboard/networking | ✅ /settings/network | 完整 |
| 设置：设备/会话 | ✅ /dashboard/devices | ✅ /settings/devices | 完整 |
| 设置：API Key | ✅ /dashboard/keys | ✅ /settings/apikeys | 完整 |
| 设置：计划任务 | ✅ /dashboard/tasks | ✅ /settings/scheduled-tasks | 完整 |
| 设置：日志 | ✅ /dashboard/logs | ✅ /settings/logs-and-activity | 完整 |
| 设置：品牌化 | ✅ /dashboard/branding | ✅ /settings/branding | 完整 |
| 设置：播放 | ✅ /dashboard/playback | ✅ /settings/playback | 完整 |
| 设置：字幕下载 | ✅ 部分 | ✅ /settings/subtitle-download | 完整 |
| 远端 Emby 中转 | ❌ | ✅ /settings/remote-emby | 当前项目独有 |

---

## 二、缺失的功能和细节 ❌

### A. 页面/路由级缺失

| 优先级 | 缺失功能 | Jellyfin 对应 | 说明 |
|--------|---------|-------------|------|
| 🔴 高 | **元数据编辑器页面** | `/metadata` (edititemmetadata) | 目前仅有"刷新元数据"，无法手动编辑标题、简介、年份、评级、类型标签、外部ID(TMDB/IMDB)等字段 |
| 🔴 高 | **用户偏好：首页区块配置** | `/mypreferenceshome` | 用户无法自定义首页显示哪些区块（继续观看、最近添加等）及排列顺序 |
| 🟡 中 | **用户偏好：显示设置** | `/mypreferencesdisplay` | 日期/时间格式、主题切换、语言偏好等独立页面（当前仅有深浅模式按钮） |
| 🟡 中 | **用户偏好：播放设置** | `/mypreferencesplayback` | 独立的播放偏好页（默认音轨语言、字幕语言等，当前塞在AccountSettings里但不够完整） |
| 🟡 中 | **管理面板首页 Dashboard** | `/dashboard` | 服务器概览小部件：活动播放、CPU/内存、最近活动、任务状态、磁盘空间等一屏聚合 |
| 🟡 中 | **单个计划任务详情 & 触发器编辑** | `/dashboard/tasks/:id` | 当前任务列表只能启动/取消，不能查看单个任务详情或编辑其触发器时间表 |
| 🟡 中 | **单个用户详情（多 Tab）** | `/dashboard/users/:userId/:tab` | 当前用户管理是内联编辑，无独立的用户详情页（含访问时间、设备、策略分Tab） |
| 🟡 中 | **单个日志文件查看器** | `/dashboard/logs/:file` | 当前只有日志列表，无法在页面内查看单个日志文件内容 |
| 🟢 低 | **Quick Connect** | `/quickconnect` | 用手机/设备码快速登录（Emby/Jellyfin 特性） |
| 🟢 低 | **Trickplay 配置页** | `/dashboard/trickplay` | 管理员配置 trickplay 生成参数 |
| 🟢 低 | **备份管理** | `/dashboard/backups` | 服务器备份创建/恢复 |

### B. 组件/交互级缺失

| 优先级 | 缺失功能 | Jellyfin 对应 | 说明 |
|--------|---------|-------------|------|
| 🔴 高 | **元数据编辑对话框/表单** | `metadataEditor/` | 编辑条目的标题、原始标题、简介(Overview)、年份、类型标签、外部ID、评级、社区评分等。后端已有 `POST /Items/{id}` (update_item) 和 `GET /Items/{id}/MetadataEditor`，前端完全未调用 |
| 🔴 高 | **条目识别（Identify）** | `itemidentifier/` | 通过 TMDB/TVDB 搜索匹配正确的条目，修正元数据错误匹配。后端有 `RemoteSearch/Subtitles` 但缺条目级 Identify |
| 🟡 中 | **多选批量操作** | `multiSelect/` | 在媒体库页面长按/Ctrl+点击多个条目后批量：删除、标记已看、收藏、刷新元数据 |
| 🟡 中 | **合集(Collection)编辑器** | `collectionEditor/` | 创建/管理电影合集（如"漫威系列"），将多个电影归入一个合集 |
| 🟡 中 | **视图切换（网格/列表/海报）** | `viewSettings/` | 媒体库页面的 `libraryViewType` 当前仅筛选 ItemType，**不支持**切换展示布局（海报卡片 vs 横向列表 vs 详细列表） |
| 🟡 中 | **字母索引跳转** | `alphaPicker` | 媒体库右侧或顶部的字母条(A-Z)，点击跳到对应字母开头的条目 |
| 🟡 中 | **Up Next 弹窗** | `upnextdialog/` | 播放器中剧集快结束时弹出"即将播放下一集"倒计时弹窗（当前有 nextUpEpisode 数据但无弹窗 UI） |
| 🟡 中 | **播放全部 / 随机播放** | `PlayArrowIconButton` | 在媒体库/剧集页面一键"播放全部"或"随机播放"整个库/季 |
| 🟡 中 | **NowPlayingBar 增强** | `nowPlayingBar/` | 底部常驻迷你播放条（当前 MiniPlayer 较简单），显示当前播放进度、封面、快进快退、音量 |
| 🟡 中 | **远程控制** | `remotecontrol/` | 从浏览器控制另一台设备上的播放（SyncPlay 的简单形式） |
| 🟢 低 | **媒体文件下载** | `useGetDownload` | 允许用户下载媒体文件到本地（受策略控制 EnableContentDownloading） |
| 🟢 低 | **字幕同步调整** | `subtitlesync/` | 播放时实时调整字幕偏移量（±秒数） |
| 🟢 低 | **媒体信息弹窗** | `itemMediaInfo/` | 展示详细的媒体流技术信息（编码、码率、分辨率等）弹窗 |
| 🟢 低 | **播放器统计/调试面板** | `playerstats/` | 实时显示播放帧率、码率、丢帧等信息 |
| 🟢 低 | **背景/Logo 屏保** | `backdropScreensaver` | 空闲时的背景切换屏保 |
| 🟢 低 | **幻灯片** | `slideshow/` | 照片类条目的幻灯片播放 |

### C. API 层缺失（前端已有但未调用，或前后端都缺）

| 优先级 | 缺失 | 说明 |
|--------|------|------|
| 🔴 高 | **前端未封装 `POST /Items/{id}` (更新条目元数据)** | 后端 `update_item` 已存在，前端 `emby.ts` 中无 `updateItem()` 方法 |
| 🔴 高 | **前端未封装 `GET /Items/{id}/MetadataEditor`** | 后端 `item_metadata_editor` 已存在，前端未调用 |
| 🟡 中 | **条目远程搜索/识别 (RemoteSearch/Item)** | 后端部分存在 (subtitles 搜索)，但条目级 Identify（`/Items/RemoteSearch/*`）可能缺 |
| 🟡 中 | **合集 API** | `POST /Collections`、`POST /Collections/{id}/Items` — 前后端均未实现 |
| 🟡 中 | **首页区块配置 API** | `UserConfiguration.OrderedViews` / `MyMediaExcludes` / `LatestItemsExcludes` 字段已定义但首页未使用 |
| 🟡 中 | **任务触发器更新** | 前端有 `updateScheduledTaskTriggers` 方法，但 ScheduledTasksSettings 页面未使用 |
| 🟢 低 | **Quick Connect API** | 前后端均未实现 |
| 🟢 低 | **下载流 API** | 后端可能已有文件服务，前端缺 UI |

### D. 首页细节差异

| 细节 | Jellyfin | 当前项目 | 差距 |
|------|---------|---------|------|
| 首页区块顺序可配 | ✅ 用户偏好配置 | ❌ 硬编码顺序 | 缺自定义 |
| 库入口可隐藏 | ✅ MyMediaExcludes | ❌ 显示所有库 | 缺用户偏好 |
| 最近添加可排除某库 | ✅ LatestItemsExcludes | ❌ 显示所有 | 缺用户偏好 |
| 首页建议/推荐 | ✅ 多种推荐算法 | ✅ 仅 continueWatching + latest | 基本可用 |

### E. 播放器细节差异

| 细节 | Jellyfin | 当前项目 | 差距 |
|------|---------|---------|------|
| Up Next 倒计时弹窗 | ✅ | ❌ 仅底部按钮 | 缺弹窗 UI |
| 字幕偏移调整 | ✅ subtitlesync | ❌ | 缺功能 |
| 播放速率选择 UI | ✅ | ⚠️ 有 ref 但可能无 UI | 需检查 |
| 播放器统计面板 | ✅ playerstats | ❌ | 缺功能 |
| 画中画切换 | ✅ | ✅ pip ref | 基本可用 |
| 全屏切换 | ✅ | ✅ | 完整 |
| 音量 OSD | ✅ | ⚠️ 基本 | 可能需美化 |

### F. 媒体库浏览页细节差异

| 细节 | Jellyfin | 当前项目 | 差距 |
|------|---------|---------|------|
| 布局切换 (海报/横列/详细列表) | ✅ | ❌ 仅海报网格 | 缺列表/详细视图 |
| 字母索引 A-Z 快速跳转 | ✅ alphaPicker | ❌ | 缺功能 |
| 播放全部/随机播放按钮 | ✅ | ❌ | 缺功能 |
| 多选模式 | ✅ multiSelect | ❌ | 缺功能 |
| 清除所有筛选按钮 | ✅ | ✅ resetLibraryFilters | 完整 |

---

## 三、推荐实施优先级

### 第一批（核心体验提升）
1. **元数据编辑器** — 用户最常用的管理功能，补 `updateItem` API 封装 + 编辑表单
2. **Up Next 弹窗** — 播放体验关键，数据已有只缺 UI
3. **播放全部 / 随机播放** — 库/季页面常用操作
4. **视图切换（网格/列表）** — 提升大库浏览体验

### 第二批（管理功能完善）
5. **Dashboard 管理面板首页** — 服务器状态聚合
6. **条目识别 (Identify)** — 修正元数据匹配错误
7. **字母索引跳转** — 大库快速导航
8. **多选批量操作** — 批量管理效率
9. **任务触发器编辑** — 计划任务灵活配置
10. **单个用户详情页** — 完善用户管理

### 第三批（体验细节打磨）
11. **首页区块自定义** — 用户偏好
12. **合集编辑器** — 电影合集管理
13. **字幕同步调整** — 播放器细节
14. **NowPlayingBar 增强** — 底部播放条
15. **日志文件查看器** — 管理员调试
16. **媒体信息弹窗** — 技术细节展示
