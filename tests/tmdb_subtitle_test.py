#!/usr/bin/env python3
"""TMDB 元数据获取 + 字幕获取 测试脚本"""

import json, sys, time, uuid, urllib.request, urllib.error

BASE = "http://127.0.0.1:8096"
TOKEN = ""
USER_ID = ""

def api(method, path, body=None, timeout=30):
    url = BASE + path
    data = json.dumps(body).encode() if body else None
    headers = {"Content-Type": "application/json"}
    if TOKEN:
        headers["X-Emby-Token"] = TOKEN
    r = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        resp = urllib.request.urlopen(r, timeout=timeout)
        raw = resp.read()
        try:
            return json.loads(raw) if raw else {}, resp.status
        except (json.JSONDecodeError, ValueError):
            return {}, resp.status
    except urllib.error.HTTPError as e:
        body_text = ""
        try:
            body_text = e.read().decode()
        except Exception:
            pass
        print(f"  ✗ HTTP {e.code}: {body_text[:200]}")
        return {}, e.code
    except Exception as e:
        print(f"  ✗ 异常: {e}")
        return {}, 0

def login():
    global TOKEN, USER_ID
    auth_header = 'MediaBrowser Client="Test", Device="CLI", DeviceId="tmdb-test", Version="1.0"'
    for pw in ["TestPass123!", "NewTestPass456!", ""]:
        try:
            req = urllib.request.Request(
                BASE + "/Users/AuthenticateByName",
                data=json.dumps({"Username": "testadmin", "Pw": pw}).encode(),
                headers={"Content-Type": "application/json", "X-Emby-Authorization": auth_header},
                method="POST"
            )
            resp = urllib.request.urlopen(req, timeout=10)
            data = json.loads(resp.read())
            TOKEN = data["AccessToken"]
            USER_ID = data["User"]["Id"]
            print(f"✓ 已登录 (User: {USER_ID[:10]}...)")
            return
        except Exception:
            continue
    print("✗ 登录失败")
    sys.exit(1)

# ─── Step 1: 插入真实电影用于测试 ───
def insert_test_movies():
    """插入几部真实电影到数据库"""
    import subprocess
    movies = [
        {
            "id": "00000000-0000-4000-9000-000000000001",
            "name": "The Matrix",
            "sort_name": "Matrix",
            "year": 1999,
            "path": "/movies/The Matrix (1999)/The Matrix (1999).mkv",
            "overview": "A computer hacker learns from mysterious rebels about the true nature of his reality."
        },
        {
            "id": "00000000-0000-4000-9000-000000000002",
            "name": "Inception",
            "sort_name": "Inception",
            "year": 2010,
            "path": "/movies/Inception (2010)/Inception (2010).mkv",
            "overview": "A thief who steals corporate secrets through dream-sharing technology."
        },
        {
            "id": "00000000-0000-4000-9000-000000000003",
            "name": "Interstellar",
            "sort_name": "Interstellar",
            "year": 2014,
            "path": "/movies/Interstellar (2014)/Interstellar (2014).mkv",
            "overview": "A team of explorers travel through a wormhole in space."
        },
        {
            "id": "00000000-0000-4000-9000-000000000004",
            "name": "Parasite",
            "sort_name": "Parasite",
            "year": 2019,
            "path": "/movies/Parasite (2019)/Parasite (2019).mkv",
            "overview": "Greed and class discrimination threaten the newly formed symbiotic relationship."
        },
        {
            "id": "00000000-0000-4000-9000-000000000005",
            "name": "Spirited Away",
            "sort_name": "Spirited Away",
            "year": 2001,
            "path": "/movies/Spirited Away (2001)/Spirited Away (2001).mkv",
            "overview": "During her family's move to the suburbs, a sullen 10-year-old girl wanders into a world of spirits."
        }
    ]

    for m in movies:
        sql = f"""
        INSERT INTO media_items (
            id, parent_id, name, sort_name, item_type, media_type, path,
            overview, production_year, container, library_id,
            genres, studios, tags, production_locations, air_days,
            backdrop_paths, remote_trailers, provider_ids,
            date_created, date_modified, image_blur_hashes
        ) VALUES (
            '{m["id"]}', NULL, '{m["name"]}', '{m["sort_name"]}', 'Movie', 'Video',
            '{m["path"]}', '{m["overview"]}', {m["year"]}, 'mkv',
            'a0000000-0000-4000-8000-000000000001',
            ARRAY['Action','Sci-Fi'], ARRAY['Warner Bros'], ARRAY[]::text[], ARRAY[]::text[], ARRAY[]::text[],
            ARRAY[]::text[], ARRAY[]::text[], '{{}}'::jsonb,
            NOW(), NOW(), '{{}}'::jsonb
        ) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, production_year = EXCLUDED.production_year;
        """
        proc = subprocess.run(
            ["docker", "exec", "-i", "movie-test-pg", "psql", "-U", "movie", "-d", "movie_rust", "-c", sql],
            capture_output=True, text=True, timeout=10
        )
        if proc.returncode == 0:
            print(f"  ✓ 插入: {m['name']} ({m['year']})")
        else:
            print(f"  ✗ 插入失败 {m['name']}: {proc.stderr[:100]}")

# ─── Step 2: TMDB 元数据测试 ───
def test_tmdb_search():
    """测试 TMDB 搜索功能"""
    print("\n── TMDB 搜索测试 ──")

    # 搜索电影
    searches = [
        ("Movie", "The Matrix", 1999),
        ("Movie", "Inception", 2010),
        ("Movie", "Interstellar", 2014),
        ("Movie", "Parasite", 2019),
        ("Movie", "Spirited Away", 2001),
    ]

    for item_type, name, year in searches:
        t0 = time.perf_counter()
        path = f"/Items/RemoteSearch/{item_type}?SearchInfo.Name={urllib.request.quote(name)}&SearchInfo.Year={year}"
        result, code = api("POST", path, body={
            "SearchInfo": {"Name": name, "Year": year},
            "IncludeDisabledProviders": False
        })
        elapsed = (time.perf_counter() - t0) * 1000

        if code == 200 and isinstance(result, list) and len(result) > 0:
            top = result[0]
            tmdb_name = top.get("Name", "?")
            tmdb_year = top.get("ProductionYear", "?")
            tmdb_id = top.get("ProviderIds", {}).get("Tmdb", "?")
            print(f"  ✓ 搜索 '{name}': 找到 {len(result)} 个结果, 首选: \"{tmdb_name}\" ({tmdb_year}) [TMDB:{tmdb_id}] ({elapsed:.0f}ms)")
        else:
            print(f"  ✗ 搜索 '{name}': code={code}, results={len(result) if isinstance(result, list) else 'N/A'} ({elapsed:.0f}ms)")

def test_tmdb_identify():
    """测试 TMDB 识别（Identify）功能 - 将搜索结果应用到条目"""
    print("\n── TMDB 识别/刷新元数据测试 ──")

    test_items = [
        ("00000000-0000-4000-9000-000000000001", "The Matrix"),
        ("00000000-0000-4000-9000-000000000002", "Inception"),
        ("00000000-0000-4000-9000-000000000003", "Interstellar"),
    ]

    for item_id, name in test_items:
        # 刷新元数据
        t0 = time.perf_counter()
        emby_id = item_id.replace("-", "").upper()[:32]
        path = f"/Items/{emby_id}/Refresh?Recursive=false&MetadataRefreshMode=FullRefresh&ImageRefreshMode=FullRefresh&ReplaceAllMetadata=true&ReplaceAllImages=true"
        result, code = api("POST", path, timeout=60)
        elapsed = (time.perf_counter() - t0) * 1000
        if code in (200, 204):
            print(f"  ✓ 刷新 '{name}': 成功 ({elapsed:.0f}ms)")
        else:
            print(f"  ✗ 刷新 '{name}': code={code} ({elapsed:.0f}ms)")

        time.sleep(1)

    # 等待刷新完成
    time.sleep(3)

    # 验证元数据是否更新
    print("\n  验证元数据更新:")
    for item_id, name in test_items:
        emby_id = item_id.replace("-", "").upper()[:32]
        path = f"/Users/{USER_ID}/Items/{emby_id}"
        result, code = api("GET", path)
        if code == 200:
            updated_name = result.get("Name", "?")
            overview = result.get("Overview", "")[:60]
            year = result.get("ProductionYear", "?")
            rating = result.get("CommunityRating", "?")
            providers = result.get("ProviderIds", {})
            tmdb_id = providers.get("Tmdb", "N/A")
            imdb_id = providers.get("Imdb", "N/A")
            genres = result.get("Genres", [])
            print(f"  ✓ {updated_name} ({year})")
            print(f"    评分: {rating}, TMDB: {tmdb_id}, IMDB: {imdb_id}")
            print(f"    类型: {', '.join(genres[:5])}")
            print(f"    简介: {overview}...")
        else:
            print(f"  ✗ 获取 '{name}' 详情失败: code={code}")

# ─── Step 3: 字幕搜索测试 ───
def test_subtitle_search():
    """测试字幕搜索功能"""
    print("\n── 字幕搜索测试 (OpenSubtitles) ──")

    test_items = [
        ("00000000-0000-4000-9000-000000000001", "The Matrix"),
        ("00000000-0000-4000-9000-000000000002", "Inception"),
        ("00000000-0000-4000-9000-000000000003", "Interstellar"),
    ]

    for item_id, name in test_items:
        emby_id = item_id.replace("-", "").upper()[:32]
        # 搜索字幕
        t0 = time.perf_counter()
        path = f"/Items/{emby_id}/RemoteSearch/Subtitles/chi?IsPerfectMatch=false"
        result, code = api("GET", path, timeout=30)
        elapsed = (time.perf_counter() - t0) * 1000

        if code == 200 and isinstance(result, list):
            print(f"  ✓ '{name}' 字幕搜索: {len(result)} 个结果 ({elapsed:.0f}ms)")
            for sub in result[:3]:
                sub_name = sub.get("Name", "?")
                sub_lang = sub.get("Language", "?")
                sub_format = sub.get("Format", "?")
                sub_provider = sub.get("ProviderName", "?")
                print(f"    - [{sub_lang}] {sub_name} ({sub_format}) by {sub_provider}")
        else:
            print(f"  ✗ '{name}' 字幕搜索失败: code={code} ({elapsed:.0f}ms)")

    # 也测试英文字幕搜索
    print("\n  英文字幕搜索:")
    emby_id = "00000000-0000-4000-9000-000000000001".replace("-", "").upper()[:32]
    t0 = time.perf_counter()
    path = f"/Items/{emby_id}/RemoteSearch/Subtitles/eng?IsPerfectMatch=false"
    result, code = api("GET", path, timeout=30)
    elapsed = (time.perf_counter() - t0) * 1000
    if code == 200 and isinstance(result, list):
        print(f"  ✓ 'The Matrix' 英文字幕: {len(result)} 个结果 ({elapsed:.0f}ms)")
        for sub in result[:3]:
            print(f"    - [{sub.get('Language','?')}] {sub.get('Name','?')} ({sub.get('Format','?')})")
    else:
        print(f"  ✗ 英文字幕搜索失败: code={code} ({elapsed:.0f}ms)")

def test_subtitle_providers():
    """测试字幕提供者配置"""
    print("\n── 字幕提供者配置 ──")
    result, code = api("GET", "/api/subtitle/providers")
    if code == 200:
        if isinstance(result, list):
            for p in result:
                print(f"  - {p.get('Name', '?')}: enabled={p.get('Enabled', '?')}")
        else:
            print(f"  配置: {json.dumps(result, ensure_ascii=False)[:200]}")
    else:
        print(f"  ✗ 获取字幕提供者失败: code={code}")

    # 也尝试 Emby 风格的字幕 providers 端点
    result2, code2 = api("GET", "/Providers/Subtitles")
    if code2 == 200:
        print(f"  ✓ Emby字幕提供者: {json.dumps(result2, ensure_ascii=False)[:200]}")

# ─── 主流程 ───
if __name__ == "__main__":
    print("=" * 60)
    print("  TMDB 元数据 + 字幕获取 综合测试")
    print("=" * 60)

    login()

    print("\n── Step 1: 插入真实电影测试数据 ──")
    insert_test_movies()

    test_tmdb_search()
    test_tmdb_identify()
    test_subtitle_providers()
    test_subtitle_search()

    print("\n" + "=" * 60)
    print("  测试完成")
    print("=" * 60)
