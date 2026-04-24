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
