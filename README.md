# Movie Rust

这是一个面向 Emby/Jellyfin 客户端兼容的流媒体平台骨架，技术栈是 Rust、Vue 和 PostgreSQL。

当前第一版重点：

- 后端使用 `axum + sqlx + PostgreSQL`。
- API 返回字段使用 Emby/Jellyfin 常见的 PascalCase DTO。
- 支持 `/Users/AuthenticateByName` 登录并返回 `AccessToken`。
- 支持媒体库、媒体条目、图片、直链播放、播放进度上报。
- 参考 Jellyfin 命名解析，支持电影清洗、剧集 `Series/Season/Episode` 层级和外挂字幕轨。
- 支持收藏、已播放/未播放和 `UserData` 更新，便于 Emby/Jellyfin 客户端同步用户状态。
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

默认地址是 `http://127.0.0.1:8096`。首次启动不会自动创建固定管理员，打开前端后按向导创建管理员账户。

4. 启动前端：

```powershell
cd frontend
npm install
npm run dev
```

前端默认地址是 `http://127.0.0.1:5173`。

> 当前环境里 `node.exe` 被系统拒绝执行，前端依赖需要在 Node 可用后安装运行。

## Docker 构建与推送

项目现在以单镜像为主：

- `Dockerfile`：内含 Rust API、Vue 静态资源和 Nginx，推送为 `movie-rust`。
- `backend/Dockerfile` 和 `frontend/Dockerfile` 保留给拆分部署调试使用，默认 compose 和 GitHub Actions 不再使用它们。

1. 本地构建并启动：

```powershell
docker compose up -d --build
```

Docker 入口：`http://127.0.0.1:8096`。Nginx 在同一个容器内代理 Emby/Jellyfin API 到 Rust 后端。

媒体目录默认示例挂载为 `./media:/media:ro`。如果你的影片在其它目录，需要在 `docker-compose.yml` 的 `movie-rust.volumes` 中改成类似：

```yaml
- D:/Movies:/media/movies:ro
```

然后在前端添加媒体库路径时填写容器内路径，例如 `/media/movies`。

2. 推送到 DockerHub：

```powershell
docker login
docker build -f Dockerfile -t yuanhu66/movie-rust:latest .
docker push yuanhu66/movie-rust:latest
```

也可以用 buildx 直接构建并推送：

```powershell
docker buildx create --use --driver docker-container
docker buildx build -f Dockerfile -t yuanhu66/movie-rust:latest --push .
```

单镜像运行时仍然需要 PostgreSQL，例如：

```powershell
docker run -d --name movie-rust `
  -p 8096:80 `
  -e DATABASE_URL=postgres://movie:movie@host.docker.internal:5432/movie_rust `
  -v D:/Movies:/media/movies:ro `
  yuanhu66/movie-rust:latest
```

## 本地播放器连接

你的 Emby 本地播放器可以优先尝试连接：

```text
http://127.0.0.1:8096
```

已经实现的兼容接口见 [API_COMPAT.md](./docs/API_COMPAT.md)。

## 媒体库扫描

前端添加本地路径后点击“扫描”，后端会递归导入 Jellyfin 常见视频格式，例如：

```text
mp4, m4v, mkv, avi, mov, webm, wmv, flv, ts, m2ts, iso, vob, mpg, mpeg, strm, rmvb
```

同目录下的 `poster.jpg`、`folder.jpg`、`cover.jpg`，或与视频同名的 `jpg/png/webp` 文件会作为 Primary 图片。

电影库会清洗文件名中的 `1080p`、`2160p`、`UHD`、`HDR`、`x264`、`x265`、`HEVC`、`DTS` 等发布标记，并提取年份、容器、粗略分辨率和编码。剧集库会识别 `S01E02`、`1x02`、`2024.04.19` 等模式，并生成 `Series -> Season -> Episode` 虚拟目录。

外挂字幕会按 Jellyfin/Emby 的外部字幕轨返回。支持同目录同名或同名前缀字幕，例如：

```text
Movie.Name.2024.mkv
Movie.Name.2024.zh.srt
Movie.Name.2024.en.ass
```

## 后续建议

- 加入 `ffprobe` 元数据提取，补齐真实时长、码率、声道和内封字幕。
- 增加 HLS 转码接口，兼容不能直接播放原始文件的客户端。
- 增加 TMDB/TVDB 元数据刮削和封面下载。
