# 百万片源性能测试报告

**测试日期**: 2026-04-29  
**测试环境**: Windows 10 (10.0.26200), Rust backend + PostgreSQL 16 (Docker), Vue 前端  
**数据规模**: 1,030,000 条媒体项

---

## 一、测试数据概述

| 数据类别 | 数量 |
|---------|------|
| 电影 (Movie) | 500,000 |
| 剧集系列 (Series) | 5,000 |
| 季 (Season) | 25,000 |
| 集 (Episode) | 500,000 |
| 媒体库 | 2 (百万电影库 + 百万剧集库) |
| **总计** | **1,030,000** |

每个条目包含完整元数据：名称、年份、评分、分辨率、格式、genres、studios、tags 等。

---

## 二、优化措施

### 2.1 数据库索引优化
- B-tree 索引：`date_created DESC`、`community_rating DESC`、`(item_type, sort_name)`、`(item_type, date_created DESC)`
- GIN 三元组索引（pg_trgm）：`name`、`sort_name` — 加速 ILIKE 模糊搜索

### 2.2 COUNT 查询优化
- 无过滤条件：使用 `pg_class.reltuples` 估算值（~0ms）
- library_id/item_type 过滤：独立 `SELECT COUNT(*)` + 索引扫描
- 搜索查询：`LIMIT 10000` 子查询截断估算，避免全表扫描

### 2.3 N+1 查询消除（核心优化）
**优化前**：每个 DTO 转换调用 3-5 次额外 DB 查询
- `media_sources_for_item()` — 获取流信息
- `get_item_people()` — 获取演员/导演
- `get_media_item(parent_id)` — 获取父级条目
- `get_media_item(series_uuid)` — 获取系列条目
- `metadata_preferences_from_settings()` — 获取配置

200 条列表 = 600-1000 次额外查询 → **53 秒**

**优化后**：`media_item_to_dto_for_list()` 零额外查询
- 仅使用 `DbMediaItem` 自身字段 + 预取的 `UserData` + 预取的 `Counts`
- 跳过列表不需要的 media_sources、people、parent 查询
- 200 条列表 = 0 次额外查询 → **<1 秒**

### 2.4 搜索条件精简
- 搜索字段从 5 个（name, sort_name, original_title, series_name, overview）缩减至 2 个（name, sort_name）
- 仅搜索有 pg_trgm GIN 索引的字段，避免全表顺序扫描

---

## 三、API 性能测试结果

### 全部 21 个端点测试结果：

| 端点 | 平均耗时 | 中位数 | 最小 | 最大 | 评级 |
|-----|---------|--------|------|------|------|
| **基础端点** |
| GET /System/Info/Public | 17ms | 16ms | 15ms | 22ms | ⚡ 极快 |
| GET /System/Info | 15ms | 12ms | 11ms | 23ms | ⚡ 极快 |
| **媒体库** |
| GET /Users/{id}/Views | 579ms | 559ms | 554ms | 665ms | ✅ 良好 |
| **分页查询（核心）** |
| Items 首页 Limit=50 | 615ms | 618ms | 574ms | 652ms | ✅ 良好 |
| Items Limit=100 | 622ms | 619ms | 609ms | 638ms | ✅ 良好 |
| Items Limit=200 | 647ms | 648ms | 615ms | 676ms | ✅ 良好 |
| Items Offset=500000 Limit=50 | 436ms | 428ms | 407ms | 490ms | ✅ 良好 |
| Movie 过滤 Limit=50 | 279ms | 275ms | 258ms | 310ms | ✅ 良好 |
| Episode 过滤 Limit=50 | 571ms | 560ms | 557ms | 601ms | ✅ 良好 |
| Series 过滤 Limit=50 | 622ms | 580ms | 543ms | 801ms | ✅ 良好 |
| **搜索** |
| 搜索 '龙虎' Limit=50 | 473ms | 335ms | 226ms | 794ms | ✅ 良好 |
| 搜索 '星际' Limit=50 | 458ms | 238ms | 224ms | 819ms | ✅ 良好 |
| 搜索 '第3集' Limit=50 | 679ms | 678ms | 654ms | 695ms | ✅ 良好 |
| 搜索 'movie_0099' (精确) | 150ms | 149ms | 140ms | 159ms | ⚡ 极快 |
| **排序** |
| SortBy=CommunityRating DESC | 264ms | 256ms | 237ms | 296ms | ✅ 良好 |
| SortBy=PremiereDate DESC | 886ms | 847ms | 822ms | 1000ms | ✅ 良好 |
| SortBy=DateCreated DESC | 776ms | 748ms | 725ms | 896ms | ✅ 良好 |
| **流派/人物** |
| GET /Genres | 356ms | 357ms | 345ms | 372ms | ✅ 良好 |
| GET /Persons | 22ms | 9ms | 7ms | 55ms | ⚡ 极快 |
| **管理端点** |
| GET /ScheduledTasks | 38ms | 34ms | 33ms | 56ms | ⚡ 极快 |
| GET /Sessions | 90ms | 92ms | 81ms | 101ms | ⚡ 极快 |

### 分布统计

| 等级 | 标准 | 数量 | 比例 |
|-----|------|------|------|
| ⚡ 极快 | <200ms | 6 | 28.6% |
| ✅ 良好 | <1000ms | 15 | 71.4% |
| ⚠️ 可接受 | 1-2s | 0 | 0% |
| ❌ 需优化 | >2s | 0 | 0% |

**全部端点平均响应时间: 409ms**

---

## 四、优化前后对比

| 指标 | 优化前 | 优化后 | 提升倍数 |
|-----|--------|--------|---------|
| Items Limit=50 | 13,750ms | 615ms | **22x** |
| Items Limit=100 | 27,007ms | 622ms | **43x** |
| Items Limit=200 | 52,979ms | 647ms | **82x** |
| 全部端点平均 | ~1,400ms | 409ms | **3.4x** |
| >2s 端点数 | 3+ | 0 | **100%消除** |

关键发现：响应时间不再随 Limit 线性增长（50→200 仅增加 32ms），证明 N+1 问题已完全消除。

---

## 五、UI 渲染测试结果

| 页面 | 状态 | 说明 |
|-----|------|------|
| 首页 | ✅ 正常 | 侧栏显示 "媒体库 · 1030000"，最近添加卡片正常渲染 |
| 百万电影库 | ✅ 正常 | 72/500000 个条目，卡片带 4K/HD 标签、年份、格式 |
| 百万剧集库 | ✅ 正常 | 72/5000 个条目，剧集系列和季数正常显示 |
| 搜索页（"龙虎"） | ✅ 正常 | 搜索 200 项结果秒级返回，卡片流畅渲染 |

---

## 六、已知限制与后续优化方向

1. **搜索计数估算**: 搜索结果使用 LIMIT 10000 估算总数，超大结果集显示 "10000" 而非精确值
2. **Views 端点**: ~500ms 响应，可缓存优化
3. **Genres 端点**: ~350ms，可增加缓存
4. **PremiereDate 排序**: ~886ms，接近 1s 阈值，可考虑增加复合索引
5. **列表 DTO 简化**: 列表视图不返回 media_sources/people/chapters，详情页仍使用完整 DTO

---

## 七、结论

百万级片源场景下，系统表现优秀：
- **所有 21 个 API 端点均在 1 秒内响应**
- **核心分页/搜索端点平均 ~500ms**
- **UI 首页/媒体库/搜索均可正常使用**
- **N+1 查询优化带来 22-82 倍性能提升**

系统已具备支持百万级媒体库的能力。
