# Emby/Jellyfin API 兼容记录

本项目后端以 Jellyfin 的现代控制器路径和 Emby 的老式 ServiceStack 路径为参考，实现本地播放器最常用的一组接口。

## 系统

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/System/Info/Public` | 登录前服务器信息 |
| GET | `/System/Info` | 登录后服务器信息 |
| GET/POST | `/System/Ping` | 健康检查 |

## 用户与认证

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/Users/Public` | 登录页用户列表 |
| GET | `/Users` | 用户列表 |
| GET | `/Users/{userId}` | 用户详情 |
| GET | `/Users/Me` | 当前用户 |
| POST | `/Users/AuthenticateByName` | 用户名密码认证，返回 `AccessToken` |

支持的 Token 传递方式：

- `X-Emby-Token`
- `X-MediaBrowser-Token`
- `Authorization: MediaBrowser ... Token="..."`
- `Authorization: Emby ... Token="..."`
- 查询参数 `api_key`

## 媒体库与条目

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/Users/{userId}/Views` | 媒体库视图 |
| GET | `/Library/MediaFolders` | 媒体库列表 |
| GET | `/Items/Root` | 根目录 |
| GET | `/Users/{userId}/Items/Root` | 用户根目录 |
| GET | `/Items` | 条目查询 |
| GET | `/Users/{userId}/Items` | 用户条目查询 |
| GET | `/Users/{userId}/Items/Latest` | 最新条目 |
| GET | `/Items/{itemId}` | 条目详情 |
| GET | `/Users/{userId}/Items/{itemId}` | 用户条目详情 |
| GET/POST | `/Items/{itemId}/PlaybackInfo` | 播放信息 |

已支持常见查询参数：

- `ParentId`
- `IncludeItemTypes`
- `Recursive`
- `SearchTerm`
- `SortBy`
- `SortOrder`
- `StartIndex`
- `Limit`

## 图片与播放

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/Items/{itemId}/Images` | 图片列表 |
| GET/HEAD | `/Items/{itemId}/Images/{imageType}` | 图片文件 |
| GET/HEAD | `/Items/{itemId}/Images/{imageType}/{imageIndex}` | 图片文件 |
| GET/HEAD | `/Videos/{itemId}/stream` | 原文件直链播放 |
| GET/HEAD | `/Videos/{itemId}/stream.{container}` | 带容器扩展名的直链播放 |
| GET/HEAD | `/Items/{itemId}/File` | 原文件 |
| GET/HEAD | `/Items/{itemId}/Download` | 下载原文件 |

当前播放接口是 Direct Play / Direct Stream，暂未实现转码。

## 播放进度

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `/Sessions/Playing` | 播放开始 |
| POST | `/Sessions/Playing/Progress` | 播放进度 |
| POST | `/Sessions/Playing/Stopped` | 播放停止 |
| POST | `/PlayingItems/{itemId}` | 旧版播放开始 |
| POST | `/PlayingItems/{itemId}/Progress` | 旧版播放进度 |
| DELETE | `/PlayingItems/{itemId}` | 旧版播放停止 |
| POST | `/Users/{userId}/PlayingItems/{itemId}` | 旧版用户播放开始 |
| POST | `/Users/{userId}/PlayingItems/{itemId}/Progress` | 旧版用户播放进度 |
| DELETE | `/Users/{userId}/PlayingItems/{itemId}` | 旧版用户播放停止 |

## 管理接口

这些接口用于 Vue 前端，不属于 Emby 原生接口：

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/admin/libraries` | 管理端媒体库列表 |
| POST | `/api/admin/libraries` | 新建媒体库 |
| POST | `/api/admin/scan` | 扫描全部媒体库 |
