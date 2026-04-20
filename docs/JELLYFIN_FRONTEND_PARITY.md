# Jellyfin 前端复刻清单

目标不是只做一个首页，而是按 Jellyfin 前端模板的页面职责拆分前台、后台和首次启动流程，同时继续保持 Emby API 调用习惯。

## 页面分区

| 分区 | Jellyfin 模板页面 | Movie Rust 当前状态 |
| --- | --- | --- |
| 首次启动 | `pages/wizard.vue`、`components/Wizard/*` | 已实现语言、管理员、元数据、远程访问 4 步 |
| 登录/服务器 | `pages/server/login.vue`、`server/select.vue`、`server/add.vue` | 已实现公开用户选择和手动登录；单服务器模式 |
| 前台首页 | `pages/index.vue` | 已实现媒体库、继续观看、收藏、最新媒体 |
| 媒体库浏览 | `pages/library/[itemId].vue` | 已实现媒体库、文件夹、剧集层级浏览 |
| 条目详情 | `pages/item/[itemId].vue`、`series/[itemId].vue` | 已实现详情弹窗；后续拆成独立详情页 |
| 搜索 | `pages/search.vue` | 已实现全局搜索输入；后续拆成搜索页和结果筛选 |
| 播放 | `pages/playback/video.vue`、`music.vue` | 已实现直链播放；后续补全播放器页、字幕选择、播放队列 |
| 后台首页 | `pages/settings/index.vue` | 已实现后台控制台入口和概览 |
| 服务器设置 | `pages/settings/server.vue` | 已实现名称、语言、元数据、远程访问保存入口 |
| 用户管理 | `pages/settings/users/*` | 已实现用户列表；后续补新建、编辑、权限策略 |
| 设备/API Key | `settings/devices.vue`、`apikeys.vue` | 待实现，需要后端设备/API Key 数据模型 |
| 日志与活动 | `settings/logs-and-activity.vue` | 待实现，需要后端任务、日志和活动 API |
| 字幕设置 | `settings/subtitles.vue` | 待实现，需要字幕默认语言、样式和下载配置 |

## 后续实现顺序

1. 引入 Vue Router，把当前内部 `appPage/adminPage` 状态拆成真实路由。
2. 先复刻后台设置页：服务器、媒体库、用户、设备、API Key、日志活动、字幕。
3. 再复刻前台详情/播放链路：独立详情页、播放器页、字幕/音轨选择、播放进度恢复。
4. 最后补齐 Jellyfin 风格任务系统、通知、日志、插件、DLNA/网络等高级后台逻辑。
