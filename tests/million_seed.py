#!/usr/bin/env python3
"""百万片源数据生成 + 性能测试

在 PostgreSQL 中使用 generate_series 批量生成 1,000,000 条电影/剧集模拟数据，
然后对核心 API 端点进行性能压测。

用法: python tests/million_seed.py
"""

import json, sys, time, urllib.request, urllib.error, subprocess, statistics

BASE = "http://127.0.0.1:8096"
DB_URL = "postgres://movie:movie@localhost:5432/movie_rust"
TOKEN = ""
USER_ID = ""

# ── 工具 ─────────────────────────────────────────────────────────────

def api(method, path, body=None):
    url = BASE + path
    data = json.dumps(body).encode() if body else None
    headers = {"Content-Type": "application/json"}
    if TOKEN:
        headers["X-Emby-Token"] = TOKEN
    r = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        resp = urllib.request.urlopen(r, timeout=30)
        raw = resp.read()
        try:
            return json.loads(raw) if raw else {}, resp.status
        except (json.JSONDecodeError, ValueError):
            return {}, resp.status
    except urllib.error.HTTPError as e:
        return {}, e.code

def timed_api(method, path, body=None):
    t0 = time.perf_counter()
    result, code = api(method, path, body)
    elapsed = (time.perf_counter() - t0) * 1000
    return result, code, elapsed

def psql(sql):
    proc = subprocess.run(
        ["docker", "exec", "-i", "movie-test-pg",
         "psql", "-U", "movie", "-d", "movie_rust", "-c", sql],
        capture_output=True, text=True, timeout=600
    )
    return proc.stdout, proc.stderr, proc.returncode

# ══════════════════════════════════════════════════════════════════════
# Phase 1: 登录
# ══════════════════════════════════════════════════════════════════════
print("\n═══ Phase 1: 登录 ═══")

auth_header = 'MediaBrowser Client="PerfTest", Device="Python", DeviceId="perf-001", Version="1.0"'
for pw in ["TestPass123!", "NewTestPass456!"]:
    try:
        r = urllib.request.Request(
            BASE + "/Users/AuthenticateByName",
            data=json.dumps({"Name": "testadmin", "Pw": pw}).encode(),
            headers={"Content-Type": "application/json", "X-Emby-Authorization": auth_header},
            method="POST",
        )
        resp = urllib.request.urlopen(r, timeout=10)
        d = json.loads(resp.read())
        TOKEN = d.get("AccessToken", "")
        USER_ID = d.get("User", {}).get("Id", "")
        if TOKEN:
            break
    except Exception:
        continue

if not TOKEN:
    print("  ✗ 登录失败")
    sys.exit(1)
print(f"  ✓ 已登录 (UserId: {USER_ID[:12]}...)")

# ══════════════════════════════════════════════════════════════════════
# Phase 2: 创建媒体库 + 批量插入数据
# ══════════════════════════════════════════════════════════════════════
print("\n═══ Phase 2: 生成百万片源数据 ═══")

MOVIE_LIB_ID   = "a0000000-0000-4000-8000-000000000001"
SERIES_LIB_ID  = "a0000000-0000-4000-8000-000000000002"
MOVIE_COUNT    = 500_000
SERIES_COUNT   = 5_000
SEASON_PER     = 5
EP_PER_SEASON  = 20
TOTAL_EPISODES = SERIES_COUNT * SEASON_PER * EP_PER_SEASON  # 500,000
TOTAL          = MOVIE_COUNT + SERIES_COUNT + SERIES_COUNT * SEASON_PER + TOTAL_EPISODES

print(f"  目标: {MOVIE_COUNT:,} 电影 + {SERIES_COUNT:,} 剧集 × {SEASON_PER} 季 × {EP_PER_SEASON} 集")
print(f"  总计: {TOTAL:,} 条媒体记录")

# 创建两个库
lib_sql = f"""
INSERT INTO libraries (id, name, collection_type, path)
VALUES
  ('{MOVIE_LIB_ID}', '百万电影库', 'movies',  '/data/movies'),
  ('{SERIES_LIB_ID}', '百万剧集库', 'tvshows', '/data/tvshows')
ON CONFLICT DO NOTHING;
"""
psql(lib_sql)

# 清理之前的测试数据
print("  清理旧测试数据...")
psql(f"DELETE FROM media_items WHERE library_id IN ('{MOVIE_LIB_ID}', '{SERIES_LIB_ID}');")

# ── 2a: 50万电影 ──
print(f"  正在插入 {MOVIE_COUNT:,} 部电影...")
t0 = time.perf_counter()

genres_pool = [
    "动作", "喜剧", "剧情", "科幻", "悬疑", "恐怖", "爱情", "冒险",
    "奇幻", "动画", "犯罪", "纪录", "历史", "战争", "家庭", "音乐剧"
]
studios_pool = [
    "华纳兄弟", "迪士尼", "环球影业", "索尼影业", "派拉蒙",
    "二十世纪", "狮门影业", "A24", "Netflix", "Amazon",
    "中影集团", "博纳影业", "光线传媒", "万达影视", "北京文化"
]

movie_sql = f"""
INSERT INTO media_items (
    id, library_id, name, sort_name, item_type, media_type, path,
    container, overview, community_rating, runtime_ticks,
    premiere_date, production_year, official_rating,
    width, height, video_codec, audio_codec, bit_rate, size,
    genres, studios, provider_ids, is_movie, is_folder,
    image_tags, primary_image_tag, primary_image_aspect_ratio
)
SELECT
    gen_random_uuid(),
    '{MOVIE_LIB_ID}',
    '电影_' || lpad(i::text, 7, '0') || '_' ||
        (ARRAY['龙虎风云','星际穿越','盗梦空间','流浪地球','哪吒闹海',
               '大话西游','无间道','霸王别姬','功夫','让子弹飞',
               '少年的你','战狼','红海行动','我不是药神','哈利波特',
               '指环王','黑客帝国','阿凡达','泰坦尼克','疯狂动物城'])[1 + (i % 20)],
    'movie_' || lpad(i::text, 7, '0'),
    'Movie', 'Video',
    '/data/movies/movie_' || i || '/movie_' || i || '.mkv',
    'mkv',
    '这是第 ' || i || ' 部模拟电影的剧情简介。' ||
        '讲述了一个关于' || (ARRAY['冒险','爱情','悬疑','科幻','历史'])[1 + (i % 5)] ||
        '的精彩故事，由著名导演执导。' ||
        '影片获得了多项国际大奖提名。',
    3.0 + (random() * 7.0),
    (5400 + (random() * 7200)::int) * 10000000::bigint,
    ('2000-01-01'::date + (random() * 9125)::int),
    2000 + (i % 25),
    (ARRAY['PG', 'PG-13', 'R', 'G', 'NC-17'])[1 + (i % 5)],
    (ARRAY[1920, 3840, 1280, 1920, 3840])[1 + (i % 5)],
    (ARRAY[1080, 2160, 720, 1080, 2160])[1 + (i % 5)],
    (ARRAY['h264', 'hevc', 'av1', 'h264', 'hevc'])[1 + (i % 5)],
    (ARRAY['aac', 'dts', 'truehd', 'eac3', 'flac'])[1 + (i % 5)],
    (8000000 + (random() * 40000000)::int)::bigint,
    (1500000000 + (random() * 30000000000)::bigint),
    ARRAY[(ARRAY['{genres_pool[0]}','{genres_pool[1]}','{genres_pool[2]}','{genres_pool[3]}',
                 '{genres_pool[4]}','{genres_pool[5]}','{genres_pool[6]}','{genres_pool[7]}',
                 '{genres_pool[8]}','{genres_pool[9]}','{genres_pool[10]}','{genres_pool[11]}',
                 '{genres_pool[12]}','{genres_pool[13]}','{genres_pool[14]}','{genres_pool[15]}'])[1 + (i % 16)],
          (ARRAY['{genres_pool[0]}','{genres_pool[1]}','{genres_pool[2]}','{genres_pool[3]}',
                 '{genres_pool[4]}','{genres_pool[5]}','{genres_pool[6]}','{genres_pool[7]}',
                 '{genres_pool[8]}','{genres_pool[9]}','{genres_pool[10]}','{genres_pool[11]}',
                 '{genres_pool[12]}','{genres_pool[13]}','{genres_pool[14]}','{genres_pool[15]}'])[1 + ((i+3) % 16)]],
    ARRAY[(ARRAY['{studios_pool[0]}','{studios_pool[1]}','{studios_pool[2]}','{studios_pool[3]}',
                 '{studios_pool[4]}','{studios_pool[5]}','{studios_pool[6]}','{studios_pool[7]}',
                 '{studios_pool[8]}','{studios_pool[9]}','{studios_pool[10]}','{studios_pool[11]}',
                 '{studios_pool[12]}','{studios_pool[13]}','{studios_pool[14]}'])[1 + (i % 15)]],
    jsonb_build_object('Tmdb', (100000 + i)::text, 'Imdb', 'tt' || lpad((1000000+i)::text, 7, '0')),
    true, false,
    jsonb_build_object('Primary', 'tag_' || i),
    'tag_' || i,
    0.6667
FROM generate_series(1, {MOVIE_COUNT}) AS s(i);
"""

out, err, rc = psql(movie_sql)
elapsed_movie = time.perf_counter() - t0
if rc != 0:
    print(f"  ✗ 电影插入失败: {err[:300]}")
    sys.exit(1)
print(f"  ✓ {MOVIE_COUNT:,} 部电影已插入 ({elapsed_movie:.1f}s)")

# ── 2b: 5000剧集 ──
print(f"  正在插入 {SERIES_COUNT:,} 部剧集...")
t0 = time.perf_counter()

series_sql = f"""
INSERT INTO media_items (
    id, library_id, name, sort_name, item_type, media_type, path,
    overview, community_rating, production_year, official_rating,
    genres, studios, provider_ids,
    is_series, is_folder, child_count, season_count,
    status, image_tags, primary_image_tag, primary_image_aspect_ratio
)
SELECT
    ('b' || lpad(i::text, 7, '0') || '-0000-4000-8000-000000000000')::uuid,
    '{SERIES_LIB_ID}',
    '剧集_' || lpad(i::text, 5, '0') || '_' ||
        (ARRAY['权力游戏','绝命毒师','老友记','黑镜','怪奇物语',
               '纸牌屋','西部世界','切尔诺贝利','鱿鱼游戏','三体'])[1 + (i % 10)],
    'series_' || lpad(i::text, 5, '0'),
    'Series', 'Video',
    '/data/tvshows/series_' || i,
    '这是第 ' || i || ' 部模拟剧集的简介。跨越多季讲述了一段引人入胜的故事。',
    4.0 + (random() * 6.0),
    2010 + (i % 15),
    (ARRAY['TV-14', 'TV-MA', 'TV-PG', 'TV-G', 'TV-Y7'])[1 + (i % 5)],
    ARRAY['{genres_pool[0]}', '{genres_pool[2]}'],
    ARRAY['{studios_pool[8]}'],
    jsonb_build_object('Tmdb', (900000 + i)::text),
    true, true, {SEASON_PER * EP_PER_SEASON}, {SEASON_PER},
    (ARRAY['Continuing', 'Ended'])[1 + (i % 2)],
    jsonb_build_object('Primary', 'series_tag_' || i),
    'series_tag_' || i,
    0.6667
FROM generate_series(1, {SERIES_COUNT}) AS s(i);
"""
out, err, rc = psql(series_sql)
elapsed_series = time.perf_counter() - t0
print(f"  ✓ {SERIES_COUNT:,} 部剧集已插入 ({elapsed_series:.1f}s)")

# ── 2c: 季 ──
total_seasons = SERIES_COUNT * SEASON_PER
print(f"  正在插入 {total_seasons:,} 个季...")
t0 = time.perf_counter()

season_sql = f"""
INSERT INTO media_items (
    id, library_id, parent_id, name, sort_name, item_type, media_type, path,
    index_number, series_id, series_name, is_folder, child_count,
    production_year, image_tags, primary_image_tag
)
SELECT
    gen_random_uuid(),
    '{SERIES_LIB_ID}',
    ('b' || lpad(s::text, 7, '0') || '-0000-4000-8000-000000000000')::uuid,
    '第 ' || sn || ' 季',
    'series_' || lpad(s::text, 5, '0') || '_season_' || lpad(sn::text, 2, '0'),
    'Season', 'Video',
    '/data/tvshows/series_' || s || '/Season ' || sn,
    sn,
    ('b' || lpad(s::text, 7, '0') || '-0000-4000-8000-000000000000')::uuid,
    '剧集_' || lpad(s::text, 5, '0'),
    true, {EP_PER_SEASON},
    2010 + (s % 15) + sn - 1,
    jsonb_build_object('Primary', 'season_tag'),
    'season_tag'
FROM generate_series(1, {SERIES_COUNT}) AS gs(s),
     generate_series(1, {SEASON_PER}) AS gsn(sn);
"""
out, err, rc = psql(season_sql)
elapsed_season = time.perf_counter() - t0
print(f"  ✓ {total_seasons:,} 个季已插入 ({elapsed_season:.1f}s)")

# ── 2d: 50万集 ──
print(f"  正在插入 {TOTAL_EPISODES:,} 集剧集...")
t0 = time.perf_counter()

episode_sql = f"""
INSERT INTO media_items (
    id, library_id, name, sort_name, item_type, media_type, path,
    container, overview, community_rating, runtime_ticks,
    index_number, parent_index_number,
    series_id, series_name,
    production_year, video_codec, audio_codec,
    width, height, bit_rate, size,
    is_movie, is_folder,
    image_tags, primary_image_tag
)
SELECT
    gen_random_uuid(),
    '{SERIES_LIB_ID}',
    'S' || lpad(sn::text, 2, '0') || 'E' || lpad(ep::text, 2, '0') || ' 第' || ep || '集',
    'series_' || lpad(s::text, 5, '0') || '_s' || lpad(sn::text, 2, '0') || '_e' || lpad(ep::text, 2, '0'),
    'Episode', 'Video',
    '/data/tvshows/series_' || s || '/Season ' || sn || '/S' || lpad(sn::text,2,'0') || 'E' || lpad(ep::text,2,'0') || '.mkv',
    'mkv',
    '剧集 ' || s || ' 第' || sn || '季第' || ep || '集的剧情简介。',
    4.0 + (random() * 6.0),
    (2400 + (random() * 3600)::int) * 10000000::bigint,
    ep, sn,
    ('b' || lpad(s::text, 7, '0') || '-0000-4000-8000-000000000000')::uuid,
    '剧集_' || lpad(s::text, 5, '0'),
    2010 + (s % 15) + sn - 1,
    (ARRAY['h264', 'hevc'])[1 + ((s+ep) % 2)],
    (ARRAY['aac', 'eac3'])[1 + ((s+sn) % 2)],
    1920, 1080,
    (5000000 + (random() * 15000000)::int)::bigint,
    (800000000 + (random() * 5000000000)::bigint),
    false, false,
    jsonb_build_object('Primary', 'ep_tag'),
    'ep_tag'
FROM generate_series(1, {SERIES_COUNT}) AS gs(s),
     generate_series(1, {SEASON_PER}) AS gsn(sn),
     generate_series(1, {EP_PER_SEASON}) AS gep(ep);
"""
out, err, rc = psql(episode_sql)
elapsed_ep = time.perf_counter() - t0
if rc != 0:
    print(f"  ✗ 剧集插入失败: {err[:300]}")
else:
    print(f"  ✓ {TOTAL_EPISODES:,} 集已插入 ({elapsed_ep:.1f}s)")

# 确认总数
out, _, _ = psql("SELECT count(*) FROM media_items;")
total_in_db = out.strip().split("\n")[-2].strip() if out else "?"
print(f"\n  数据库总记录: {total_in_db}")

total_insert_time = elapsed_movie + elapsed_series + elapsed_season + elapsed_ep
print(f"  总插入耗时: {total_insert_time:.1f}s")

# ══════════════════════════════════════════════════════════════════════
# Phase 3: API 性能测试
# ══════════════════════════════════════════════════════════════════════
print("\n═══ Phase 3: API 性能测试 ═══")

results = []

def bench(label, method, path, runs=5):
    times = []
    last_data = None
    for _ in range(runs):
        data, code, ms = timed_api(method, path)
        times.append(ms)
        last_data = data
    avg = statistics.mean(times)
    med = statistics.median(times)
    mn = min(times)
    mx = max(times)
    total_count = ""
    if isinstance(last_data, dict) and "TotalRecordCount" in last_data:
        total_count = f" (共 {last_data['TotalRecordCount']:,} 条)"
    elif isinstance(last_data, dict) and "Items" in last_data:
        total_count = f" ({len(last_data.get('Items', []))} 项)"

    status = "✓" if avg < 2000 else ("⚠" if avg < 5000 else "✗")
    print(f"  {status} {label}: avg={avg:.0f}ms med={med:.0f}ms min={mn:.0f}ms max={mx:.0f}ms{total_count}")
    results.append({"label": label, "avg": avg, "med": med, "min": mn, "max": mx})

# ── 3.1 基础端点 ──
print("\n  ── 基础端点 ──")
bench("GET /System/Info/Public", "GET", "/System/Info/Public")
bench("GET /System/Info", "GET", "/System/Info")
bench("GET /Features", "GET", "/Features")

# ── 3.2 媒体库列表 ──
print("\n  ── 媒体库/Views ──")
bench("GET /Users/{id}/Views", "GET", f"/Users/{USER_ID}/Views")

# ── 3.3 分页查询 ──
print("\n  ── 分页查询（核心性能） ──")
bench("Items 首页 Limit=50",
      "GET", f"/Users/{USER_ID}/Items?Recursive=true&Limit=50&SortBy=DateCreated&SortOrder=Descending&Fields=PrimaryImageAspectRatio")
bench("Items Limit=100",
      "GET", f"/Users/{USER_ID}/Items?Recursive=true&Limit=100&SortBy=DateCreated&SortOrder=Descending")
bench("Items Limit=200",
      "GET", f"/Users/{USER_ID}/Items?Recursive=true&Limit=200&SortBy=DateCreated&SortOrder=Descending")
bench("Items Offset=500000 Limit=50",
      "GET", f"/Users/{USER_ID}/Items?Recursive=true&Limit=50&StartIndex=500000&SortBy=SortName&SortOrder=Ascending")
bench("Movie 过滤 Limit=50",
      "GET", f"/Users/{USER_ID}/Items?IncludeItemTypes=Movie&Recursive=true&Limit=50&SortBy=SortName")
bench("Episode 过滤 Limit=50",
      "GET", f"/Users/{USER_ID}/Items?IncludeItemTypes=Episode&Recursive=true&Limit=50&SortBy=SortName")
bench("Series 过滤 Limit=50",
      "GET", f"/Users/{USER_ID}/Items?IncludeItemTypes=Series&Recursive=true&Limit=50&SortBy=SortName")

# ── 3.4 搜索 ──
print("\n  ── 搜索 ──")
bench("搜索 '龙虎' Limit=50",
      "GET", f"/Users/{USER_ID}/Items?SearchTerm=%E9%BE%99%E8%99%8E&Recursive=true&Limit=50")
bench("搜索 '星际' Limit=50",
      "GET", f"/Users/{USER_ID}/Items?SearchTerm=%E6%98%9F%E9%99%85&Recursive=true&Limit=50")
bench("搜索 '第3集' Limit=50",
      "GET", f"/Users/{USER_ID}/Items?SearchTerm=%E7%AC%AC3%E9%9B%86&Recursive=true&Limit=50")
bench("搜索 'movie_0099' (精确)",
      "GET", f"/Users/{USER_ID}/Items?SearchTerm=movie_0099&Recursive=true&Limit=50")

# ── 3.5 排序 ──
print("\n  ── 排序 ──")
bench("SortBy=CommunityRating DESC Limit=50",
      "GET", f"/Users/{USER_ID}/Items?IncludeItemTypes=Movie&Recursive=true&Limit=50&SortBy=CommunityRating&SortOrder=Descending")
bench("SortBy=PremiereDate DESC Limit=50",
      "GET", f"/Users/{USER_ID}/Items?IncludeItemTypes=Movie&Recursive=true&Limit=50&SortBy=PremiereDate&SortOrder=Descending")
bench("SortBy=DateCreated DESC Limit=50",
      "GET", f"/Users/{USER_ID}/Items?IncludeItemTypes=Movie&Recursive=true&Limit=50&SortBy=DateCreated&SortOrder=Descending")

# ── 3.6 流派/人物 ──
print("\n  ── 流派/人物 ──")
bench("GET /Genres", "GET", "/Genres")
bench("GET /Persons", "GET", "/Persons")

# ── 3.7 Trickplay / MediaSegments ──
print("\n  ── 新功能端点 ──")
bench("GET /Items/{fake}/Trickplay",
      "GET", "/Items/00000000000000000000000000000000/Trickplay")
bench("GET /MediaSegments/{fake}",
      "GET", "/MediaSegments/00000000000000000000000000000000")

# ── 3.8 计划任务 / 会话 ──
print("\n  ── 管理端点 ──")
bench("GET /ScheduledTasks", "GET", "/ScheduledTasks")
bench("GET /Sessions", "GET", "/Sessions")
bench("GET /Devices", "GET", "/Devices")

# ══════════════════════════════════════════════════════════════════════
# Phase 4: 总结
# ══════════════════════════════════════════════════════════════════════
print(f"\n{'═' * 70}")
print(f"  百万片源性能测试报告")
print(f"{'═' * 70}")
print(f"  数据库记录总数: {total_in_db}")
print(f"  数据插入耗时:   {total_insert_time:.1f}s")
print(f"{'─' * 70}")

slow = [r for r in results if r["avg"] >= 2000]
warn = [r for r in results if 1000 <= r["avg"] < 2000]
fast = [r for r in results if r["avg"] < 1000]
very_fast = [r for r in results if r["avg"] < 200]

print(f"  <200ms (极快):  {len(very_fast)}/{len(results)} 项")
print(f"  <1000ms (良好): {len(fast)}/{len(results)} 项")
print(f"  1-2s (可接受):  {len(warn)}/{len(results)} 项")
print(f"  >2s (需优化):   {len(slow)}/{len(results)} 项")

if slow:
    print(f"\n  ⚠ 慢查询:")
    for r in slow:
        print(f"    - {r['label']}: avg={r['avg']:.0f}ms")

if warn:
    print(f"\n  ⓘ 中等耗时:")
    for r in warn:
        print(f"    - {r['label']}: avg={r['avg']:.0f}ms")

overall_avg = statistics.mean([r["avg"] for r in results])
print(f"\n  全部端点平均响应: {overall_avg:.0f}ms")
print(f"{'═' * 70}")

if slow:
    print(f"\n  结论: 存在 {len(slow)} 个慢端点需要优化")
    sys.exit(1)
else:
    print(f"\n  结论: 所有端点在百万数据量下表现良好!")
    sys.exit(0)
