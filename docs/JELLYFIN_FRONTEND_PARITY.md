# Jellyfin 前端复刻清单

目标不是只做一个首页，而是按 Jellyfin 前端模板的页面职责拆分前台、后台和首次启动流程，同时继续保持 Emby API 的调用习惯。

## 页面分区

| 分区 | Jellyfin 模板页面 | Movie Rust 当前状态 |
| --- | --- | --- |
| 首次启动 | `pages/wizard.vue` | 已实现语言、管理员、元数据、远程访问 4 步向导 |
| 登录/服务器 | `pages/server/login.vue`、`server/select.vue`、`server/add.vue` | 已实现公开用户选择、手动登录、服务器选择和添加 |
| 前台首页 | `pages/index.vue` | 已实现媒体库、继续观看、收藏、最近添加、多媒体库分区 |
| 媒体库浏览 | `pages/library/[itemId].vue` | 已实现媒体库、文件夹、季/集层级浏览 |
| 条目详情 | `pages/item/[itemId].vue`、`series/[itemId].vue` | 已实现独立详情页和剧集页，支持季标签、分集列表、类型跳转 |
| 搜索 | `pages/search.vue` | 已实现独立搜索页和按类型分栏结果 |
| 播放 | `pages/playback/video.vue`、`music.vue` | 已实现独立视频/音乐播放页，接入播放会话、进度汇报和关闭回写 |
| 类型页 | `pages/genre/[itemId].vue` | 已实现按类型名称筛选的独立类型页 |
| 后台首页 | `pages/settings/index.vue` | 已实现设置首页、用户设置区和管理员概览 |
| 账户设置 | `pages/settings/account.vue` | 已实现当前账户信息和密码修改 |
| 服务器设置 | `pages/settings/server.vue` | 已实现名称、语言、元数据、远程访问保存入口 |
| 用户管理 | `pages/settings/users/*` | 已实现用户列表；新建、编辑、权限策略仍待补齐 |
| 设备/API Key | `settings/devices.vue`、`apikeys.vue` | 已实现设备会话列表；API Key 仍是兼容占位页 |
| 日志与活动 | `settings/logs-and-activity.vue` | 已实现活动流读取；日志文件列表暂为空实现 |
| 字幕设置 | `settings/subtitles.vue` | 已实现客户端字幕样式与预览 |

## 当前缺口

1. `person/[itemId].vue` 还不能 1:1 落地：当前扫描和数据库模型还没有持久化演职员、人名别名、出生地等人物元数据。
2. `musicalbum/[itemId].vue` 还未拆成单独页面，目前音乐条目优先进入独立音乐播放页。
3. 后台用户页缺少 Jellyfin 那套新建用户、细粒度权限、头像和策略表单。
4. API Key、日志文件、通知、插件、DLNA/网络高级配置还只是第一层兼容。

## 下一步顺序

1. 补人物元数据模型和扫描流程，把 `person/[itemId].vue` 与演员/导演跳转真正接上。
2. 拆 `musicalbum/[itemId].vue` 和更完整的音乐库浏览。
3. 继续补后台用户编辑、权限策略、API Key 真模型、日志文件输出。
4. 收尾 Jellyfin 风格的活动、通知、插件、网络高级设置。
