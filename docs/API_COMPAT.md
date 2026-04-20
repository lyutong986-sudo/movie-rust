# Emby/Jellyfin API 兼容记录

本项目后端以 Jellyfin 的现代控制器路径和 Emby 的老式 ServiceStack 路径为参考，实现本地播放器最常用的一组接口。

所有兼容接口同时支持根路径、`/emby` 前缀和 `/mediabrowser` 前缀。例如 `/System/Info/Public`、`/emby/system/info/public`、`/mediabrowser/System/Info/Public` 都会进入同一套路由。

## 系统

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/System/Info/Public` | 登录前服务器信息 |
| GET | `/System/Info` | 登录后服务器信息 |
| GET/POST | `/System/Ping` | 健康检查 |

已补 Emby 客户端常用小写探测路径：`/system/info/public`、`/system/info`、`/system/ping`。

## 用户与认证

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/Users/Public` | 登录页用户列表 |
| GET | `/Users` | 用户列表 |
| GET | `/Users/{userId}` | 用户详情 |
| GET | `/Users/Me` | 当前用户 |
| POST | `/Users/AuthenticateByName` | 用户名密码认证，返回 `AccessToken` |
| POST | `/Users/{userId}/Authenticate` | 用户 ID + 密码认证，返回 `AccessToken` |

登录接口同时支持 `/Users/AuthenticateByName`、`/Users/authenticatebyname`、`/users/authenticatebyname`，请求正文支持 `application/json` 和 `application/x-www-form-urlencoded`。密码字段优先读取 `Pw`，兼容读取 `Password`。

支持的 Token 传递方式：

- `X-Emby-Token`
- `X-MediaBrowser-Token`
- `X-Emby-Authorization: MediaBrowser ... Token="..."`
- `Authorization: MediaBrowser ... Token="..."`
- `Authorization: Emby ... Token="..."`
- 查询参数 `api_key`

## 首次启动向导

这些路径参考 Jellyfin 的 Startup API，用于首次启动时创建管理员并保存初始偏好。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET/POST | `/Startup/Configuration` | 读取/保存服务器名、界面语言和元数据语言 |
| GET/POST | `/Startup/User` | 读取首个用户/创建首个管理员 |
| GET | `/Startup/FirstUser` | 首个用户兼容路径 |
| POST | `/Startup/RemoteAccess` | 保存远程访问和自动端口映射偏好 |
| POST | `/Startup/Complete` | 标记首次启动向导完成 |

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
| GET/POST | `/UserItems/{itemId}/UserData` | 当前用户条目状态 |
| GET/POST | `/Users/{userId}/Items/{itemId}/UserData` | 旧版用户条目状态 |
| POST/DELETE | `/UserFavoriteItems/{itemId}` | 收藏/取消收藏 |
| POST/DELETE | `/Users/{userId}/FavoriteItems/{itemId}` | 旧版收藏/取消收藏 |
| POST/DELETE | `/UserPlayedItems/{itemId}` | 标记已播放/未播放 |
| POST/DELETE | `/Users/{userId}/PlayedItems/{itemId}` | 旧版标记已播放/未播放 |
| GET | `/Shows/{seriesId}/Seasons` | 获取剧集的季列表 |
| GET | `/Shows/{seriesId}/Episodes` | 获取剧集的剧集列表 |
| GET | `/Seasons/{seasonId}/Episodes` | 获取指定季的剧集列表 |

已支持常见查询参数：

- `ParentId`
- `IncludeItemTypes`
- `Recursive`
- `SearchTerm`
- `SortBy`
- `SortOrder`
- `StartIndex`
- `Limit`
- `UserId` (季/集查询)

## 图片与播放

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/Items/{itemId}/Images` | 图片列表 |
| GET/HEAD | `/Items/{itemId}/Images/{imageType}` | 图片文件（支持本地文件路径和远程URL代理） |
| GET/HEAD | `/Items/{itemId}/Images/{imageType}/{imageIndex}` | 图片文件 |
| GET | `/Images/Remote` | 远程图片代理（ImageUrl参数） |
| GET/HEAD | `/Videos/{itemId}/stream` | 原文件直链播放（支持.strm文件代理） |
| GET/HEAD | `/Videos/{itemId}/stream.{container}` | 带容器扩展名的直链播放 |
| GET/HEAD | `/Videos/{itemId}/{mediaSourceId}/Subtitles/{index}/Stream.{format}` | 外挂字幕直链 |
| GET/HEAD | `/Videos/{itemId}/{mediaSourceId}/Subtitles/{index}/{startPositionTicks}/Stream.{format}` | Jellyfin 字幕流兼容路径 |
| GET/HEAD | `/Items/{itemId}/File` | 原文件 |
| GET/HEAD | `/Items/{itemId}/Download` | 下载原文件 |

当前播放接口是 Direct Play / Direct Stream，暂未实现转码。

### 代理功能说明
- **STRM文件代理**：当请求播放.strm文件时，后端会读取文件中的URL（第一行），并通过代理方式获取远程流媒体内容，支持Range头传递，实现边下边播。
- **图片代理**：图片路径支持本地文件路径和远程URL。当`image_primary_path`或`backdrop_path`字段包含`http://`或`https://`开头的URL时，后端会代理获取远程图片并返回，支持缓存控制。
- **远程图片代理**：`/Images/Remote`端点可用于直接代理任意远程图片，需要提供`ImageUrl`查询参数。

`PlaybackInfo` 和条目详情会返回 Jellyfin/Emby 常见的 `MediaSources`、`MediaStreams`、`DirectStreamUrl`、`DefaultAudioStreamIndex`、`ETag`、`Size` 等字段。当前媒体流信息来自文件名和外挂字幕推断，后续接入 `ffprobe` 后可以补齐真实码率、时长、声道和内封字幕。

## 命名解析与剧集

扫描器参考 Jellyfin 的命名规则，已支持：

- 常见视频扩展名：`mp4`、`mkv`、`m4v`、`avi`、`mov`、`webm`、`wmv`、`flv`、`ts`、`m2ts`、`iso`、`vob`、`mpg`、`mpeg`、`strm`、`rmvb` 等。
- 电影标题清洗：移除常见发布组质量/编码标记，如 `1080p`、`2160p`、`UHD`、`HDR`、`x264`、`x265`、`HEVC`、`DTS`、`AC3` 等，并提取年份。
- 剧集识别：`S01E02`、`S01E02E03`、`1x02`、`2024.04.19` 这类文件名会导入为 `Series -> Season -> Episode` 层级。
- 外挂字幕：同目录同名或同名前缀的 `srt`、`ass`、`ssa`、`vtt`、`sub`、`smi`、`sup` 等会作为外部字幕轨返回。

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

## 其它兼容接口

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/System/Endpoint` | 客户端网络位置探测 |
| GET | `/Branding/Configuration` | 登录页品牌配置 |
| GET | `/Branding/Css` | 自定义 CSS |
| GET | `/Branding/Css.css` | 自定义 CSS 兼容路径 |

## 管理接口

这些接口用于 Vue 前端，不属于 Emby 原生接口：

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/admin/libraries` | 管理端媒体库列表 |
| POST | `/api/admin/libraries` | 新建媒体库 |
| POST | `/api/admin/scan` | 扫描全部媒体库 |
