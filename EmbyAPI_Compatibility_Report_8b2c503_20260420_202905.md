# Emby API 兼容性评估报告

**报告版本**: 1.0  
**生成时间**: 2026-04-20 20:29:05  
**Git提交哈希**: 8b2c503  
**评估对象**: Movie Rust 后端项目  
**对照标准**: Emby Server REST API v4.9.3.0 (OpenAPI v3)  
**评估日期**: 2026-04-20

## 执行摘要

本报告对比了当前 Movie Rust 后端项目实现的 Emby API 接口与官方 Emby Server REST API 规范（版本 4.9.3.0）之间的兼容性差异。评估涵盖了认证、媒体库、播放、用户数据、同步、直播、插件等核心模块。

### 总体兼容性评分
- **接口覆盖率**: ~35% (估算)
- **核心功能完整度**: ~60%
- **客户端兼容性**: 部分支持 (需进一步测试)

### 关键发现
1. **认证流程基本完整**，但缺少 API Key 认证和令牌刷新机制
2. **媒体库管理接口部分实现**，缺少虚拟文件夹查询和库选项管理
3. **播放和流媒体传输接口严重缺失**，不支持转码、字幕流和直播
4. **用户数据接口基本完整**，但缺少部分收藏和播放状态管理
5. **错误处理标准化不足**，自定义错误码与官方不匹配
6. **分页和过滤参数支持有限**，缺少复杂的查询筛选功能

## 1. 已覆盖的 Emby API 端点清单

### 1.1 认证模块 (Authentication)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Users/AuthenticateByName` | POST | 完整 | 支持 JSON 和表单编码，但设备信息应通过头部传递 |
| `/Users/{Id}/Authenticate` | POST | 完整 | 通过用户ID认证 |
| `/Users/Me` | GET | 完整 | 获取当前用户信息 |
| `/Users/Public` | GET | 完整 | 获取公开用户列表 |

**缺失端点**:
- `/Users/AuthenticateWithQuickConnect` - QuickConnect 认证
- `/Users/ForgotPassword` - 密码重置
- `/Users/ForgotPassword/Pin` - 密码重置PIN验证

### 1.2 系统模块 (System)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/System/Info/Public` | GET | 完整 | 公开系统信息 |
| `/System/Info` | GET | 完整 | 详细系统信息 |
| `/System/Endpoint` | GET | 完整 | 端点信息 |
| `/System/Ping` | GET/POST | 完整 | 服务器连通性测试 |
| `/System/Logs` | GET | 部分 | 缺少日志文件下载支持 |
| `/System/ActivityLog/Entries` | GET | 部分 | 缺少完整查询参数 |

### 1.3 用户管理模块 (Users)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Users` | GET | 完整 | 获取用户列表 |
| `/Users/{Id}` | GET | 完整 | 获取用户详情 |
| `/Users/{Id}/Policy` | POST | 完整 | 更新用户策略 (已调整为POST) |
| `/Users/{Id}/Password` | POST | 完整 | 更新用户密码 |
| `/Users/New` | POST | 缺失 | 创建新用户 |
| `/Users/{Id}/Delete` | DELETE | 缺失 | 删除用户 |

### 1.4 媒体库模块 (Library)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Library/VirtualFolders` | GET/POST/DELETE | 完整 | 虚拟文件夹管理 |
| `/Library/VirtualFolders/Name` | POST | 完整 | 重命名虚拟文件夹 |
| `/Library/VirtualFolders/Paths` | POST/DELETE | 完整 | 媒体路径管理 |
| `/Library/VirtualFolders/Paths/Update` | POST | 完整 | 更新媒体路径 |
| `/Library/VirtualFolders/LibraryOptions` | POST | 完整 | 更新库选项 |
| `/Library/MediaFolders` | GET | 完整 | 获取媒体文件夹 |
| `/Library/Refresh` | POST | 缺失 | 刷新媒体库 |
| `/Library/PhysicalPaths` | GET | 缺失 | 获取物理路径 |
| `/Library/SelectableMediaFolders` | GET | 缺失 | 可选择的媒体文件夹 |

### 1.5 媒体项模块 (Items)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Items` | GET | 部分 | 缺少完整查询参数支持 |
| `/Users/{UserId}/Items` | GET | 部分 | 用户专属项查询 |
| `/Users/{UserId}/Items/Latest` | GET | 部分 | 最新项查询 |
| `/Items/{Id}` | GET | 完整 | 获取项详情 |
| `/Items/{Id}/PlaybackInfo` | GET/POST | 部分 | 播放信息查询 |
| `/UserItems/{Id}/UserData` | GET/POST | 完整 | 用户数据管理 |
| `/UserFavoriteItems/{Id}` | POST/DELETE | 完整 | 收藏项管理 |
| `/UserPlayedItems/{Id}` | POST/DELETE | 完整 | 播放状态管理 |
| `/Items/Root` | GET | 完整 | 根项获取 |
| `/Users/{UserId}/Items/Root` | GET | 完整 | 用户根项获取 |

### 1.6 图像模块 (Images)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Items/{Id}/Images` | GET | 部分 | 图像列表 |
| `/Items/{Id}/Images/{Type}` | GET | 完整 | 获取特定图像 |
| `/Images/Remote` | GET | 部分 | 远程图像代理 |

### 1.7 人物模块 (Persons)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Persons` | GET | 部分 | 人物列表，支持分页和过滤 |
| `/Persons/{Id}` | GET | 完整 | 支持名称或UUID查找 |
| `/Persons/{Id}/Items` | GET | 部分 | 人物相关作品 |

### 1.8 剧集模块 (Shows)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Shows/{Id}/Seasons` | GET | 部分 | 系列季列表 |
| `/Shows/{Id}/Episodes` | GET | 部分 | 系列剧集列表 |
| `/Seasons/{Id}/Episodes` | GET | 部分 | 季剧集列表 |

### 1.9 视频流模块 (Videos)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Videos/{Id}/{StreamPath}` | GET | 部分 | 基本视频流，缺少转码支持 |
| `/Items/{Id}/File` | GET | 完整 | 直接文件访问 |
| `/Items/{Id}/Download` | GET | 完整 | 文件下载 |

### 1.10 会话模块 (Sessions)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Sessions` | GET | 部分 | 会话列表 |
| `/Sessions/Playing` | POST | 部分 | 播放开始报告 |
| `/Sessions/Playing/Progress` | POST | 部分 | 播放进度报告 |
| `/Sessions/Playing/Stopped` | POST | 部分 | 播放停止报告 |
| `/Sessions/Capabilities` | POST | 占位符 | 未实现 |
| `/Sessions/Logout` | POST | 占位符 | 未实现 |

### 1.11 启动配置模块 (Startup)
| 端点 | 方法 | 实现状态 | 差异说明 |
|------|------|----------|----------|
| `/Startup/Configuration` | GET/POST | 完整 | 启动配置 |
| `/Startup/User` | GET/POST | 完整 | 初始用户设置 |
| `/Startup/RemoteAccess` | POST | 完整 | 远程访问配置 |
| `/Startup/Complete` | POST | 完整 | 完成启动向导 |

## 2. 端点详细差异分析

### 2.1 认证端点差异

#### `/Users/AuthenticateByName`
| 方面 | 官方规范 | 当前实现 | 差异严重性 |
|------|----------|----------|------------|
| **请求方法** | POST | POST | 无差异 |
| **请求头** | X-Emby-Authorization | 支持多种头部 | 部分差异 |
| **请求体字段** | Username, Pw | Username, Pw, Password, DeviceId, DeviceName, Client | 扩展字段 |
| **响应字段** | User, SessionInfo, AccessToken, ServerId | 完全匹配 | 无差异 |
| **认证方式** | API Key 或 User Token | 仅支持用户密码 | 主要差异 |

**关键差异**:
- 官方要求设备信息通过 `X-Emby-Authorization` 头部传递，当前实现同时支持请求体字段
- 缺少 API Key 认证支持
- 缺少令牌刷新机制

### 2.2 媒体库端点差异

#### `/Library/VirtualFolders`
| 方面 | 官方规范 | 当前实现 | 差异严重性 |
|------|----------|----------|------------|
| **查询参数** | startIndex, limit, 等 | 自定义参数 | 中等差异 |
| **响应结构** | VirtualFolderInfo[] | VirtualFolderInfoDto[] | 字段部分匹配 |
| **POST 请求体** | AddVirtualFolderDto | AddVirtualFolderDto | 结构类似 |

**缺失功能**:
- `/Library/VirtualFolders/Query` - 虚拟文件夹查询
- 完整的库选项管理
- 库刷新和扫描状态报告

### 2.3 播放信息端点差异

#### `/Items/{Id}/PlaybackInfo`
| 方面 | 官方规范 | 当前实现 | 差异严重性 |
|------|----------|----------|------------|
| **查询参数** | userId, maxStreamingBitrate, startTimeTicks, 等 | userId, mediaSourceId | 严重差异 |
| **响应结构** | PlaybackInfoResponse | PlaybackInfoResponse | 字段大量缺失 |
| **转码支持** | 完整的转码配置 | 仅支持直接播放 | 关键差异 |

**关键缺失**:
- 转码配置 (VideoCodec, AudioCodec, MaxStreamingBitrate, 等)
- 直播流支持
- 字幕流集成
- DRM 支持

## 3. 认证流程验证

### 3.1 支持的认证方式
| 认证方式 | 实现状态 | 说明 |
|----------|----------|------|
| **Emby Token** | 完整 | 通过 `X-Emby-Token` 或 `X-MediaBrowser-Token` 头部 |
| **API Key** | 缺失 | 官方支持但未实现 |
| **用户密码** | 完整 | 通过 `/Users/AuthenticateByName` |
| **QuickConnect** | 缺失 | 未实现 |
| **OAuth** | 缺失 | 未实现 |

### 3.2 错误状态码处理
| 状态码 | 官方含义 | 当前实现 | 一致性 |
|--------|----------|----------|--------|
| **401 Unauthorized** | 认证失败 | 正确返回 | 完全一致 |
| **403 Forbidden** | 权限不足 | 正确返回 | 完全一致 |
| **400 BadRequest** | 请求错误 | 正确返回 | 完全一致 |
| **404 NotFound** | 资源不存在 | 正确返回 | 完全一致 |

### 3.3 令牌刷新机制
- **官方机制**: 支持通过现有令牌获取新令牌
- **当前实现**: 无令牌刷新，会话有过期时间但无刷新端点
- **差异**: 关键缺失，影响客户端长时间连接

## 4. 数据格式与编码

### 4.1 时间戳格式
| 字段类型 | 官方格式 | 当前实现 | 一致性 |
|----------|----------|----------|--------|
| **DateTime** | ISO 8601 (UTC) | RFC 3339 (UTC) | 基本一致 |
| **TimeSpan** | Ticks (100-ns间隔) | 未标准化 | 不一致 |
| **DateOnly** | yyyy-MM-dd | 字符串 | 可能不一致 |

### 4.2 GUID 格式
| 使用场景 | 官方格式 | 当前实现 | 一致性 |
|----------|----------|----------|--------|
| **用户ID** | 大写带连字符 | 小写带连字符 | 格式差异 |
| **媒体项ID** | 大写带连字符 | 小写带连字符 | 格式差异 |
| **会话ID** | 大写带连字符 | 小写带连字符 | 格式差异 |

### 4.3 ImageTags 系统
| 功能 | 官方实现 | 当前实现 | 差异 |
|------|----------|----------|------|
| **图像标识** | 哈希字符串 | 未实现 | 完全缺失 |
| **图像URL构造** | `/Items/{Id}/Images/{Type}?tag={ImageTag}` | 无tag参数 | 安全性差异 |

### 4.4 UserData 结构
| 字段 | 官方规范 | 当前实现 | 差异 |
|------|----------|----------|------|
| **PlayCount** | integer | integer | 一致 |
| **IsFavorite** | boolean | boolean | 一致 |
| **PlaybackPositionTicks** | long | 未实现 | 缺失 |
| **LastPlayedDate** | DateTime | 未实现 | 缺失 |
| **Played** | boolean | boolean | 一致 |

## 5. 错误处理

### 5.1 错误码映射
| 错误场景 | 官方错误码 | 当前错误码 | HTTP状态码 | 一致性 |
|----------|------------|------------|------------|--------|
| **用户不存在** | UserNotFound | NotFound | 404 | 基本一致 |
| **认证失败** | AuthenticationFailed | Unauthorized | 401 | 一致 |
| **权限不足** | Forbidden | Forbidden | 403 | 一致 |
| **媒体项不存在** | ItemNotFound | NotFound | 404 | 基本一致 |
| **无效参数** | InvalidArgument | BadRequest | 400 | 基本一致 |

### 5.2 错误响应体格式
**官方格式**:
```json
{
  "ErrorCode": "UserNotFound",
  "ErrorMessage": "User not found",
  "ErrorDetails": "Additional details..."
}
```

**当前实现**:
```json
{
  "Message": "资源不存在: 用户不存在"
}
```

**关键差异**:
- 缺少标准化 `ErrorCode` 字段
- 错误消息未国际化
- 缺少错误详情字段

## 6. 分页与过滤

### 6.1 分页参数支持
| 参数 | 官方规范 | 当前实现 | 支持度 |
|------|----------|----------|--------|
| **StartIndex** | 支持 | 部分支持 | 中等 |
| **Limit** | 支持 | 部分支持 | 中等 |
| **SortBy** | 多种字段 | 有限字段 | 低 |
| **SortOrder** | Ascending/Descending | 支持 | 高 |

### 6.2 过滤参数支持
| 过滤类型 | 官方参数 | 当前实现 | 支持度 |
|----------|----------|----------|--------|
| **媒体类型** | IncludeItemTypes, ExcludeItemTypes | 部分支持 | 低 |
| **内容过滤** | Filters (IsFavorite, IsPlayed, 等) | 未实现 | 无 |
| **日期范围** | MinDate, MaxDate | 未实现 | 无 |
| **评分过滤** | MinCommunityRating, MinCriticRating | 未实现 | 无 |

### 6.3 字段选择
| 功能 | 官方规范 | 当前实现 | 差异 |
|------|----------|----------|------|
| **Fields参数** | 支持字段选择 | 未实现 | 完全缺失 |
| **精简响应** | EnableUserData, EnableImages | 未实现 | 完全缺失 |

## 7. 媒体流传输

### 7.1 流端点支持
| 端点 | 官方功能 | 当前实现 | 差异 |
|------|----------|----------|------|
| `/Videos/{Id}/stream` | 完整转码支持 | 仅直接文件流 | 严重缺失 |
| `/Audio/{Id}/stream` | 音频转码 | 未实现 | 完全缺失 |
| `/Videos/{Id}/master.m3u8` | HLS流 | 未实现 | 完全缺失 |
| `/Videos/{Id}/subtitles/{Index}/stream` | 字幕流 | 未实现 | 完全缺失 |

### 7.2 转码参数支持
| 参数 | 官方支持 | 当前实现 | 差异 |
|------|----------|----------|------|
| **VideoCodec** | H264, HEVC, VP9, 等 | 未支持 | 完全缺失 |
| **AudioCodec** | AAC, MP3, AC3, 等 | 未支持 | 完全缺失 |
| **MaxStreamingBitrate** | 比特率控制 | 未支持 | 完全缺失 |
| **MaxAudioChannels** | 声道控制 | 未支持 | 完全缺失 |
| **TranscodingProtocol** | HLS, Dash | 未支持 | 完全缺失 |

### 7.3 字幕支持
| 功能 | 官方支持 | 当前实现 | 差异 |
|------|----------|----------|------|
| **嵌入式字幕** | 提取和流式传输 | 仅元数据提取 | 部分缺失 |
| **外部字幕** | SRT, VTT, ASS支持 | 未实现 | 完全缺失 |
| **字幕烧录** | 视频中烧录字幕 | 未实现 | 完全缺失 |

## 8. 实时通知与 WebSocket

### 8.1 WebSocket 支持
| 功能 | 官方规范 | 当前实现 | 差异 |
|------|----------|----------|------|
| **WebSocket连接** | `/embywebsocket` | 未实现 | 完全缺失 |
| **消息类型** | 多种事件类型 | 无 | 完全缺失 |
| **心跳机制** | Ping/Pong | 无 | 完全缺失 |

### 8.2 事件类型支持
| 事件类型 | 官方支持 | 当前实现 | 差异 |
|----------|----------|----------|------|
| **LibraryUpdate** | 库更新通知 | 无 | 完全缺失 |
| **UserDataChanged** | 用户数据变更 | 无 | 完全缺失 |
| **SessionsChanged** | 会话变更 | 无 | 完全缺失 |
| **ServerRestarting** | 服务器重启 | 无 | 完全缺失 |

## 9. 客户端兼容性测试

### 9.1 测试方法
由于当前实现缺少完整的转码和流媒体支持，官方客户端仅能进行基本功能测试。

### 9.2 预期兼容性问题
| 客户端 | 登录 | 浏览 | 播放 | 转码 | 字幕 |
|--------|------|------|------|------|------|
| **Emby Web** | ✓ | ✓ | 部分 | ✗ | ✗ |
| **Emby Theater** | ✓ | ✓ | 部分 | ✗ | ✗ |
| **Emby Android** | ✓ | ✓ | 部分 | ✗ | ✗ |
| **Emby iOS** | ✓ | ✓ | 部分 | ✗ | ✗ |

### 9.3 关键失败场景
1. **转码播放**: 客户端请求转码时返回错误
2. **直播流**: 完全不支持
3. **字幕显示**: 无法加载外部字幕
4. **同步播放**: 缺少WebSocket导致状态不同步
5. **下载功能**: 可能工作，但缺少进度报告

## 10. 修复建议清单

### P0 - 关键缺陷 (阻止基本功能)
1. **实现媒体转码支持**
   - 添加 `/Videos/{Id}/stream` 端点的转码参数处理
   - 集成 FFmpeg 进行实时转码
   - 支持 HLS 和 MP4 分段流

2. **完善认证机制**
   - 添加 API Key 认证支持
   - 实现令牌刷新端点
   - 支持 QuickConnect 认证

3. **实现基本 WebSocket 通知**
   - 添加 `/embywebsocket` 端点
   - 支持 LibraryUpdate 和 SessionsChanged 事件
   - 实现心跳机制

### P1 - 重要功能缺失
4. **完整的分页和过滤系统**
   - 实现所有官方查询参数
   - 添加 Fields 参数支持
   - 完善排序和过滤逻辑

5. **错误处理标准化**
   - 统一错误响应格式
   - 添加 ErrorCode 和 ErrorDetails 字段
   - 完善错误码映射

6. **字幕流支持**
   - 实现 `/Videos/{Id}/subtitles/{Index}/stream` 端点
   - 支持外部字幕文件
   - 添加字幕烧录选项

### P2 - 增强功能
7. **直播流支持**
   - 添加直播频道管理
   - 实现直播流端点
   - 支持 EPG 数据

8. **同步功能**
   - 实现媒体项同步端点
   - 添加同步作业管理
   - 支持离线播放

9. **插件系统**
   - 添加插件加载机制
   - 实现插件API端点
   - 支持第三方插件

## 11. 结论

当前 Movie Rust 后端项目实现了 Emby API 的核心子集，能够支持基本的用户认证、媒体库管理和媒体项浏览功能。然而，对于完整的 Emby 客户端兼容性，仍存在以下关键差距：

1. **媒体流传输**: 缺少转码支持是最大的兼容性障碍
2. **实时通信**: WebSocket 通知系统完全缺失
3. **高级功能**: 直播、同步、插件等未实现
4. **细节兼容性**: 错误格式、分页过滤、字段选择等需要完善

**建议行动路线**:
1. 优先实现 P0 级别的转码和 WebSocket 支持
2. 逐步完善 P1 级别的分页过滤和错误处理
3. 最后实现 P2 级别的增强功能

只有在解决了 P0 级别的关键缺陷后，才能实现与官方 Emby 客户端的完全兼容。

---

*报告生成时间: 2026-04-20 20:29:05 UTC*  
*评估基于 Emby Server REST API v4.9.3.0 规范*  
*当前项目提交: 8b2c503*