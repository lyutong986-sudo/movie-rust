# Movie Rust

这是一个面向 Emby/Jellyfin 客户端兼容的流媒体平台骨架，技术栈是 Rust、Vue 和 PostgreSQL。

当前第一版重点：

- 后端使用 `axum + sqlx + PostgreSQL`。
- API 返回字段使用 Emby/Jellyfin 常见的 PascalCase DTO。
- 支持 `/Users/AuthenticateByName` 登录并返回 `AccessToken`。
- 支持媒体库、媒体条目、图片、直链播放、播放进度上报。
- Vue 前端提供登录、添加媒体库、扫描、浏览和直链播放。

## 快速启动

1. 启动 PostgreSQL：

```powershell
docker compose up -d postgres
```

2. 配置后端环境变量：

```powershell
Copy-Item backend\.env.example backend\.env
```

3. 启动后端：

```powershell
cd backend
cargo run
```

默认地址是 `http://127.0.0.1:8096`，默认账号是 `admin`，默认密码是 `admin123`。

4. 启动前端：

```powershell
cd frontend
npm install
npm run dev
```

前端默认地址是 `http://127.0.0.1:5173`。

> 当前环境里 `node.exe` 被系统拒绝执行，前端依赖需要在 Node 可用后安装运行。

## 本地播放器连接

你的 Emby 本地播放器可以优先尝试连接：

```text
http://127.0.0.1:8096
```

已经实现的兼容接口见 [API_COMPAT.md](./docs/API_COMPAT.md)。

## 媒体库扫描

前端添加本地路径后点击“扫描”，后端会递归导入以下视频格式：

```text
mp4, m4v, mkv, avi, mov, webm, wmv, flv, ts, m2ts
```

同目录下的 `poster.jpg`、`folder.jpg`、`cover.jpg`，或与视频同名的 `jpg/png/webp` 文件会作为 Primary 图片。

## 后续建议

- 加入 `ffprobe` 元数据提取，补齐时长、音轨、字幕、分辨率。
- 增加 HLS 转码接口，兼容不能直接播放原始文件的客户端。
- 增加剧集目录解析，支持 Series、Season、Episode 层级。
- 增加 TMDB/TVDB 元数据刮削和封面下载。
