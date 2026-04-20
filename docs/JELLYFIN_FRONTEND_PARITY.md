# Jellyfin 前端复刻清单

目标不是只做一个首页，而是按 Jellyfin 前端模板的页面职责拆分前台、后台和首次启动流程，同时继续保持 Emby API 调用习惯。

## 页面分区

| 分区 | Jellyfin 模板页面 | Movie Rust 当前状态 |
| --- | --- | --- |
| 首次启动 | `pages/wizard.vue`、`components/Wizard/*` | 已实现语言、管理员、元数据、远程访问 4 步 |
| 登录/服务器 | `pages/server/login.vue`、`server/select.vue`、`server/add.vue` | 已实现公开用户选择、手动登录、服务器选择和添加 |
| 前台首页 | `pages/index.vue` | 已实现媒体库、继续观看、收藏、最新媒体 |
| 媒体库浏览 | `pages/library/[itemId].vue` | 已实现媒体库、文件夹、剧集层级浏览 |
| 条目详情 | `pages/item/[itemId].vue`、`series/[itemId].vue` | 已实现独立详情页；后续补人物、相关推荐、季标签细节 |
| 搜索 | `pages/search.vue` | 已实现独立搜索页和按类型分栏结果 |
| 播放 | `pages/playback/video.vue`、`music.vue` | 已实现直链播放；后续补全播放器页、字幕选择、播放队列 |
| 后台首页 | `pages/settings/index.vue` | 已实现设置首页、用户设置区和管理员概览 |
| 账户设置 | `pages/settings/account.vue` | 已实现当前账户信息和密码修改 |
| 服务器设置 | `pages/settings/server.vue` | 已实现名称、语言、元数据、远程访问保存入口 |
| 用户管理 | `pages/settings/users/*` | 已实现用户列表；后续补新建、编辑、权限策略 |
| 设备/API Key | `settings/devices.vue`、`apikeys.vue` | 已实现设备会话列表；API Key 页先以令牌兼容说明为主，后续补独立生成/撤销 |
| 日志与活动 | `settings/logs-and-activity.vue` | 已实现活动流读取；日志文件列表暂为空实现 |
| 字幕设置 | `settings/subtitles.vue` | 已实现客户端字幕样式与预览；后续补下载/语言策略 |

## 后续实现顺序

1. 继续补前台详情细节：人物、相关推荐、系列/季标签、更多媒体信息布局。
2. 引入独立播放器页，对齐 Jellyfin 的视频/音乐播放页面而不是只用直链视频标签。
3. 补后台剩余深层功能：API Key 真正的数据模型、用户新建/编辑、权限策略、日志文件输出。
4. 最后补齐 Jellyfin 风格任务系统、通知、插件、DLNA/网络等高级后台逻辑。
