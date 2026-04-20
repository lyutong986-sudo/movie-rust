# 新功能测试指南

本文档说明如何测试新实现的功能，包括季/集API、图片代理和STRM代理。

## 前提条件

1. 确保后端服务正在运行：
   ```bash
   docker-compose up -d
   ```
2. 确保已登录并获取有效的API密钥（`api_key`）。

## 季/集API测试

### 1. 获取季列表
测试 `/Shows/{seriesId}/Seasons` 端点：
```bash
# 替换 {seriesId} 为实际的剧集ID，{api_key} 为您的API密钥
curl -v "http://localhost:10004/Shows/{seriesId}/Seasons?api_key={api_key}"
```

### 2. 获取剧集列表
测试 `/Shows/{seriesId}/Episodes` 端点：
```bash
curl -v "http://localhost:10004/Shows/{seriesId}/Episodes?api_key={api_key}"
```

### 3. 获取指定季的剧集列表
测试 `/Seasons/{seasonId}/Episodes` 端点：
```bash
curl -v "http://localhost:10004/Seasons/{seasonId}/Episodes?api_key={api_key}"
```

### 查询参数支持：
- `userId`：指定用户ID（可选）
- `fields`：指定返回字段（可选）
- `startIndex`：分页起始索引（默认0）
- `limit`：每页数量（默认100）

## 图片代理测试

### 1. 测试远程图片代理（当image_primary_path为URL时）
首先，确保数据库中有某个条目的`image_primary_path`字段包含远程URL（如`https://example.com/poster.jpg`）。

然后访问图片：
```bash
curl -v "http://localhost:10004/Items/{itemId}/Images/Primary?api_key={api_key}"
```
如果图片路径是URL，后端会代理获取并返回图片。

### 2. 测试直接远程图片代理
使用 `/Images/Remote` 端点直接代理任意远程图片：
```bash
curl -v "http://localhost:10004/Images/Remote?ImageUrl=https://example.com/image.jpg&api_key={api_key}"
```

### 3. 测试本地图片服务
确保数据库中有某个条目的`image_primary_path`字段包含本地文件路径。
```bash
curl -v "http://localhost:10004/Items/{itemId}/Images/Primary?api_key={api_key}"
```

## STRM代理播放测试

### 1. 准备测试环境
1. 在媒体库中创建一个`.strm`文件，内容为远程视频URL（例如：`https://example.com/video.mp4`）
2. 确保该文件被扫描器识别并导入数据库

### 2. 测试STRM代理播放
```bash
# 测试直接播放（会触发代理）
curl -v "http://localhost:10004/Videos/{itemId}/stream?api_key={api_key}"

# 测试带Range头的播放（支持边下边播）
curl -v -H "Range: bytes=0-1048575" "http://localhost:10004/Videos/{itemId}/stream?api_key={api_key}"
```

### 3. 验证代理功能
- 检查响应状态码应为200（或206对于部分内容）
- 检查响应头应包含正确的Content-Type
- 检查响应体应为视频内容

## 验证数据库记录

### 1. 检查季/集数据结构
```sql
-- 连接到数据库
docker-compose exec postgres psql -U movie -d movie_rust

-- 查询系列、季、剧集的关系
SELECT 
    series.id as series_id,
    series.name as series_name,
    season.id as season_id,
    season.name as season_name,
    episode.id as episode_id,
    episode.name as episode_name
FROM media_items series
LEFT JOIN media_items season ON season.parent_id = series.id AND season.item_type = 'Season'
LEFT JOIN media_items episode ON episode.parent_id = season.id AND episode.item_type = 'Episode'
WHERE series.item_type = 'Series'
LIMIT 10;
```

### 2. 检查图片路径
```sql
-- 检查包含URL的图片路径
SELECT id, name, image_primary_path 
FROM media_items 
WHERE image_primary_path LIKE 'http%' 
LIMIT 5;

-- 检查本地图片路径
SELECT id, name, image_primary_path 
FROM media_items 
WHERE image_primary_path IS NOT NULL AND image_primary_path NOT LIKE 'http%'
LIMIT 5;
```

## 日志监控

### 1. 查看后端日志
```bash
docker-compose logs -f movie-rust
```

### 2. 关键日志信息
- STRM代理启动：`代理远程流: {url}`
- 图片代理请求：`代理远程图片: {url}`
- API请求处理：`GET /Shows/{id}/Seasons` 等

## 常见问题排查

### 1. 季/集API返回空列表
- 确认数据库中存在`item_type`为`Series`、`Season`、`Episode`的记录
- 确认父子关系正确（`parent_id`字段）

### 2. 图片代理返回404
- 确认URL可公开访问
- 检查网络连接
- 查看后端日志中的错误信息

### 3. STRM代理播放失败
- 确认.strm文件内容为有效的URL
- 确认URL可公开访问且返回视频内容
- 检查后端日志中的代理错误

## 自动化测试脚本（PowerShell）

创建一个 `test_new_apis.ps1` 脚本：

```powershell
$baseUrl = "http://localhost:10004"
$apiKey = "your_api_key_here"

# 测试季/集API
Write-Host "Testing Season/Episode APIs..."
$seriesId = "your_series_id_here"
$seasonId = "your_season_id_here"

# 获取季列表
$response = Invoke-RestMethod -Uri "$baseUrl/Shows/$seriesId/Seasons?api_key=$apiKey" -Method Get
Write-Host "Seasons API response: $($response | ConvertTo-Json -Depth 2)"

# 获取剧集列表
$response = Invoke-RestMethod -Uri "$baseUrl/Shows/$seriesId/Episodes?api_key=$apiKey" -Method Get
Write-Host "Episodes API response: $($response | ConvertTo-Json -Depth 2)"

# 获取指定季的剧集
$response = Invoke-RestMethod -Uri "$baseUrl/Seasons/$seasonId/Episodes?api_key=$apiKey" -Method Get
Write-Host "Season Episodes API response: $($response | ConvertTo-Json -Depth 2)"

Write-Host "All API tests completed!"
```

注意：在实际运行前，请替换脚本中的 `your_api_key_here`、`your_series_id_here` 和 `your_season_id_here` 为实际值。