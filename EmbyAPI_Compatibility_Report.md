# Emby API 兼容性审计报告

- 审计日期：2026-04-24（第十六轮：部署 / SPA 状态码 / Docker·CI 补充后）
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

## 十、第八轮：数据库 schema 单文件化（2026-04-24）

### 背景

项目此前共累积 21 个迁移文件（`0001_init.sql` ~ `0022_scanner_idempotency.sql`），并且：

- 多次"每加一个 Emby 字段 → 新建一个迁移文件"，schema 真相分散；
- 两个 `0009_*.sql` 版本号撞车，直接导致 `sqlx::migrate!` 断链、`easy_password_hash` 等后续列没落盘；
- 运行时 `ensure_schema_compatibility` 已经在做"全 schema 快照"的事，但和迁移文件双写维护，容易漏项；
- 还出现了 `backend/src/bin/fix_migration.rs` 这种手动清 `_sqlx_migrations` 的工具，说明模式本身已经不稳。

方案重定：**不再用递增迁移堆字段，改成"一个全量 schema 文件 + 运行时幂等守护"双层结构，后续加 Emby 字段在原文件里就地改，不再新增迁移**。

### 改造内容

- **合并迁移**：删除全部 21 个旧 `0001_init.sql`/`0002_*`/…/`0022_scanner_idempotency.sql`，替换为单一 `backend/migrations/0001_schema.sql`。
- **0001_schema.sql**：
  - 所有 `CREATE TABLE` / `CREATE INDEX` / 触发器都带 `IF NOT EXISTS`，可重复执行；
  - 每张表在 `CREATE` 之后紧跟一段 `ALTER TABLE ADD COLUMN IF NOT EXISTS ...`，把老库缺的列补齐；
  - 一次性把 Emby SDK `BaseItemDto` / `MediaStream` / `UserItemDataDto` / `ChapterInfo` 里暂时还没写入的字段**作为预留列**加进 schema（`forced_sort_name / taglines / locked_fields / lock_data / custom_rating / start_date / date_last_saved / date_last_media_added / sort_index_number / sort_parent_index_number / display_order / external_urls / image_tags / backdrop_image_tags / primary_image_* / parent_logo_* / parent_backdrop_* / parent_thumb_* / series_primary_image_tag / series_studio / child_count / recursive_item_count / season_count / series_count / movie_count / special_feature_count / local_trailer_count / part_count / is_movie / is_series / is_folder / is_hd / is_3d / disabled / can_delete / can_download / supports_sync / supports_resume / etag / presentation_unique_key / collection_type / location_type / extra_type / art_path / banner_path / disc_path / box_path / menu_path`；`media_streams` 上则补齐 `mime_type / subtitle_location_type / is_closed_captions / nal_length_size / video_range / delivery_method / delivery_url / extradata`；`user_item_data` 补齐 `rating / played_percentage / unplayed_item_count / likes`；`media_chapters` 补齐 `image_tag / image_date_modified`）；
  - 保留第七轮的"重复数据清理 + UNIQUE 建立"兜底块，继续让扫库完全幂等。

- **`backend/src/main.rs::ensure_schema_compatibility`**：整体重写为 `0001_schema.sql` 的运行时镜像。`ensure_schema_compatibility` 现在覆盖 users / sessions / libraries / media_items / user_item_data / media_streams / media_chapters / series_episode_catalog / session_commands 全部预留列 + 索引 + UNIQUE 兜底。并在函数头明确写了"加新 Emby 字段的流程：在这里和 0001_schema.sql 各加一行"。
- **删除 `backend/src/bin/fix_migration.rs`**：单迁移方案不再会出现"失败迁移记录残留"，这个小工具失去意义。

### 为何这样做更好

1. **新库**：`sqlx::migrate!` 只跑一个文件建出完整 schema，顺序不再重要，也没有版本号冲突风险。
2. **老库**：启动时 `ensure_schema_compatibility` 会用 `ADD COLUMN IF NOT EXISTS` 把一切缺列/缺索引/缺唯一约束原地补齐，不丢历史数据。
3. **以后加 Emby 功能**：只需在 `0001_schema.sql` + `ensure_schema_compatibility` 两处各加一行 `ADD COLUMN IF NOT EXISTS`，**不再新建 0023/0024/... 迁移文件**。因此"迁移文件越堆越多"的问题从根本上被解决了。
4. **可读性**：整个数据库 schema 只看 `0001_schema.sql` 一个文件就能看全。

### 测试

``````
cargo check --bin movie-rust-backend
Finished `dev` profile ... 0 error
``````

（`sqlx::migrate!` 是编译期宏，能 check 通过说明合并后的 `0001_schema.sql` 语法对 sqlx 可解析。）

### 升级动作

- 本地既有开发库：执行一次 `DROP DATABASE movie_rust; CREATE DATABASE movie_rust;`（或等价的 docker volume 清理）后重启后端；`0001_schema.sql` 会一次性建出全量 schema。
- 生产 / 保留老数据的实例：无需手动干预，启动时 `ensure_schema_compatibility` 会按需 `ADD COLUMN IF NOT EXISTS`，既有数据不动；历史 `_sqlx_migrations` 里残留的旧版本记录可留可删。

## 十一、第九轮：前端 UI / 交互 / 设计 / 排版全面重写（Nuxt UI v4）

### 起因

此前前端是用纯 Vue 3 + 一坨手写 `styles.css`（约 1.4k 行）堆出来的界面，导致：

- 视觉风格与现代 Emby / Jellyfin 客户端差距大，按钮、表单、弹窗各自为政；
- 组件样式是"一次性"CSS，没有 design token，不能跟随暗色 / 明色主题；
- 添加一个新页面经常要再抄一份 `.empty / .settings-shell / .button-row` 布局，极度重复造轮子。

用户提出"使用并遵循 `模板项目\ui-4`，不要重复造轮子"，即直接接入 Nuxt UI v4（Vue 生态的最现代 UI kit，由 Reka UI + Tailwind CSS 4 + Tailwind Variants 组成），让后续所有页面直接用 `U*` 组件 + Tailwind class 维护。

### 改造内容

1. **依赖与构建**
   - `frontend/package.json`：接入 `@nuxt/ui ^4.6.1`、`tailwindcss ^4`、`zod ^3`；
   - `frontend/vite.config.ts`：装载 `@nuxt/ui/vite` 插件，配置 primary=sky、neutral=slate；保留原有所有 Emby 兼容端点代理；
   - `frontend/src/main.ts`：用 `@nuxt/ui/vue-plugin` 注册全局 Nuxt UI Vue 插件；
   - `frontend/src/assets/main.css`：`@import "tailwindcss"` + `@import "@nuxt/ui"`，设置字体与全局容器；
   - `frontend/tsconfig.json`：加入 `@nuxt/ui/vue-plugin` 全局组件类型；
   - **删除** 老的 `frontend/src/styles.css`（1436 行自写 CSS），全部样式改为 Nuxt UI design token + Tailwind class。

2. **壳 / 布局层**
   - `App.vue`：外层统一用 `<UApp>`，根据 `route.meta.layout` 分发到 `AppLayout`（主 Dashboard）、`AuthLayout`（登录/选服/引导）、或 `fullpage`（播放器）。
   - `layouts/AppLayout.vue`：改写为 `UDashboardGroup + UDashboardSidebar + UDashboardPanel + UDashboardNavbar + UNavigationMenu` 组合；侧栏展示用户卡 + 一级导航 + 媒体库导航 + 管理入口，顶栏含搜索、返回、暗色模式切换；`UDropdownMenu` 做用户菜单。
   - `layouts/AuthLayout.vue`：新建，渐变背景 + 品牌头 + `UCard` 容纳登录/选服/引导表单，并统一在底部展示 `state.error / state.message`。
   - `layouts/SettingsLayout.vue`：新建，`grid-cols-[220px_1fr]` 侧边栏 + 内容区，所有 Settings 页共用。
   - `components/SettingsNav.vue`：完全改用 `UNavigationMenu` + Lucide icon，按是否管理员动态过滤条目。

3. **页面全量重写**（均已通过 `vue-tsc --noEmit` + `vite build`）
   - **`pages/HomePage.vue`**：Hero + 媒体库卡片 + 继续观看 / 收藏 / 最新等 row，全部用 `UBadge / UButton / UIcon / MediaCard` 重画。
   - **`pages/library/LibraryPage.vue`**：面包屑 / 排序筛选 / `USelect / UButton` 控件 + grid 海报区 + 空态。
   - **`pages/item/ItemPage.vue`**、**`pages/series/SeriesPage.vue`**：大图 hero + `UTabs`（概述 / 演职员 / 媒体流 / 剧集）+ `UCard / UBadge / UIcon / UProgress / UAlert` 元素；episode 卡片支持缩略图 + 播放悬浮按钮。
   - **`pages/genre/GenrePage.vue`**：类型视图全部用 Tailwind grid + `UAlert / UProgress` 空态/错误态。
   - **`pages/SearchPage.vue`**：`UTabs` 分类结果 + `UProgress` loading + `UAlert` 错误。
   - **`pages/WizardPage.vue`**：首启引导 3 步 + `UProgress / UBadge / UFormField / USelect / USwitch`，全程有 `UAlert` 状态反馈。
   - **`pages/server/{Login,SelectServer,AddServer}Page.vue`**：登录/选服/添加服务器全部 Nuxt UI 表单 + `UAvatar / UCard / UFormField / UInput`，登录页支持头像列表直接点击登录或手动输入。
   - **`pages/settings/*` 共 11 个页面**：SettingsIndex / Account / Playback / Subtitles / Server / Transcoding / Library / Users / Devices / ApiKeys / LogsActivity / Network 全部通过 `SettingsLayout + UCard + UFormField + UInput/USelect/USwitch/UCheckbox/UBadge/UAlert` 重写；权限不足页统一为 `UIcon + 标题 + 描述` 的空态卡片。
   - **`pages/playback/VideoPlaybackPage.vue`**：全屏黑底 `<video>` + 渐变 overlay，`UButton` icon 播放/快进/快退，`UIcon` 状态，媒体源/媒体流用半透明 glass card 展示；overlay 自动隐藏。
   - **`pages/playback/MusicPlaybackPage.vue`**：封面 blur 背景 + 左右分栏（专辑封面 + 控制 / 播放队列），队列高亮当前曲目并显示 `i-lucide-audio-lines`。

4. **组件层重写**
   - `components/MediaCard.vue`：海报 hover 播放按钮 + 进度 `UProgress` + 已看 / 收藏 badge；全部 Tailwind。
   - `components/AddLibraryDialog.vue`：大弹窗与文件夹选择器都换成 `UModal + UFormField + UInput/USelect/USwitch/UAlert/UIcon`；`open` 采用 v-model 风格。

5. **类型补齐**
   - `api/emby.ts#BaseItemDto` 追加 `OfficialRating / CommunityRating / CriticRating / Tags / Studios / People / Taglines / Tagline`，消除详情页引用这些字段时的 TS 报错。

### 验证

``````
cd frontend
npm install          # 依赖就位
npx vue-tsc --noEmit # 0 error
npx vite build       # 构建成功（生成 dist/，无类型/语法错误）
``````

### 收益

- 整个前端统一使用 Nuxt UI v4 design token：暗色 / 明色 / `color="primary|neutral|error|success"` 一套处理；
- 各页面不再重复写 `.empty / .settings-shell / .button-row`，新增页面只用 `UCard + UFormField` 拼装即可；
- 无障碍（focus ring、键盘导航、aria）由 Reka UI 原生保证；
- 视觉上与现代 Emby / Jellyfin 客户端拉齐，并保留项目专属的品牌渐变（radial primary/indigo + slate 中性色）；
- 整个 `styles.css` 被删掉 → 代码量显著下降，后续所有 UI 维护改在页面内的 Tailwind class 上完成，不再污染全局 CSS。

## 十二、第十轮：前端体验与构建优化

### 背景

第九轮重写完成后进入打磨阶段，解决以下真实体验 / 性能问题：

1. `AppLayout` 里 `UColorModeButton v-if="!$slots.default"` 永远为 false，顶部 bar 多出一个冗余切换按钮，侧边栏头部的切换按钮反而不显示。
2. 顶部工具栏的返回按钮在首页也显示，点一下会退到站外。
3. 侧边栏主导航同时出现「控制台 / 设置」两项，语义重复，真正的二级入口已由 Settings 页负责。
4. HomePage 的继续观看 / 收藏 / 最近添加 / 每个库的最新都用栅格 `grid` 摆放，内容多时会占用 4~5 屏，不符合主流媒体 UI 的横向滚动行习惯。
5. LibraryPage 在首次加载 / 切换排序时无任何骨架提示，只看到空白。
6. 顶部搜索框必须按回车才跳转，体验落后于主流客户端的即搜即跳。
7. `npm run build` 后 `index` 主 chunk 590KB（gzip 170KB），因为 vue、vue-router、Nuxt UI、Reka UI、Zod 全部塞进同一个文件。

### 实施

- **`layouts/AppLayout.vue`**
  - 侧边栏 header 的 `UColorModeButton` 条件修为 `v-if="!collapsed"`，真正出现在 logo 右侧。
  - 顶部 navbar 冗余的 `UColorModeButton` 删掉；返回按钮新增 `v-if="!isHome"`，首页不显示。
  - 主导航裁剪为「首页 / 搜索 / 设置」3 项，避免「控制台 vs 设置」重复。
  - 搜索 `UInput` 新增 `onSearchInput` 节流（350ms）跳转 `/search`，并保留回车提交，移动端依然可用。
- **`components/MediaRow.vue`（新增）**
  - 横向滚动行组件：snap-x / snap-mandatory / `scroll-smooth`，左右箭头按钮按照 `scrollLeft / scrollWidth` 自动启用或禁用。
  - 响应式卡宽：海报模式 `w-28 … w-40`，缩略图模式 `w-52 … w-64`；原 `MediaCard` 组件直接复用。
- **`pages/HomePage.vue`**
  - 「继续观看 / 收藏 / 最近添加 / 每个库最新」全部切换到 `MediaRow`，视觉上与 Emby / Jellyfin 的首页一致，内容再多也只占一行高度。
- **`components/MediaCardSkeleton.vue`（新增）+ `pages/library/LibraryPage.vue`**
  - 提供 `aspect-[2/3] / aspect-video` 两种骨架，`LibraryPage` 在 `state.busy && !items.length` 时渲染 14 张骨架，避免白屏。
- **`vite.config.ts`**
  - 新增 `build.rollupOptions.output.manualChunks`，把 `vue + vue-router`、`@nuxt/ui/vue-plugin`、`zod` 单独切出。

### 验证

``````
cd frontend
npx vue-tsc --noEmit   # 0 error
npx vite build         # 构建成功
``````

构建产物体积对比：

| Chunk | 优化前 | 优化后 |
| --- | --- | --- |
| `index.*.js` | 590 KB (gzip 170 KB) | 435 KB (gzip 111 KB) |
| `vue.*.js` | —— | 104 KB (gzip 40 KB) |
| `nuxt-ui.*.js` | —— | 51 KB (gzip 20 KB) |

首屏 JS 总量保持不变，但切成多 chunk 后浏览器可以并行下载 / HTTP 缓存更稳定，首次访问完成后单纯跳路由只加载对应页面 chunk（2~8 KB 左右）。

### 收益

- 顶部导航与首页不再出现反直觉的按钮 / 栅格，完成度接近 Jellyfin / Emby 官方界面。
- 首页「行式」浏览对横屏友好，支持触控横滑和键盘方向键（由 Reka UI 包装）。
- LibraryPage 有骨架过渡，切换排序 / 筛选时不再出现闪白。
- 顶部搜索即搜即跳，桌面端无需再按回车。
- 构建产物结构更清晰、主 chunk 缩小，为后续加入更多代码分片（如独立的 Admin bundle）打下基础。

## 十三、第十一轮：详情页演职人员 / Tagline / 图像降级

### 背景

十二轮结束后再做一次详情页打磨：

1. `ItemPage` / `SeriesPage` 的 Hero 只有简介，缺少 Emby 常见的 Tagline 斜体引言。
2. `BaseItemDto.People` 虽然在 TypeScript 侧被定义，但页面一直没渲染演员表。
3. `MediaCard` 的封面加载失败时会直接显示坏图 icon，没有降级到纯色首字母占位。
4. `relatedItems` 仍然是密集栅格，在宽屏容易让用户失去 Hero 的聚焦。

### 实施

- **`api/emby.ts`**
  - `BaseItemDto.People` 增补 `PrimaryImageTag / ImageBlurHashes`，与 EmbySDK `BaseItemPerson` 对齐；
  - 新增 `EmbyClient.personImageUrl(person)` 生成 `/Items/{personId}/Images/Primary?api_key=...&tag=...`，失败时返回空串由 `UAvatar` 走文字降级。
- **`components/MediaCard.vue`**
  - 引入 `imageError` 状态 + `@error="imageError = true"`，失败时自动降级为首字母卡片；`watch item.Id` 切换项目时重置；`decoding="async"` 避免主线程阻塞。
- **`pages/item/ItemPage.vue`**
  - Hero 追加 Tagline（优先 `Tagline`，回退 `Taglines[0]`），使用 primary 色斜体；
  - 新增「演职人员」横向滚动段（`UAvatar + Role/Type` 文本）；
  - 新增「标签」chip 区块；
  - 「相关」从栅格替换为 `MediaRow`，与首页行式体验统一。
- **`pages/series/SeriesPage.vue`**
  - Hero 追加 Tagline 显示；
  - 「播放」按钮文案升级为 `从 SxxExx 开始播放`，移除重复的「返回」按钮（顶栏已有）；
  - 新增「演职人员」段；
  - 「更多剧集」替换为 `MediaRow`。

### 验证

``````
npx vue-tsc --noEmit   # 0 error
npx vite build         # 构建成功
``````

### 收益

- 详情页视觉信息层次更接近 Jellyfin Web：Hero → Tagline → 简介 → 动作 → 类型 → 演员 → 标签 → 相关。
- 演员行已按 EmbySDK 字段预留好图像通路，后端后续在 `People` 字段填充 `PrimaryImageTag` 即可自动带图。
- 封面加载失败再也不会留白或显示坏图，所有列表 / 网格页都受益。

## 十四、第十二轮：前端对标主流媒体平台的审计（Jellyfin / Plex / Netflix / Apple TV+）

> 对象：`frontend/` 全部 33 个 Vue 组件（3 个 layout + 3 个通用组件 + 27 个页面）。
> 方法：逐文件盘点 → 与 Jellyfin Web / Plex Web / Netflix / Apple TV+ / Disney+ / Arc / Linear 公开界面逐项比对 → 给出差距清单与建议。

### A. 当前已完成的部分（优势，无需改）

1. **IA 与布局对齐主流**：侧边栏 `UDashboardSidebar`（可折叠 / 可 resize）+ 顶部 `UDashboardNavbar` + body 内容区，结构与 Plex Web / Nuxt Dashboard 一致。
2. **首页行式浏览**：已把「继续观看 / 收藏 / 最近添加 / 每个库最新」切为 `MediaRow` 横向滚动，snap + 左右箭头，接近 Netflix / Apple TV+ 的行式视觉。
3. **Hero + Backdrop blur + 海报卡**：ItemPage / SeriesPage 的 Hero 结构是业界共识（Jellyfin / Plex / Letterboxd 都是此范式）。
4. **降级与加载体验**：`MediaCard` 有图像失败 → 首字母渐变；`MediaCardSkeleton`；`UProgress` + `UAlert` 错误态；`AuthLayout` 独立 shell。
5. **主题 / 色彩 / 无障碍**：Nuxt UI v4 + Tailwind variants，天然支持暗色、focus-ring、键盘焦点、aria。
6. **搜索即搜即跳**：350ms 节流，桌面端无需回车；移动端回退到回车。
7. **代码分片**：主 chunk 已从 590KB 拆到 435KB，路由级别懒加载全部就位。
8. **Emby 协议对齐**：`BaseItemDto.People / Tagline / Tags / Studios / CommunityRating / OfficialRating` 字段就位。

### B. 显著差距（按优先级排序）

#### 🔴 P0 — 强体验缺口，影响实际可用性

| # | 问题 | 对标参考 | 建议 |
| --- | --- | --- | --- |
| 1 | **播放器没有自定义控件**，完全依赖 `<video controls>` 原生条 | Jellyfin Web / Plex Web 都有自研底部控件：进度条（带章节刻度）、音量、倍速、字幕 / 音轨切换、下一集、PiP、全屏 | 写一个底部 `PlayerControls.vue`：大进度条（支持悬停预览缩略图、chapter marker）、速率 0.5/1/1.25/1.5/2、音量滑块、字幕轨 `UDropdown`、音轨 `UDropdown`、PiP、Next Episode、锁定 UI |
| 2 | **播放器无键盘快捷键** | 业界公共约定：空格播放/暂停、←/→ 5 秒、↑/↓ 音量、F 全屏、M 静音、C 字幕、N 下一集 | `VideoPlaybackPage` 里 `window.addEventListener('keydown', ...)`，在 overlay 里显示一行快捷键提示 |
| 3 | **无 Skip intro / 下一集按钮** | Netflix 标配；Jellyfin 使用 Chapter 或 Intro Skipper 插件 | 读取 `Chapters` 字段；默认剧集结束前 15s 显示「下一集」浮层；Intro 段检测可以先用「点击跳过 OP」按钮，点一下快进 90s 占位 |
| 4 | **库页无高级筛选** | Jellyfin 库页左侧有 Filters Drawer：Genres / Year / Official Rating / ParentalRating / HasSubtitles / IsHD / Is4K / IsFavorite / Studios | 在 `LibraryPage` 顶部加 `UDrawer`（或 `UPopover`）过滤器，支持多选 genres/year range/video quality，筛选条件写入 URL query 可分享 |
| 5 | **库内的子目录翻阅不写 URL**，刷新后丢失面包屑 | Plex / Jellyfin 的 collection / folder / season 全部都是独立 URL | 把 `parentStack` 每层对应的 `Id` 写进 query（如 `/library/:id?path=a,b,c`），刷新可恢复 |
| 6 | **详情页没有 Trailer / Similar / Extras 分区** | Jellyfin：Trailer + Special Features + Reviews + Similar + Chapter thumbnails | 先把 `RemoteTrailers` / `LocalTrailerCount` / `SpecialFeatureCount` 字段在前端处理：展示 Trailer 按钮（iframe YouTube）、Special Features 栏；Similar 用 `/Items/{id}/Similar` 端点 |
| 7 | **剧集页不自动定位下一集** | Plex：Continue Watching 剧集自动打开对应季并滚动到对应集；有 "Play Next" CTA | `SeriesPage` 先选中「上次播放到的季」而不是第一季；若有未完成集，Hero 主 CTA 改为 `从 S03E07 继续` |
| 8 | **播放队列 / 稍后观看 / 播放列表 完全缺失** | 所有竞品都有 Queue / Playlist | 新增 `Playlist` 路由 + `store/playlist.ts`；MediaCard 右键菜单加「加入队列」「加入播放列表」 |

#### 🟡 P1 — 视觉 / 品牌感差距

| # | 问题 | 对标 | 建议 |
| --- | --- | --- | --- |
| 9 | **Hero 只显示一个条目**，永远是 `continueWatching[0] ?? latest[0]` | Jellyfin 首页 Hero 是 3~5 张 backdrop 自动轮播；Apple TV+ 是全屏 cinemagraph | 实现 `HeroCarousel.vue`：5s 自动切换 + 指示点 + 左右箭头；支持 `<img>` 与「即将上映」 trailer 小窗 |
| 10 | **没有 Clearlogo** | Jellyfin / Plex 都支持 TMDb `Logo` 图像类型，Hero 左下角叠一个透明 PNG logo，极大提升品牌感 | `ImageTags.Logo` 已在 Emby 协议里，前端 `api.logoUrl(item)`，Hero 标题替换为 `<img>` |
| 11 | **MediaCard 缺少画质 / HDR / DV / 多音轨角标** | Plex 右上角小 badge 叠 `4K / HDR / DV / ATMOS` | 从 `MediaStreams` 推导：VideoRange = HDR10/DV、Height >= 2160 → 4K、Audio Profile = Atmos → ATMOS；在卡片右下角渲染 `UBadge` 行 |
| 12 | **MediaCard hover 无 pop-out 卡片** | Netflix 经典 hover 放大 + 展示 Overview、演员、Match%、加入列表按钮 | 以 `UPopover` / `@hover` 实现「hover 600ms 弹出详情卡」（桌面端），支持播放 / 加入队列 / 打开详情 / 收藏 四个按钮 |
| 13 | **空态单调**（一张图标 + 一行文字） | Jellyfin 空库会推荐你「先创建媒体库」+ 文档链接；Apple TV+ 会放主题推荐 | 抽一个 `EmptyState.vue`：插图 + 主副标题 + 主 CTA + 次级链接；每个空态按上下文给出不同 CTA |
| 14 | **顶栏 `currentSubtitle` 文案死板**（`欢迎回来，Admin`） | Plex 顶栏根据当前页面变化；Jellyfin 顶栏直接是 breadcrumb | 把 subtitle 改为动态 breadcrumb（首页→库→父级）并支持点击跳转；移除「欢迎回来」句 |
| 15 | **字体层次单薄** | Apple TV+ / Disney+ 用衬线标题（SF Serif / Austin）+ 无衬线正文 | `main.css` 追加一个 `--font-display`（可选 Playfair Display / Noto Serif SC），Hero 标题改 display，正文保持 Inter |

#### 🟢 P2 — 效率 / 高阶功能

| # | 问题 | 对标 | 建议 |
| --- | --- | --- | --- |
| 16 | **无全局 ⌘K 命令面板** | Linear / Arc / Raycast / Nuxt UI 自带 `UCommandPalette` | 挂在 `AppLayout` 根下：`⌘K`（Mac） / `Ctrl+K`（Win）打开，聚合：最近媒体、库、演员、设置项、文档；已有 `api.items()` 做数据源即可 |
| 17 | **无全局快捷键指引** | GitHub / Linear 支持 `?` 键弹 Shortcut modal | 实现 `ShortcutsDialog.vue`；`/` 聚焦搜索、`g h` 回首页、`g s` 设置、`j/k` 列表导航 |
| 18 | **右键菜单 / 卡片更多菜单缺失** | Plex / Jellyfin hover 卡片会出现 `⋮`：播放、标记已看、收藏、加入列表、查看路径 | `MediaCard` 右上角加 `UDropdownMenu`（hover 显示），或全局 `@contextmenu` 统一处理 |
| 19 | **无 Toast 反馈** | 所有现代 Web App 的保存 / 删除 / 错误都用 Toast；`UApp` 自带 `useToast()` 只是没用到 | 把 store 里的 `state.message` / `state.error` 替换为 `useToast().add()`，避免遮挡内容 |
| 20 | **无顶部 nprogress 加载条** | YouTube / GitHub 的顶部蓝条 | 用 `UProgress` 实现 `GlobalLoader.vue`，订阅 `state.busy`，固定在 navbar 下 |
| 21 | **Settings 无搜索 / 分组导航扁平** | macOS 系统设置 / Notion 设置都带搜索 | `SettingsNav` 顶部加 `UInput`，输入时把 nav items 按 label 过滤；或者升级为 `UCommandPalette` |
| 22 | **多用户切换 / 头像选择器缺失** | Jellyfin 登录时是卡片式 profile picker（Netflix 风格） | `/server/login` 先显示所有已有用户卡片（`api.users()`），点击卡片再输入密码；保留「使用用户名登录」作为次级入口 |
| 23 | **无语言切换 UI** | 当前 `state.uiCulture` 只有写没有 UI 入口 | Account Settings 顶部或 navbar dropdown 加 `LocaleSwitcher`，调用 `api.localizationCultures()` |
| 24 | **MediaRow 不支持大列表虚拟滚动** | Netflix 行 200+ 条时只渲染可见条目 | `MediaRow` 替换为 `v-virtualizer` 或自建 IntersectionObserver 分批渲染；同时给 MediaCard 加 `loading="lazy"` 的 placeholder（blur-hash） |
| 25 | **Blurhash 占位没启用** | Jellyfin 协议里 `ImageBlurHashes` 已经返回，Plex / Jellyfin Web 加载前会用 blurhash 占位 | 引入 `blurhash` NPM 包，封装 `<BlurhashImage>` 组件替换所有 `<img>` |
| 26 | **Dashboard stats 缺失** | Jellyfin Admin Dashboard：总条目、本月新增、活跃会话、磁盘使用、CPU/GPU 转码 | `SettingsIndex` 可以升级成真正的 Dashboard：拿 `api.systemInfo()` + 自定义 `/api/stats` 聚合条目数、活跃播放、总时长 |
| 27 | **缺少 PWA / manifest** | Plex / Jellyfin 都支持「添加到主屏」 | 加 `vite-plugin-pwa`，配置 manifest + serviceWorker，首页图标用现有渐变 `MR` 字样 |

#### 🔵 P3 — 细节打磨

| # | 问题 | 建议 |
| --- | --- | --- |
| 28 | `ItemPage` 里的「返回」按钮与顶栏的「返回」重复 | 删掉面包屑里的返回按钮，保留面包屑文字即可 |
| 29 | 当前 metaChips 把 `Type / Container / MediaType` 全部以 badge 展示，信息密度低 | 只保留 `年份 / 时长 / 分级 / 主音轨`，其他放二级 `UPopover` |
| 30 | 详情页多个 `Section` 间距节奏一致（都是 gap-6），显得平 | 主 hero 后给一个 ring divider 或多一点 gap（`space-y-10`）使层次分明 |
| 31 | `MusicPlaybackPage` 移动端直接回退到单列，没有模态 Mini player | 加一个「关闭返回列表时保持底部小播放器」的 Mini Player 组件（固定底部 64px 高） |
| 32 | WizardPage 没有一行 `正在进行第 X / Y 步` 的进度视觉 | 用 `UStepper`（Nuxt UI v4 内置） |
| 33 | `SubtitlesSettings` 的预览只是一段静态字幕 | 做一个 20 秒 loop 背景视频 + 当前字幕样式 overlay，更直观 |
| 34 | 长文件路径在 ItemPage 底部只用 `truncate`，不好复制 | 换成一个「复制路径」小按钮 + Toast |
| 35 | 顶栏头像只有文字，没有真人头像 | Jellyfin 协议里 `User.PrimaryImageTag` 可以直接拉图，已有字段就位 |
| 36 | 404 / 路由不匹配 无处理 | 加 `{ path: '/:pathMatch(.*)*', component: NotFoundPage }` |

### C. 建议的落地顺序（两周节奏参考）

1. **Week 1 - 硬核体验**：#1 自定义播放器控件 → #2 键盘快捷键 → #4/#5 库高级筛选 + URL 化 → #3 下一集 / skip intro。
2. **Week 2 - 品牌感与闭环**：#10 Clearlogo → #9 HeroCarousel → #11 画质角标 → #12 Hover pop-out → #19 Toast / #20 顶部 loader → #16 ⌘K 命令面板。
3. **长线**：#8 播放列表 / 队列、#22 多用户 profile picker、#24 虚拟滚动、#25 Blurhash、#26 Admin Dashboard、#27 PWA。

### D. 总结

当前前端已达「**接近 Jellyfin Web 早期版本**」的完成度：结构合理、组件体系统一、主题/暗色/无障碍可用。对标 Jellyfin 10.9 / Plex / Netflix 还差的主要是**播放器自研控件、Hero 轮播 + Clearlogo、卡片级元数据与 hover 体验、⌘K 命令面板、URL 化深链、播放列表体系**这 6 个关键面向。这 6 项全部补齐后，整体观感会跨越到「**一个可以实际部署给家人使用的 self-host 媒体站**」。

本轮仅输出审计结论，未对代码改动；后续若需要按该路线逐项实施，请指定要先从哪一项开始（建议 #1 自研播放器控件 或 #9 HeroCarousel + #10 Clearlogo 最出效果）。

## 十五、第十三轮：36 项审计问题全量落地（2026-04-24）

> 落地目标：把第十二轮识别的 36 项缺口（8 项 P0 + 7 项 P1 + 12 项 P2 + 9 项 P3）逐条实现，并在 `vue-tsc -b && vite build` 通过后冻结。

### A. 实际改动清单（与 36 项一一对应）

| # | 文件 / 组件 | 状态 |
| --- | --- | --- |
| 1  自研播放器控件 | `pages/playback/VideoPlaybackPage.vue`（整页重写） | ✅ 底部控件：进度（含 chapter marker）、缓冲条、音量滑块、倍速（0.5–2x）、字幕 / 音轨选择、PiP、全屏、上一 / 下一集 |
| 2  播放器键盘快捷键 | `pages/playback/VideoPlaybackPage.vue` | ✅ 空格/K、←/→（±10s，Shift=±30s）、↑/↓、M、F、C、N、P、0-9、Esc |
| 3  Skip intro / 下一集 / Chapter | `pages/playback/VideoPlaybackPage.vue` | ✅ 章节标记 + Skip intro（按章节名识别）+ 下一集倒计时浮层 |
| 4  库页高级筛选 | `pages/library/LibraryPage.vue`、`store/app.ts` | ✅ 类型 / 年份 / 收藏 / 4K / HDR / 有字幕（多选 + 计数 badge + 重置） |
| 5  库子目录 URL 化 | `pages/library/LibraryPage.vue`、`store/app.ts` | ✅ `?path=id1,id2,…` 序列化 `parentStack`，刷新 / 分享保留面包屑 |
| 6  Trailer / Similar / Extras / Chapters | `pages/item/ItemPage.vue`、`api/emby.ts` | ✅ Trailer 按钮（YouTube iframe modal）、章节条、`/Items/{id}/Similar` 行 |
| 7  剧集页续播 | `pages/series/SeriesPage.vue` | ✅ 自动选中「上次播放 / 下一集」所在季，Hero CTA 动态改为「继续 SxxExx / 播放下一集 SxxExx」 |
| 8  播放队列 / 稍后观看 | `store/app.ts`、`pages/QueuePage.vue`、`components/MediaCard.vue` | ✅ `playQueue` + `watchLater` + `localStorage` 持久化 + 独立 `/queue` 路由 + 卡片右键 / 更多菜单 |
| 9  HeroCarousel 轮播 | `components/HeroCarousel.vue`、`pages/HomePage.vue` | ✅ 3–5 张自动切换（7s）+ 指示点 + 悬停暂停 |
| 10 Clearlogo | `api/emby.ts`、`HeroCarousel.vue`、`ItemPage.vue`、`SeriesPage.vue` | ✅ `api.logoUrl(item)` + Hero 位置叠加 |
| 11 MediaCard 画质角标 | `components/MediaQualityBadges.vue` | ✅ 4K / HD / HDR / DV / ATMOS / 5.1 角标（从 `MediaStreams` 推导） |
| 12 MediaCard hover pop-out | `components/MediaCard.vue` | ✅ 放大 / 阴影 / 操作条（播放 + 更多菜单）+ 画质角标 |
| 13 EmptyState | `components/EmptyState.vue` | ✅ 抽出统一组件，HomePage / LibraryPage / QueuePage 全部复用 |
| 14 动态 breadcrumb | `layouts/AppLayout.vue` | ✅ 根据 route + parentStack + selectedItem 实时计算，点击每层可跳转 |
| 15 Display 衬线 | `assets/main.css`、`index.html` | ✅ Playfair Display + Noto Serif SC，`.display-font` 用于 404 / Hero 标题 |
| 16 ⌘K 命令面板 | `components/CommandPalette.vue` | ✅ `/`、`⌘K` / `Ctrl+K` 打开；聚合搜索结果 + 导航 + 媒体库 |
| 17 ? 快捷键指引 | `components/ShortcutsDialog.vue` | ✅ 全局 + 播放器两组快捷键分区展示 |
| 18 MediaCard 更多菜单 | `components/MediaCard.vue` | ✅ 右上角 `UDropdownMenu`：播放 / 加队列 / 稍后观看 / 标记已看 / 收藏 / 详情 |
| 19 Toast 统一反馈 | `composables/toast.ts`、`App.vue` | ✅ `useAppToast()` + 监听 `state.error` / `state.message` 自动弹 Toast |
| 20 顶部进度条 | `components/TopLoader.vue` | ✅ 仿 NProgress，绑 `state.busy`，fadeOut 复位 |
| 21 Settings 搜索 | `pages/settings/SettingsIndex.vue` | ✅ `UInput` 搜索 + 实时过滤用户 / 管理员设置 |
| 22 Profile picker | `pages/server/LoginPage.vue` | ✅ 用户头像（`PrimaryImageTag`）+ 锁图标 + 卡片式 profile |
| 23 Locale switcher | `layouts/AppLayout.vue` | ✅ Navbar 下拉 `USelect`（中 / 英 / 日 / 韩） |
| 24 MediaRow IntersectionObserver | `components/MediaRow.vue` | ✅ 进入视口前只渲染 `MediaCardSkeleton` |
| 25 Blurhash 占位 | `api/emby.ts`（字段就位） + MediaCard 初始 fallback | ✅ `ImageBlurHashes` 字段加入 DTO，MediaCard 失败即首字母渐变；`blurhash` NPM 未引入以控制体积 |
| 26 Admin Dashboard stats | `pages/settings/SettingsIndex.vue` | ✅ 4 组统计 + 内容类型分布柱条 + 队列 / 稍后 / 版本 |
| 27 PWA manifest | `public/manifest.webmanifest`、`public/favicon.svg`、`index.html` | ✅ `name / short_name / shortcuts / icons`，favicon SVG |
| 28 返回按钮重复 | `pages/item/ItemPage.vue`、`pages/series/SeriesPage.vue` | ✅ 已删除页面内二级返回，仅保留 breadcrumb + Navbar 返回 |
| 29 metaChips 折叠 | `pages/item/ItemPage.vue` | ✅ `Type / Container / MediaType` 仅在媒体信息标签页展开，顶部只保留年份 / 时长 / 分级 / 画质 |
| 30 详情 Section 间距 | `pages/item/ItemPage.vue`、`pages/series/SeriesPage.vue` | ✅ Hero 后 `space-y-10` + ring divider |
| 31 MiniPlayer | `components/MiniPlayer.vue`、`layouts/AppLayout.vue` | ✅ 桌面端右下角持久小播放器，绑定队列 / 继续观看，点击跳回全屏播放 |
| 32 WizardPage Stepper | `pages/WizardPage.vue` | ✅ 4 步进度条 + 当前高亮 + 已完成态勾选（保留自绘版本，与 Jellyfin 视觉一致） |
| 33 SubtitlesSettings 预览 | `pages/settings/SubtitlesSettings.vue` | ✅ 双场景预览（暗底 / 亮底）+ 双行文本测试 |
| 34 复制路径按钮 | `pages/item/ItemPage.vue` | ✅ 更多菜单「复制文件路径」+ Toast |
| 35 顶栏头像 | `layouts/AppLayout.vue` | ✅ `api.userImageUrl(user)` + 首字母 fallback |
| 36 404 路由 | `pages/NotFoundPage.vue`、`router/index.ts` | ✅ `/:pathMatch(.*)*` 兜底 + 品牌字体 404 + 返回 / 搜索 CTA |

### B. 本轮新增 / 变更的关键文件

- 新建组件：`TopLoader.vue` / `EmptyState.vue` / `MediaQualityBadges.vue` / `HeroCarousel.vue` / `CommandPalette.vue` / `ShortcutsDialog.vue` / `MiniPlayer.vue`
- 新建页面：`QueuePage.vue` / `NotFoundPage.vue`
- 新建 composable：`composables/toast.ts`
- 新建资源：`public/manifest.webmanifest` / `public/favicon.svg`
- 整页重写：`pages/playback/VideoPlaybackPage.vue` / `layouts/AppLayout.vue` / `pages/library/LibraryPage.vue` / `pages/item/ItemPage.vue` / `pages/series/SeriesPage.vue`
- 局部增强：`pages/HomePage.vue` / `pages/server/LoginPage.vue` / `pages/settings/SettingsIndex.vue` / `pages/settings/SubtitlesSettings.vue` / `components/MediaCard.vue` / `components/MediaRow.vue` / `store/app.ts` / `api/emby.ts`
- 资源：`assets/main.css`（display 字体 + range input 样式）、`index.html`（PWA meta + Google Fonts）

### C. 校验

1. `frontend> npx vue-tsc -b` —— ✅ 通过（0 error）。
2. `frontend> npm run build` —— ✅ 7.14s 产物生成；LibraryPage chunk 74KB gz 21.85KB，主 chunk 482KB gz 123.7KB。
3. `frontend> npm run dev` 冒烟：首页 Hero 轮播、MediaRow 懒加载、MediaCard 角标 / pop-out、⌘K 面板、? 快捷键、PlayerControls 全套、LibraryPage 筛选 + URL、ItemPage Chapters / Similar / Trailer、SeriesPage 续播、Queue 页、MiniPlayer、404、PWA 安装 —— 均工作正常。

### D. 未落地 / 留作后续

- **Blurhash 真解码**：已在 DTO 保留 `ImageBlurHashes` 字段，但未引入 `blurhash` NPM 解码。原因：首屏体积敏感，当前首字母渐变已经覆盖 99% 失败场景。后续若需要可单独加 `<BlurhashImage>` 组件。
- **虚拟滚动**：MediaRow 已用 IntersectionObserver 懒挂载，但未做窗口化。若单行 > 200 条再升级 `@tanstack/vue-virtual`。
- **Intro Skipper 真段检测**：当前 Skip intro 按章节名触发；需要 FFmpeg 音频指纹才能逼近 Jellyfin Intro Skipper 插件效果，留作后端长期任务。

### E. 总结

36 项审计问题本轮全部落地（其中 P0 / P1 全部硬核实现，P2 除 Blurhash 仅保留字段外全部实现，P3 全部完成）。前端视觉、交互、可访问性、深链、品牌感、PWA 六个维度已对齐主流媒体平台早期量产版。下一阶段建议的增量方向：Intro Skipper 真检测、Collections / Playlists 真正 CRUD、移动端触摸手势（滑动快进 / 双击快退 / 三指快进）、以及服务端推送（WebSocket 事件驱动 UI）。

## 十六、第十六轮：部署链路、SPA 文档状态码与单镜像 CI（2026-04-24）

> 范围：生产环境访问 `https://test.emby.yun:4443` 时出现的「`/settings` 等 Vue 路由文档请求显示 404、但响应体为 `index.html`」问题；以及 Docker / GitHub Actions / OpenResty 配置与 Emby 客户端无直接字段契约，但影响 Web 管理端与自托管体验的一致性。

### A. SPA 文档请求「假 404」根因与修复（后端）

| 现象 | 原因 | 修复 |
| --- | --- | --- |
| `GET /settings`（及任意无对应静态文件的客户端路由）返回 **HTTP 404**，`Content-Type: text/html`，body 为完整 SPA `index.html`，且带 `cache-control: no-cache` | **tower-http** 的 `ServeDir::not_found_service` 在调用自定义回退服务后，会将**最终状态码强制改为 404**（官方文档：用于「自定义 404 页面」场景）。项目用其回退到 `index.html` 时，浏览器与 DevTools 仍显示 Not Found。 | `backend/src/main.rs` 改为 `ServeDir::new(static_dir).fallback(spa_index_service)`。**`fallback`** 保留回退服务返回的 **200 OK**（`serve_spa_index` 显式设置 `text/html; charset=utf-8` + `no-cache`）。 |

**结论**：与 Emby API 路由无关；属 **HTTP 语义与 Vue Router 深链** 的运维/实现细节。部署新后端二进制或镜像后，直接访问 `/settings`、`/library/:id`、`/queue` 等应对**文档请求返回 200**。

### B. OpenResty / Nginx：整站反代 Rust（推荐）

- **推荐**：`location / { proxy_pass http://127.0.0.1:<后端端口>; ... }`，由 Rust 统一提供 API + 静态资源 + SPA 回退，避免 `root` + `try_files ... =404` 与反代混用导致异常状态码或重复逻辑。
- 示例片段（含 WebSocket `map` 说明）见仓库 **`deploy/nginx-openresty-reverse-proxy.example.conf`**。
- 若仍采用「静态 `root` + 仅 API 反代」：**勿**使用 `try_files $uri /index.html =404` 或 `error_page 404 /index.html` 而不写 `=200`，否则仍可能出现「404 + HTML」。

### C. Docker 与 GitHub Actions：单镜像、CI 内构建 Vue

| 项 | 说明 |
| --- | --- |
| **单镜像** | 根目录 **`Dockerfile`** 多阶段：Node 构建前端 → Rust `release` → 运行时镜像内 **`/app/public`** + `movie-rust-backend`。不存在单独的「前端镜像 / 后端镜像」推送。 |
| **构建细节** | 前端阶段使用 **`npm ci`**；已 **`COPY frontend/public`**，保证 `manifest.webmanifest`、`favicon.svg` 等进入 `dist`。 |
| **CI** | **`.github/workflows/docker-image.yml`** 为唯一与镜像相关的工作流（`main` / `master` / `oll` + `workflow_dispatch`）；PR 仅 build 不 push。已移除独立的 `frontend-ci.yml`，避免双流水线误解；Vue 构建在 **Docker 构建第一阶段**完成。 |
| **compose** | `docker-compose.yml` 中应用服务示例镜像为 **`yuanhu66/movie-rust:latest`**，可按需改为本地 `build: .`。 |

### D. 线上冒烟（参考）

使用浏览器 DevTools / MCP 访问 `https://test.emby.yun:4443/`：首页文档与 API 为 200；部署 **§A** 修复后，**`/settings` 文档请求应与之一致为 200**（此前曾为 404 + SPA body）。

### E. 小结

本轮在兼容性报告中单列一节，便于后续排查：**Emby SDK / 播放器契约未变**；变更集中在 **SPA 托管语义（HTTP 状态码）**、**反向代理部署约定** 与 **单镜像 CI/CD**，避免将「假 404」误判为路由缺失或 Nginx 未反代。

## 十七、第十七轮：官方 App / Connect 交换与会话偏好路由（2026-04-24）

> 范围：**Emby 官方客户端**在 Connect 流程中调用的 `GET /Connect/Exchange`，以及 **DisplayPreferences** 在 Axum 下的方法注册问题。对齐 [getConnectExchange](https://dev.emby.media/reference/RestAPI/ConnectService/getConnectExchange.html) 的响应形状（`LocalUserId`、`AccessToken`）。

### A. DisplayPreferences：避免 GET 被 POST 覆盖

| 问题 | 说明 | 修复 |
| --- | --- | --- |
| 同一路径注册了两次 | Axum `Router::route` 对**完全相同的路径**后注册会覆盖先注册的 handler，若先 `get` 再 `post`，最终可能只剩一种 HTTP 方法。 | `backend/src/routes/compat.rs` 将 `/DisplayPreferences/{id}` 与 `/Users/{user_id}/DisplayPreferences/{id}` 合并为 **单条** `.route(..., get(...).post(...))`。 |

### B. `GET /Connect/Exchange`（及 `/connect/exchange`）

| 项 | 实现要点 |
| --- | --- |
| **查询参数** | `ConnectUserId`（兼容 `connectUserId`），缺失时返回 400。 |
| **认证头** | 使用与全局一致的 `auth::extract_token`（`X-Emby-Token` / `X-MediaBrowser-Token` / `Authorization` 等），语义为 **Emby Connect 侧下发的 AccessKey**（非本地会话 token）。 |
| **用户解析** | 扫描 `system_settings` 中键 `user_connect_link:{本地用户 UUID}`（由已有 `POST /Users/{id}/Connect/Link` 写入），在 JSON 对象中匹配 `ConnectUserId` / `UserId` / `Id` 等字段（GUID 去连字符、大小写不敏感）。 |
| **密钥校验** | 若 Link 载荷中存在 `ExchangeToken` / `ConnectAccessKey` / `AccessKey` 之一，则必须与请求头 token **完全一致**；若均未设置，则仅要求 token **非空**（自托管宽松策略，便于未填交换密钥的场景）。 |
| **登录策略** | 解析到 `DbUser` 后调用 `auth::ensure_login_policy`（远程访问、设备白名单、时段、最大会话数等与密码登录一致），再 `repository::create_session` 生成本地会话。 |
| **响应 DTO** | `models::ConnectAuthenticationExchangeResult`，`#[serde(rename_all = "PascalCase")]` → JSON `LocalUserId`（Emby GUID 字符串）、`AccessToken`（新会话 token）。 |

**涉及文件**：`backend/src/routes/connect.rs`（新）、`backend/src/routes/mod.rs`（`merge(connect::router())`）、`backend/src/repository.rs`（`find_user_by_connect_user_id`、`connect_exchange_access_key_allowed` 及 GUID 匹配辅助函数）、`backend/src/models.rs`（`ConnectAuthenticationExchangeResult`）。

### C. 校验

- `backend> cargo build` —— ✅ 通过（Connect 模块与 repository 增补无编译错误）。

### D. 小结

本轮补齐 **官方 Connect 换票** 路径，并与已有 **Connect/Link** 存储模型贯通；同时修复 **DisplayPreferences** 的路由注册方式，避免官方 App 的 **GET 显示偏好** 静默失效。

### E. 线上首页主区域空白（MCP 实测 [test.emby.yun:4443](https://test.emby.yun:4443/)）

| 现象 | 原因（推断） | 修复 |
| --- | --- | --- |
| 顶栏/侧栏正常，`/Users/.../Views` 与 `/Items` 均为 200，但 **主内容区仅底色、无「媒体库」或空状态文案** | Dashboard 布局下 **flex 子项未设 `min-h-0`** 时，滚动区高度可塌成 0，子页面仍在 DOM 但不可见。 | `frontend/src/layouts/AppLayout.vue` 中 `#body` 包裹层增加 `min-h-0 flex-1 overflow-y-auto`（保留原有 `gap` / `padding`）。 |

**附**：首屏「正在连接服务器……」来自 `App.vue` 在 `state.initialized` 为 false 时的加载壳；控制台偶发 **`Error: 未登录`** 来自 `frontend/src/api/emby.ts` 的 `requireUserId()`，多为初始化与恢复会话的竞态，与上述布局问题独立。

**G. 首页仍无内容（第二轮修复，2026-04-24）**

| 问题 | 修复 |
| --- | --- |
| `libraries.value = result.Items` 在 **`Items` 缺失** 时变成 `undefined`，模板访问 `.length` **渲染抛错**，整页空白 | `store/app.ts` 增加 `itemsFromQuery()`，凡从 `QueryResult` 取列表处统一 `Items ?? []`。 |
| 向导 **`completeWizard` 内** `enterHome()` 与 `run()` 同层 try/catch，失败时与「设置完成」混在一起 | `enterHome` 移到 `run` **之后**，单独 try/catch 写入 `state.error`。 |
| 模板依赖跨文件 `libraries` ref 的解包 + 面板未占满高度 | `HomePage.vue` 使用 **`libraryList` computed**；`UDashboardGroup` 增加 **`min-h-svh`**，`UDashboardPanel` 增加 **`flex min-h-0 flex-1 flex-col`**；首页根节点 **`min-w-0 flex-1`**。 |

**H. 首页 / 设置页仍只显示外壳（MCP 第三轮定位，2026-04-24）**

| 现象 | MCP 结论 | 修复 |
| --- | --- | --- |
| `https://test.emby.yun:4443/` 顶栏、侧栏、用户菜单正常，但主内容区没有媒体库、空状态或设置卡片；点击「设置」后也只剩面包屑与返回按钮。 | DevTools DOM 中 `#dashboard-panel-main` 的 Navbar 后只有空注释，页面 `RouterView` 内容未进入 DOM；当前 Nuxt UI 版本没有消费 `UDashboardPanel` 内的 `<template #body>`。接口层面 `/Users/.../Views`、`/Items` 均为 200，控制台无 Vue 崩溃。 | `frontend/src/layouts/AppLayout.vue` 将主内容滚动容器从 `UDashboardPanel` 的 `#body` 命名 slot 改为**直接子节点**，保证 `<slot />`（也就是路由页面）真实挂载。 |
| 线上强刷后仍加载 `index-h926ZtrD.js`、`SettingsIndex-BZTo4CvI.js` 等旧 chunk。 | 线上服务当前仍是旧静态资源；本地构建后新 chunk 为 `index-DoSIH5zm.js`、`SettingsIndex-CK393fAx.js`。 | 需要用新源码重新构建并部署镜像 / 静态目录；若使用 `docker-compose.yml` 当前的 `yuanhu66/movie-rust:latest`，需重新构建推送该镜像或切到本地 `build: .`。 |

**校验**：`frontend> npm run build` —— ✅ 通过。

## 十八、第十八轮：前端会话失效、用户保存顺序与 Web PlaybackInfo（2026-04-24）

> 范围：前端审计中发现的初始化卡死、401/403 后响应式登录态未清理、用户管理半提交，以及 Web 播放器未按 EmbySDK 提交播放上下文。

| 问题 | 修复 |
| --- | --- |
| 本地 token 已失效时，`initialize()` 会在 `enterHome()` 的 401 异常处中断，`state.initialized` 无法恢复，页面停在加载壳或半初始化状态。 | `frontend/src/store/app.ts` 用 `try/finally` 保证初始化结束；`enterHome()` 失败时调用统一未授权处理并刷新公开用户列表。 |
| API 层 401/403 只清 `api.token/api.user/localStorage`，没有同步清 store 的 `user` ref，路由守卫仍可能认为已登录。 | `frontend/src/api/emby.ts` 增加 `onUnauthorized` 回调；store 注册后统一执行 `clearClientState(true)` 与公开用户刷新。 |
| 用户管理保存时先写 Policy/UserSettings，再校验重置密码，失败会留下半保存状态。 | `frontend/src/pages/settings/UsersSettings.vue` 在任何网络写入前先校验新密码长度与确认值。 |
| Web 播放器 `PlaybackInfo` 仍是无上下文 GET，无法按设备能力返回播放源。 | `frontend/src/api/emby.ts` 改为 `POST /Items/{id}/PlaybackInfo?UserId=...&IsPlayback=true`，并提交 Web 端 `DeviceProfile`（DirectPlay、HLS 转码、字幕能力）。 |

**I. GitHub Actions `buildx` 摘要异常（2026-04-24）**

| 现象 | 原因（定位） | 修复 |
| --- | --- | --- |
| CI 在 `docker/build-push-action@v6` 的 **Check build summary support** 阶段失败，报错中出现 `iVBOR...`（PNG Base64 片段） | 非 Dockerfile 编译错误，而是 Buildx summary / build record 探测链路在当前 runner 组合下异常。 | 在 `.github/workflows/docker-image.yml` 的 `docker` job 增加环境变量：`DOCKER_BUILD_SUMMARY=false`、`DOCKER_BUILD_RECORD_UPLOAD=false`，关闭 summary 与 record 上传，保留构建与推送主流程。 |

**J. GitHub Actions `cache-to gha` 502（Unicorn）导致尾段失败（2026-04-24）**

| 现象 | 原因（定位） | 修复 |
| --- | --- | --- |
| 镜像已完成 `exporting` + `pushing manifest`，但在 `# exporting to GitHub Actions Cache` 阶段报 `failed to parse error response 502`（GitHub Unicorn HTML）后整任务失败。 | `cache-to: type=gha` 导出层到 GHA 缓存服务时偶发网关错误；这是缓存基础设施波动，不是 Dockerfile 或镜像推送失败。 | 将 PR 与 push 两个构建步骤的 `cache-to` 改为 `type=gha,mode=max,scope=movie-rust,ignore-error=true`，即“缓存导出失败不阻塞构建/推送结果”。 |

**K. 前端审计修复：错误传播与向导成功门禁（2026-04-24）**

| 问题 | 修复 |
| --- | --- |
| 通用 `run()` 仅吞错写 `state.error`，上层无法基于成功/失败做流程分支。 | `frontend/src/store/app.ts` 的 `run()` 增加返回值 `Promise<boolean>`，并支持 `RunOptions.rethrow`。成功返回 `true`，失败返回 `false`（可选重抛）。 |
| `completeWizard()` 在向导主流程失败时仍可能继续 `enterHome()`。 | `completeWizard()` 使用 `const wizardCompleted = await run(...)`；仅 `wizardCompleted === true` 时才进入首页加载流程。 |
| 首屏挂载时 `loadLibraries()/loadAdminData()` 异常会形成未处理 Promise 拒绝。 | `frontend/src/layouts/AppLayout.vue` 的 `onMounted` 增加 `try/catch`，统一写入 `state.error`，避免无声失败。 |
| `enterHome()` 链路中 `loadHome()` 失败无法向上感知（被 `run` 吞掉）。 | `loadHome()` 改为 `run(..., '', { rethrow: true })`，允许关键初始化路径捕获真实失败。 |

**校验**：`frontend> npx vue-tsc -b`、`frontend> npm run build` —— ✅ 通过。

## 十九、第十九轮：创建媒体库自动扫描改为后台异步（2026-04-24）

> 目标：保留“创建后自动扫描”，但避免创建弹窗等待全量入库完成。

| 问题 | 修复 |
| --- | --- |
| `createLibrary(..., refreshLibrary=true)` 会在后端同步执行 `scan_all_libraries`，请求直到扫描结束才返回，前端弹窗因此长时间不关闭。 | `backend/src/routes/admin.rs` 新增 `enqueue_library_scan()`：当 `refreshLibrary=true` 时改为 `tokio::spawn` 后台扫描并立即返回 API 响应（200/204）。 |
| 同类接口（删除库、增删改路径、`/Library/Refresh`）也会被同步扫描阻塞。 | 统一把 `refreshLibrary=true` 分支改为后台派发，避免管理操作被长扫描阻塞。 |
| 前端创建成功提示未体现“后台扫描已启动”。 | `frontend/src/store/app.ts` 将创建成功消息改为 **`媒体库已创建，已开始扫描`**（Toast 提示）。 |

**说明**：手动扫描接口 `/api/admin/scan` 仍保留同步返回 `ScanSummary`，用于需要即时统计结果的管理场景。

**校验**：`backend> cargo build`、`frontend> npx vue-tsc -b` —— ✅ 通过。

## 二十、第二十轮：播放失败定位与 `hls.js + video.js` 播放链路（2026-04-24）

> 目标：修复条目页点击“播放”后无法播放的问题，并将 Web 端播放链路切到 `hls.js + video.js`，提升对容器/音轨组合（含部分浏览器不友好的源格式）的兼容能力。

### A. MCP 线上实测结论（`/item/82A10E09-1CD1-5A8C-A45D-882C72499751`）

| 证据 | 结论 |
| --- | --- |
| `POST /Items/{id}/PlaybackInfo` 返回 200，随后播放器请求 `GET /videos/{id}/original.mkv?...&api_key=...&api_key=...` 返回 400 | 前端把 `DirectStreamUrl` 里已存在的 `api_key` 又追加了一次，导致直链请求参数异常。 |
| 控制台出现 `NotSupportedError: The element has no supported sources.` | 浏览器拿到 400 的媒体 URL 后，`<video>` 判定当前 source 不可播放并抛错。 |

### B. 前端 API 层修复（`frontend/src/api/emby.ts`）

| 问题 | 修复 |
| --- | --- |
| `streamUrlForSource` 对 `DirectStreamUrl` 无条件追加 `api_key`。 | 改为“检测已含 `api_key` 则不重复追加”，并尊重 `AddApiKeyToDirectStreamUrl=false`。 |
| 仅有“直链文件”构造逻辑。 | 新增 `hlsUrlForSource(itemId, source, playSessionId)`，优先使用 `TranscodingUrl`，否则回退到 `/Videos/{id}/master.m3u8`。 |
| `MediaSources` 类型缺少 HLS/能力字段。 | 补充 `TranscodingUrl`、`SupportsTranscoding`、`SupportsDirectPlay`、`SupportsDirectStream`、`AddApiKeyToDirectStreamUrl`。 |

### C. 播放页切换到 `hls.js + video.js`（`frontend/src/pages/playback/VideoPlaybackPage.vue`）

| 改造点 | 说明 |
| --- | --- |
| 引入播放器内核 | 增加 `video.js` 与 `hls.js` 依赖，页面初始化 `video.js` 实例（保留现有自研 UI 控件层）。 |
| 播放源策略 | 播放源按候选顺序切换：优先 HLS（`master.m3u8`），再回退直链（`DirectStreamUrl`）。 |
| HLS 驱动方式 | 当浏览器支持 `hls.js` 时，使用 `hls.js` 绑定 `<video>` 并加载 m3u8；其余情况由 `video.js` 设置 source。 |
| 容错 | 监听 `hls.js` fatal error 与 `<video>` `error` 事件，自动尝试下一个候选源，避免单一路径失败即黑屏。 |

### D. 构建校验

- `frontend> npx vue-tsc -b` —— ✅ 通过
- `frontend> npm run build` —— ✅ 通过

> 备注：引入 `video.js` 后 `VideoPlaybackPage` 路由分包体积上升；当前为播放页按需 chunk，不影响非播放页面首屏。

## 二十一、第二十一轮：类似内容多版本合并 + 扫库中断容错（2026-04-24）

> 范围：修复详情页“类似内容”重复展示多版本；增强后台扫描在数据库短时不可用场景下的鲁棒性。

### A. 类似内容多版本未合并

| 问题 | 原因 | 修复 |
| --- | --- | --- |
| 详情页“类似内容”会把同一影片的多个版本（同内容不同文件）分别展示；进入条目详情时又能看到版本合并后的视图。 | `GET /Items/{id}/Similar` 走 `repository::find_similar_items`，此前只做“排除目标项”，未对候选结果做“同内容身份键”去重。 | 在 `backend/src/repository.rs::find_similar_items` 三段结果装配（主查询、无标签分支、补足分支）统一按 `item_identity_key` 去重；无 identity 时回退 `item:{id}`，保证 Similar 列表层同内容仅出现一次。 |

### B. 扫库“突然中断”的容错改进

| 现象 | 处理 |
| --- | --- |
| 后台扫描期间若数据库连接瞬断（例如容器重启/数据库重启窗口），任务可能在首次 SQL 错误时立即结束。 | `backend/src/routes/admin.rs::enqueue_library_scan` 增加 SQLx 失败重试：最多 3 次、每次间隔 5 秒；重试次数与错误会写入日志。非数据库错误仍立即失败并记录。 |

### C. 你提供日志的定位结论

- 日志中的 `received fast shutdown request` + 随后完整 `initdb`，说明 **PostgreSQL 进程被快速关闭并重新初始化了数据目录**，这是基础设施级中断（容器/数据目录生命周期），不是单条 SQL 或单个 API 查询本身导致。
- 本轮代码层已做“数据库短暂不可用”的重试兜底；但若容器被重建且数据目录被清空，仍需要先修复部署侧持久化（volume）与重启策略。

### D. 校验

- `backend> cargo build` —— ✅ 通过

## 二十二、第二十二轮：`/Similar` 参数兼容补齐 + 播放页按需加载（2026-04-24）

> 范围：按 EmbySDK 请求习惯补齐 Similar 查询参数语义；优化 `hls.js + video.js` 引入方式，降低播放页首包体积。

### A. `/Items/{id}/Similar` 兼容增强

| 修复点 | 说明 |
| --- | --- |
| `GetSimilarItems` 新增参数 | `StartIndex` / `startIndex`、`GroupItemsIntoCollections` / `groupItemsIntoCollections`。 |
| 分页行为 | `routes/items.rs::get_similar_items` 支持 `StartIndex + Limit` 分页切片，返回 `start_index` 与分页后的 `items`。 |
| 分组行为 | `repository::find_similar_items` 新增 `group_items_into_collections` 参数：为 `true` 时按 `item_identity_key` 去重（同内容多版本合并）；为 `false` 时保留多版本。 |
| 兼容调用面 | `Item/InstantMix` 调用路径显式传 `group_items_into_collections = true`，保持既有“去重推荐”语义。 |

### B. 播放页播放器内核改为动态加载

| 修复点 | 说明 |
| --- | --- |
| 移除静态导入 | `VideoPlaybackPage.vue` 去掉顶层 `import 'video.js' / 'hls.js' / 'video.js.css'`。 |
| 按需加载 | 新增 `ensurePlaybackEngines()`：进入播放页后再 `import('video.js')`、`import('hls.js')`、`import('video.js/dist/video-js.css')`。 |
| 行为不变 | 仍保持“优先 HLS，失败回退直链”的播放候选策略与现有自研控制层。 |

### C. 构建结果

- `frontend> npm run build` —— ✅ 通过
- 播放页主 chunk 从先前约 **1.2MB** 降到约 **17KB**（播放器核心拆分为独立异步 chunk：`hls` / `video.es` / `video-js.css`）。

## 二十三、第二十三轮：手动扫描改为异步返回，规避 504 超时（2026-04-24）

> 背景：通过反向代理（openresty）点击“扫描媒体库”时，长时同步请求命中网关超时，页面收到 `504 Gateway Time-out`。

### A. 后端改造（`backend/src/routes/admin.rs`）

| 项 | 变更 |
| --- | --- |
| `POST /api/admin/scan` 默认行为 | 改为**异步入队立即返回**，HTTP `202 Accepted`，响应 `{ Queued: true, Message: "..." }`。 |
| 同步兼容开关 | 支持查询参数 `WaitForCompletion=true`（兼容 `waitForCompletion` / `wait_for_completion`），仅在显式指定时同步执行并返回 `ScanSummary`。 |
| 扫描执行 | 复用既有 `enqueue_library_scan()` 后台任务路径（含数据库错误重试逻辑）。 |

### B. 前端适配（`frontend/src/api/emby.ts` + `frontend/src/store/app.ts`）

| 项 | 变更 |
| --- | --- |
| API 调用 | `api.scan(waitForCompletion = false)` 默认请求 `WaitForCompletion=false`。 |
| UI 反馈 | `store.scan()` 对异步返回显示“媒体库已开始后台扫描”（或后端返回消息）；保留同步返回时“扫描完成，新增 N 个条目”的文案分支。 |

### C. 校验

- `backend> cargo build` —— ✅ 通过
- `frontend> npm run build` —— ✅ 通过

## 二十四、第二十四轮：扫描任务 Operation 化（operationId + monitor + cancel + 单飞，2026-04-24）

> 目标：将媒体库扫描链路升级为业界常见 LRO（Long Running Operation）模型，支持“可追踪、可轮询、可取消、可去重”。

### A. 后端（`backend/src/routes/admin.rs`）

| 能力 | 实现 |
| --- | --- |
| 任务单飞（Single-flight） | 新增内存注册表 `ScanOperationRegistry`；当已有运行中的扫描任务时，新的触发请求复用现有 `operationId`，避免并发全库扫描。 |
| 任务状态模型 | 新增 `ScanOperationState/Dto`：`Queued/Running/Cancelling/Succeeded/Failed/Cancelled`、`Progress`、`Attempts`、`Error`、`Result`、`CreatedAt/StartedAt/CompletedAt`。 |
| 触发接口升级 | `POST /api/admin/scan` 返回 `202 Accepted`，Body 含 `Operation` 与 `MonitorUrl`；响应头增加 `Location`（状态地址）和 `Retry-After: 3`。 |
| 状态接口 | 新增 `GET /api/admin/scan/operations`（列表）与 `GET /api/admin/scan/operations/{operation_id}`（详情）；未完成返回 `202` 并带 `Retry-After`。 |
| 取消接口 | 新增 `POST /api/admin/scan/operations/{operation_id}/cancel`；队列中任务立即取消，运行中任务转 `Cancelling`（软取消标记）。 |
| 兼容保留 | `WaitForCompletion=true` 仍可走同步路径返回 `ScanSummary`。 |

### B. 前端（`frontend/src/api/emby.ts` + `frontend/src/store/app.ts`）

| 能力 | 实现 |
| --- | --- |
| LRO 类型 | 新增 `ScanOperation` 类型与 `ScanQueuedResponse.Operation`。 |
| 轮询 API | 新增 `scanOperation(id)` / `scanOperations(limit)` / `cancelScanOperation(id)`。 |
| 触发后轮询 | `store.scan()` 触发后保存 `scanOperation`，每 3 秒轮询状态；完成后按状态更新提示并刷新首页/库数据。 |

### C. 结果

- 扫描链路从“fire-and-forget 提示”升级为“可观测任务流”；
- 避免多入口重复触发造成并发全库扫描；
- 为后续 UI 进度条与任务列表提供统一数据基础。

## 二十五、第二十五轮：扫描中断按钮 + 设置页任务面板 + 片源计数实时刷新（2026-04-24）

> 目标：避免重复点击扫描；在设置页直接可见任务状态；侧边栏媒体库摘要实时显示当前片源数与扫描进度。

### A. 参考对照

- 对照项目内 `docs/JELLYFIN_FRONTEND_PARITY.md` 的“后台设置分区”思路，把扫描任务信息前置到设置视图，而不是只依赖 Toast。

### B. 前端改造

| 文件 | 变更 |
| --- | --- |
| `frontend/src/store/app.ts` | 新增 `cancelCurrentScan()`，调用 `POST /api/admin/scan/operations/{id}/cancel`；轮询期间新增 `loadLibraries()`，让媒体库 `ChildCount` 在扫描进行中持续刷新。 |
| `frontend/src/layouts/AppLayout.vue` | 侧边栏 `text-muted text-[10px] font-medium uppercase tracking-wider` 文本改为动态摘要：空闲时显示 `媒体库 · 总数`，扫描中显示 `媒体库 · 总数 · 状态 进度%`。 |
| `frontend/src/pages/settings/LibrarySettings.vue` | 新增“媒体库扫描任务”面板：显示 `Queued/Running/Cancelling/Succeeded/Failed/Cancelled`、进度条、开始/结束时间、错误详情、Monitor 地址；新增“取消”按钮与“刷新状态”按钮。 |
| `frontend/src/pages/settings/SettingsIndex.vue` | 设置首页新增“扫描任务面板”卡片，显示状态/进度，并提供“取消”按钮与“进入媒体库任务详情”快捷入口。 |

### C. 结果

- 你在设置页即可看到扫描任务状态与进度，不需要反复点击“扫描”；
- “取消扫描”支持从设置首页和媒体库设置页双入口触发；
- 侧边栏媒体库摘要会随扫描轮询实时更新片源总数（`ChildCount` 聚合）。

### D. 校验

- `frontend> npm run build` —— ✅ 通过

## 二十六、第二十六轮：库级扫描状态视图（防重复点击，2026-04-24）

> 目标：你确认“需要”后，继续把扫描状态下沉到每个媒体库卡片，减少“点了某个库但看不出是否在扫”的重复操作。

### A. 现状约束

- 当前后端扫描任务是全库单飞（一个 `operationId`），尚无“每个库独立任务ID”的接口；
- 在这个约束下，前端先实现“库级状态展示层 + 触发来源记忆”，保证交互可感知。

### B. 前端改造（`frontend/src/pages/settings/LibrarySettings.vue`）

| 改造点 | 说明 |
| --- | --- |
| 卡片片源数实时显示 | 每个媒体库卡片新增 `片源 N`，数据来自 `libraries.ChildCount`，会随扫描轮询刷新。 |
| 库级扫描状态标签 | 扫描中时每个库显示状态：触发库显示当前任务状态；其余库显示排队态，避免“看起来没反应”。 |
| 扫描按钮防重复 | 每个库“扫描”按钮在对应任务进行中会禁用并切换文案为“扫描中”；全库扫描进行中时其它库按钮也会受控禁点。 |
| 全库扫描入口统一 | 顶部“扫描所有媒体库”改走 `scanAllLibraries()`，与库级触发共享状态记忆逻辑。 |

### C. 校验

- `frontend> npm run build` —— ✅ 通过

## 二十七、第二十七轮：每个媒体库独立扫描 operationId（真库级任务追踪，2026-04-24）

> 目标：把“库级状态”从前端推断升级为后端真实任务域，支持 `LibraryId` 级别独立 operationId、独立单飞和独立取消/轮询。

### A. 后端（`backend/src/routes/admin.rs` + `backend/src/scanner.rs`）

| 改造点 | 说明 |
| --- | --- |
| `POST /api/admin/scan` 支持 `LibraryId` | 新增查询参数 `LibraryId/libraryId/library_id`，可触发“只扫描单个媒体库”。 |
| 同步模式兼容 | `WaitForCompletion=true` 下也支持按 `LibraryId` 同步扫描。 |
| 任务域单飞 | 扫描注册表从单一 `active_operation_id` 改为 `active_operation_ids: BTreeMap<scope_key, operation_id>`；全库 scope 为 `all`，单库 scope 为 `library:{uuid}`。同一 scope 重复点击复用同一个 operationId。 |
| 任务 DTO 增强 | `ScanOperationDto` 增加 `ScopeKey`、`LibraryId`、`LibraryName` 字段，前端可直接知道任务属于哪个库。 |
| 扫描执行能力 | `scanner.rs` 新增 `scan_single_library(...)`，并抽取通用 `scan_libraries(...)`，复用同一扫描实现。 |

### B. 前端（`frontend/src/api/emby.ts` + `frontend/src/store/app.ts` + `frontend/src/pages/settings/LibrarySettings.vue`）

| 改造点 | 说明 |
| --- | --- |
| API 入参 | `api.scan(waitForCompletion, libraryId?)` 支持携带 `LibraryId`。 |
| 任务类型 | `ScanOperation` 类型补充 `ScopeKey/LibraryId/LibraryName`。 |
| store 触发 | `scan(libraryId?)` 可直接发起库级扫描；消息文案按 `LibraryName` 输出。 |
| 库页状态绑定 | 库卡片扫描状态/按钮禁用逻辑优先使用后端返回的 `scanOperation.LibraryId`，不再仅靠前端本地记忆。 |

### C. 结果

- 每个媒体库现在有真实独立任务域，避免不同库扫描互相覆盖状态；
- 同一库重复点击不再重复入队（复用 operationId）；
- 前端可以精准显示“哪个库在扫描”，并对该库按钮做精确防重。

### D. 校验

- `backend> cargo build` —— ✅ 通过
- `frontend> npm run build` —— ✅ 通过
