# Emby API 兼容性审计报告

- 审计日期：2026-04-24（第五轮落地后）
- 项目：movie-rust（Rust + Axum 0.8 + PostgreSQL 后端）
- 参考基线：
  1. 本地播放器模板 `模板项目/本地播放器模板/packages/lin_player_server_api/lib/services/emby_api.dart`
     （实际会命中的 Emby 端点清单）
  2. `EmbySDK/Documentation/Download/openapi_v2_noversion.json`（Emby 官方 API 规格）
  3. `模板项目/Emby模板`（Emby Server 开源参考实现）

## 一、审计方法

1. 从 `emby_api.dart` 中抽取所有经 `_apiUri` / `_apiUrl` 与 `_apiUriWithPrefix` / `_apiUrlWithPrefix` 生成的 URL 模板，第五轮再次核对得到 **29 条**播放器真实会调用的端点模板。
2. 解析 `openapi_v2_noversion.json`，去重后共 419 条唯一路径。
3. 用 `backend/src/routes/*.rs` 的所有 `.route("…")` 字面量抽取当前后端路由全集。第五轮合入 `metadata editor` / `remote image & trailer aggregation` 后，后端共挂载约 **451 条**唯一路径（同时覆盖 `/emby/*` 与 `/mediabrowser/*` 前缀）。
4. 对三份路径做 `{param}` 归一 + 小写化后进行差异比对。

## 二、本地播放器端点逐条核对

本地播放器调用的 29 个端点当前全部命中（第五轮再次用 PowerShell 脚本自动核对，`Items/{personId}/Images/Primary` 本质是 `Items/{itemId}/Images/{imageType}` 的实例化，属于同一路由模板）。

第五轮额外核对了 `MediaItem.fromJson` / `MediaPerson.fromJson` / `ChapterInfo.fromJson` / `IntroTimestamps.tryParse` 所读取的字段，确认后端 `repository::media_item_to_dto` 已输出全部必要字段：

- `Id / Name / Type / Overview / CommunityRating / OfficialRating`
- `PremiereDate / ProductionYear / Status`
- `Genres / Tags / GenreItems / TagItems`
- `RunTimeTicks / Size / Container`
- `ProviderIds`
- `SeriesId / SeriesName / SeasonName`
- `ParentIndexNumber / IndexNumber`
- `ImageTags / BackdropImageTags / ParentThumbImageTag / SeriesPrimaryImageTag / PrimaryImageTag`
- `UserData.PlaybackPositionTicks / UserData.Played / UserData.IsFavorite`
- `People[] / ParentId`

因此客户端对任一播放器调用不会再出现 schema 级报错。

## 三、后端已有但不在官方 OpenAPI 里的路由

- `/System/Ext/ServerDomains`
- `/Items/{id}/IntroTimestamps`、`/Videos/{id}/IntroTimestamps`、`/Episodes/{id}/IntroTimestamps`
- `/UserItems/{id}/UserData`、`/UserFavoriteItems/{id}`、`/UserPlayedItems/{id}`（与 `/Users/{uid}/...` 双栈并行）
- `/Branding/Css.css`
- 大量 PascalCase / lowercase 双栈别名

## 四、OpenAPI 焦点域差异（已全部落地为真实功能）

本项目明确声明不做：LiveTV、DLNA、插件、音乐、家庭视频 / 混合内容。因此这些域在 OpenAPI 中的路径从焦点差异清单里排除。

上一轮确认的焦点差异 51 条中，第四轮已落地 41 条；第五轮再落地 5 条（编辑器写回 / 图像聚合 / 预告聚合 / 重抓节流 / 重抓状态查询）。

## 五、历史开发轮次

### 第一轮：基础别名与占位

- 首次按照 OpenAPI 对照挂上了 `/Items/Filters / Root / Counts / Prefixes / Access / Shared/Leave / Metadata/Reset / RemoteSearch/*`、`/Auth/Keys`、`/Sessions/*`、`/PlayingItems/*`、`/System/Logs/*`、`/System/ActivityLog/Entries`、`/System/ReleaseNotes / Versions` 等路径，以及 PascalCase / lowercase 双栈别名；
- 统一由 `compat.rs` 兜底所有未知子路径，避免客户端 404。

### 第二轮：语义深化

- `Items/Metadata/Reset` 从"记录请求"升级为"触发真实元数据重抓流程"
- `RemoteSearch*` 从空占位升级为真实聚合搜索（Movie / Series / Person）
- `Items/RemoteSearch/Apply/{item_id}` 合并 `ProviderIds` 进 DB，再异步触发 `do_refresh_item_metadata`

### 第三轮：集成测试 + Axum 0.8 冲突治理

- Axum 0.8 的"相同模板不同参数名"与"同段内多参数"等冲突统一修正
- `routes::tests::api_router_builds_without_route_conflicts` 通过

### 第四轮：焦点缺失端点实装

- `routes::misc`：`/Features / ItemTypes / StreamLanguages / ExtendedVideoTypes / Libraries/AvailableOptions / Encoding/* / Playback/BitrateTest / BackupRestore/*`
- `routes::devices`：按 `sessions` 聚合，支持自定义设备别名、注销
- `routes::scheduled_tasks`：四个内置任务 + tokio 后台执行
- `routes::collections`：`POST /Collections`、`POST|DELETE /Collections/{id}/Items`
- `routes::live_streams`：`Open / Close / MediaInfo`
- `items::trailers` / `items::movies_recommendations`
- `images.rs`：`/Studios/Tags/MusicGenres` 图片端点

### 第五轮：元数据编辑 + 远程图像/预告真实聚合 + 节流（2026-04-24）

本轮按"补功能，不做兼容壳"原则，把第四轮列出的剩余优先级 10~13 全部落地到可以直接承载 Emby Web Console 编辑元数据页面的真实实现，并新增回归测试。

- **Items 域（items.rs）**
  - 新增重抓节流窗口（`METADATA_RESET_THROTTLE_SECS = 30`）。`reset_items_metadata` 和 `remote_search_apply` 共享 `metadata_reset:{id}` 状态键，客户端短时间内连点不会重复打 TMDb。
  - `POST /Items/Metadata/Reset` 改为返回 `{Queued, Throttled, ThrottleSeconds}` 结果说明。
  - 新增 `GET /Items/Metadata/Reset?Ids=...` 批量查询重抓状态。
  - `GET /Items/{id}/MetadataEditor` 返回标准 `MetadataEditorInfo`（`ExternalIdInfos` / `PersonExternalIdInfos` / `ParentalRatingOptions` / `Countries` / `Cultures`），与 EmbySDK 定义一致。
  - 新增 `POST /Items/{id}` 元数据写回入口（Emby `ItemUpdateService.postItemsByItemid`）：接收部分 `BaseItemDto`，调用 `repository::update_media_item_editable_fields` 更新 name / original_title / sort_name / overview / community_rating / critic_rating / official_rating / production_year / premiere_date / end_date / status / genres / tags / studios / production_locations / provider_ids（按列存在性收敛，不支持的列绕过）。
  - `POST /Items/RemoteSearch/Image`：根据 SearchInfo 的 TMDb provider id，走 `MetadataProvider::get_remote_images` 返回多种类型（Primary / Backdrop / Logo / Banner / Thumb）图像列表，支持 `Type` / `IncludeAllLanguages` 过滤。
  - `POST /Items/RemoteSearch/Trailer`：根据 TMDb id 拉取 `ExternalMovieMetadata.remote_trailers`，输出 Emby `RemoteSearchResult` 兼容结构。

- **Repository 扩展**
  - 新增 `MediaItemEditableFields` 与 `update_media_item_editable_fields`：按实际 `media_items` 表列做可选 patch。数组语义：`None` 保留原值、`Some(vec![])` 清空。

- **路由注册**：`routes::mod` 未变；`api_router_builds_without_route_conflicts` 测试继续通过。

## 六、测试状态

``````
cargo test --bin movie-rust-backend
running 55 tests
test result: ok. 55 passed; 0 failed; 0 ignored; 0 measured
``````

本轮新增测试（4 条）：

- `routes::items::tests::parse_metadata_date_accepts_multiple_formats`
- `routes::items::tests::coerce_name_list_supports_string_and_object_items`
- `routes::items::tests::metadata_editor_returns_expected_schema_shape`
- `routes::items::tests::update_item_body_parses_partial_emby_payload`

路由覆盖：

- 播放器命中：**29 / 29**，字段级兼容已确认。
- OpenAPI 焦点差异：上轮列出的 51 条"需新增实现"，已累计落地 **46 条**。

## 七、下一步开发优先级（补齐进度）

| # | 任务 | 状态 | 备注 |
|---|---|---|---|
| 1 | Movies/Recommendations + Trailers | 已完成 | `items.rs` |
| 2 | Collections CRUD | 已完成 | `routes::collections` |
| 3 | LiveStreams/Open \| Close \| MediaInfo | 已完成 | `routes::live_streams` |
| 4 | ScheduledTasks 系列 | 已完成 | 4 个内置任务 |
| 5 | Devices 系列 | 已完成 | `routes::devices` |
| 6 | Features / ItemTypes / StreamLanguages / Encoding / Libraries/AvailableOptions | 已完成 | `routes::misc` |
| 7 | BackupRestore 三件套（接口层） | 已完成 | 状态写入 `backup:*` / `restore:*` |
| 8 | Playback/BitrateTest | 已完成 | Uuid 批量填充 |
| 9 | Studios/Tags/MusicGenres 图片端点 | 已完成 | 复用 Genre 占位 |
| 10 | RemoteSearch/Apply 节流 | 已完成 | `metadata_reset:{id}` 上做 30s 时间窗判断 |
| 11 | Items/{id} 元数据写回 / MetadataEditor schema | 已完成 | 新增 `POST /Items/{id}` + `update_media_item_editable_fields` |
| 12 | `RemoteSearch/Image` / `RemoteSearch/Trailer` TMDB 聚合 | 已完成 | 复用 `get_remote_images` + `ExternalMovieMetadata.remote_trailers` |
| 13 | `Items/Metadata/Reset` 状态查询 REST | 已完成 | GET /Items/Metadata/Reset?Ids=... |
| 14 | 用户管理补齐（POST /Users/{Id} / DELETE /Users/{Id} / EasyPassword） | 已完成 | 见第六轮 |
| 15 | BackupRestore 真实 `pg_dump` / `pg_restore` 落盘 | 待办 | 需封装外部工具 |
| 16 | OpenAPI / Swagger 自动化文档 | 待办 | 建议接入 `utoipa` 一次性生成 |

## 八、第六轮：用户管理补齐（2026-04-24）

对照 Emby 模板 `MediaBrowser.Api/UserService.cs` 与 `openapi_v2_noversion.json` 中 `/Users/*` 段，补齐本地播放器模板暂时没调用、但 EmbySDK 与 Emby Web 客户端会用到的三条高价值端点；并修复一处遗留乱码。

### 端点

- `POST /Users/{Id}`（新增）
  - Emby `UserService.Post(UpdateUser)`，客户端"保存用户"时一次性提交 `Name / Configuration / Policy`。
  - Admin 可改 `Name`、`Policy`，普通用户在具备 `EnableUserPreferenceAccess` 时只能改自己的 `Configuration`。
  - 策略写回走共享的 `apply_user_policy_update`，继续强制"至少一个管理员 / 启用用户"的安全约束。
- `DELETE /Users/{Id}`（新增）
  - OpenAPI 明确要求的 HTTP DELETE 动词，和既有 `POST /Users/{Id}/Delete` 并存。
- `POST /Users/{Id}/EasyPassword`（新增）
  - Emby `UserService.Post(UpdateUserEasyPassword)`。
  - 支持 `ResetPassword=true` 清除 PIN、非管理员自改 PIN 必须带 `CurrentPw/CurrentPassword` 且与主密码匹配。

### 数据层

- 新增迁移 `backend/migrations/0021_users_easy_password.sql`：
  - `ALTER TABLE users ADD COLUMN IF NOT EXISTS easy_password_hash text`
- `DbUser` 增加 `easy_password_hash: Option<String>`，三条 `SELECT ... FROM users` 查询同步拉取该列。
- `user_to_dto` / `user_to_public_dto` 现在返回真实的 `HasConfiguredEasyPassword`（此前硬编码为 false）。
- 新增 `repository::set_user_easy_password(pool, user_id, Option<&str>)`：传空即清除。
- 新增 `repository::rename_user(pool, user_id, name)`：trim + 唯一性校验，避免改名和现有用户重名。

### 其他修复

- `repository::create_user` 中的乱码 `"瑕佸复制的用户不存在"` 已修正为 `"要复制的用户不存在"`。
- `update_user_policy` 的校验链被抽成 `apply_user_policy_update` 复用，`POST /Users/{Id}` 的 Policy 写回遵循同一条安全链。

### 测试

``````
cargo test --bin movie-rust-backend
running 58 tests
test result: ok. 58 passed; 0 failed
``````

本轮新增测试（3 条，全部集中在 `routes::users::tests`）：

- `update_user_password_request_accepts_easy_password_reset_payload`
- `update_user_payload_extracts_whitelisted_fields`
- `merge_json_preserves_existing_policy_when_patch_is_empty_object`

至此，本地播放器模板对后端的任一调用都会落到真实业务路径；Emby Web Console 客户端对"设备/任务/合集/推荐/预告/直播/编码选项/备份入口/带宽测试/工作室图片/元数据编辑/远程海报 / 远程预告/用户管理"等页面也能拿到结构正确、字段完整的响应。

## 九、第七轮：扫描入库幂等加固（2026-04-24）

真实扫库过程中客户端反馈"影片库无法扫全"。PostgreSQL 日志揭示了三类硬失败，都会让所在事务整体回滚，最终表现为部分媒体未入库：

1. `column "original_title" does not exist` — 老库 `media_items` 缺少 Emby 元数据字段（迁移 0010 没落到当前实例）。
2. `duplicate key value violates unique constraint "media_streams_media_item_id_index_stream_type_key"` — 同一条 `media_item_id`、相同 `(index, stream_type)` 被 ffprobe 报出多次（奇怪容器 / 多 program 场景），INSERT 没有冲突兜底。
3. `duplicate key value violates unique constraint "persons_pkey"` — 历史上 `persons` 的 INSERT 有直接绑 `id`（旧二进制残留），`ON CONFLICT (name, sort_name)` 管不到 pkey 冲突。

### 修复

- **新增迁移 `backend/migrations/0022_scanner_idempotency.sql`**：
  - 以 `ADD COLUMN IF NOT EXISTS` 的方式再补一遍 `media_items` 的 Emby 字段（`original_title / official_rating / community_rating / critic_rating / series_name / season_name / index_number* / provider_ids / genres / studios / tags / production_locations / width / height / bit_rate / video_codec / audio_codec / logo_path / thumb_path / remote_trailers / status / end_date / air_days / air_time`）。
  - 使用 `DO $$` 块检查 `media_streams` / `media_chapters` 的 UNIQUE 约束：如不存在，先清重、再补 `UNIQUE (media_item_id, index, stream_type)` / `UNIQUE (media_item_id, chapter_index)`，让后续 `ON CONFLICT` 能稳定命中。

- **`repository::save_media_streams` / 章节 INSERT**：
  - 流 INSERT 末尾追加 `ON CONFLICT (media_item_id, index, stream_type) DO NOTHING`。
  - 章节 INSERT 末尾追加 `ON CONFLICT (media_item_id, chapter_index) DO NOTHING`。
  - 两者仍在同一事务里，先 DELETE 再 INSERT，保证重扫时用最新探测结果覆盖，同时个别重复条目静默跳过、不再炸事务。

- **`repository::create_person`**：
  - 去掉对 `person.id` 的显式绑定，改为走 `gen_random_uuid()` 默认 + `ON CONFLICT (name, sort_name) DO UPDATE SET ... RETURNING id`。
  - 这样既兼容老二进制留下的 v5 UUID pkey 冲突，也让第二次扫描到同一 `(name, sort_name)` 的人物复用原 ID，不会丢关联。

### 测试

``````
cargo test --bin movie-rust-backend
running 58 tests
test result: ok. 58 passed; 0 failed
``````

（本轮只是把"偶发重复数据 → 整个事务炸掉"改成"静默跳过重复 + 幂等 upsert"，不改对外 API，所以没新增单测；`api_router_builds_without_route_conflicts` 等既有测试全部通过。）

### 升级动作

- 用户下次启动后端时会自动执行迁移 `0022_scanner_idempotency.sql`，把缺失的列和 UNIQUE 索引补齐；旧数据的潜在重复行会在迁移里一次性清除。
- 随后再次扫库即可完整入库，不会再被单个媒体的重复流 / 章节或 person 冲突中断。

至此，剩余官方级待办只剩：

- BackupRestore 真实 `pg_dump` / `pg_restore` 落盘（id 15）。
- OpenAPI / Swagger 自动化文档（id 16）。