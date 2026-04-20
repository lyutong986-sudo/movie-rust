# 日志目录结构与问题复现指南

## 日志目录结构

```
logs/
├── docker-compose.log          # Docker 容器标准输出（由 docker-compose logs 生成）
├── network-capture.pcap        # 网络抓包文件（需手动抓取）
├── client-error.log            # 客户端错误日志（需前端注入错误捕获）
└── app/                        # 应用日志（如果挂载了 /config/log）
    ├── jellyfin.log            # Jellyfin 风格日志
    └── access.log              # 访问日志
```

## 问题复现步骤

### 封面 404 问题复现

1. **触发条件**：访问 `/Items/{itemId}/Images/Primary` 返回 404
2. **检查点**：
   - 确认媒体文件是否包含封面图片（如 `folder.jpg`、`poster.jpg`）
   - 检查数据库 `media_items` 表的 `image_primary_path` 字段是否为空
   - 验证图片文件是否可读（权限问题）
3. **调试命令**：
   ```bash
   # 检查数据库记录
   docker-compose exec postgres psql -U movie -d movie_rust -c "SELECT id, path, image_primary_path FROM media_items WHERE id='<itemId>';"

   # 手动访问图片 URL（替换实际参数）
   curl -v "http://localhost:10004/Items/<itemId>/Images/Primary"
   ```

### NFO 解析失败复现

1. **触发条件**：电视剧/电影元数据缺失，无法识别季/集结构
2. **检查点**：
   - 确认 NFO 文件是否存在（与视频文件同名，扩展名 `.nfo`）
   - 检查配置 `EnableNfoMetadata: true`（当前项目暂不支持 NFO，需实现）
   - 验证 NFO 文件格式是否符合 Jellyfin 规范
3. **调试命令**：
   ```bash
   # 遍历 metadata 目录
   find /config/metadata -name "*.nfo" -exec grep -l "<title>\|<plot>\|<thumb>" {} \;

   # 手动解析 NFO 文件
   cat "path/to/video.nfo" | grep -E "<title>|<plot>|<thumb>" | head -5
   ```

### 季/集结构缺失复现

1. **触发条件**：电视剧只显示封面，无法展开季和集列表
2. **检查点**：
   - 前端是否调用 `/Shows/{seriesId}/Seasons` 和 `/Seasons/{seasonId}/Episodes` 接口
   - 后端是否识别剧集文件命名规范（如 `S01E01.mkv`）
   - 数据库 `media_items` 表中 `video_type` 字段是否为 `Episode`
3. **调试命令**：
   ```bash
   # 检查剧集识别
   docker-compose exec postgres psql -U movie -d movie_rust -c "SELECT id, path, video_type FROM media_items WHERE parent_id IS NOT NULL;"

   # 测试季接口
   curl "http://localhost:10004/Shows/<seriesId>/Seasons?api_key=<token>"

   # 测试集接口
   curl "http://localhost:10004/Seasons/<seasonId>/Episodes?api_key=<token>"
   ```

## 自动诊断脚本

运行 `make diagnose` 自动收集以下信息：

1. **容器日志**：`docker-compose logs`
2. **网络抓包**：（需手动配置）使用 tcpdump 抓取 8096/8920 端口流量
3. **数据库导出**：导出 `movie_rust` 数据库全量数据
4. **客户端错误日志**：收集前端注入的错误日志
5. **打包归档**：生成 `diagnose-{timestamp}.tar.gz`

## 性能基线要求

- **封面加载时间**：≤ 300 ms（首次加载）
- **电视剧展开时间**：≤ 800 ms（全部季/集）
- **CPU 占用增加**：≤ 5%
- **内存占用**：≤ 100 MB 增量

## 单元测试覆盖

针对 `PlaybackInfoController` 的测试用例需覆盖：

1. STRM 文件重定向（302 到真实 URL）
2. MKV 文件直接流传输（206 Partial Content）
3. HLS 流处理（m3u8 播放列表）
4. 认证令牌验证
5. 媒体源切换（多版本处理）

所有测试需通过 CI，通过率 100%。

## 常见问题快速修复

### STRM 文件直接下载
**问题**：点击播放 .strm 文件时直接下载文件而非播放流
**修复**：已提交补丁 `backend/src/routes/videos.rs`，检测 .strm 扩展名并重定向到文件内容中的 URL

### 封面 404
**问题**：图片路径未正确扫描到数据库
**修复**：执行重新扫描命令或手动更新数据库：
```sql
UPDATE media_items SET image_primary_path = '/media/poster.jpg' WHERE image_primary_path IS NULL AND path LIKE '%.jpg';
```

### 季/集无法展开
**问题**：前端未调用季/集专用接口
**修复**：在前端添加以下 API 调用：
```javascript
// Vue 示例
async function loadSeasons(seriesId) {
  const response = await api.get(`/Shows/${seriesId}/Seasons`);
  return response.data;
}
```