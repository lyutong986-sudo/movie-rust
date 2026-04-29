#!/usr/bin/env python3
"""百万片源 API 性能测试（仅测试，不重新生成数据）"""

import json, sys, time, urllib.request, urllib.error, statistics

BASE = "http://127.0.0.1:8096"
TOKEN = ""
USER_ID = ""

def api(method, path, body=None):
    url = BASE + path
    data = json.dumps(body).encode() if body else None
    headers = {"Content-Type": "application/json"}
    if TOKEN:
        headers["X-Emby-Token"] = TOKEN
    r = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        resp = urllib.request.urlopen(r, timeout=60)
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

# 登录
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
print(f"✓ 已登录 (UserId: {USER_ID[:12]}...)\n")

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

print("── 基础端点 ──")
bench("GET /System/Info/Public", "GET", "/System/Info/Public")
bench("GET /System/Info", "GET", "/System/Info")

print("\n── 媒体库/Views ──")
bench("GET /Users/{id}/Views", "GET", f"/Users/{USER_ID}/Views")

print("\n── 分页查询（核心性能） ──")
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

print("\n── 搜索 ──")
bench("搜索 '龙虎' Limit=50",
      "GET", f"/Users/{USER_ID}/Items?SearchTerm=%E9%BE%99%E8%99%8E&Recursive=true&Limit=50")
bench("搜索 '星际' Limit=50",
      "GET", f"/Users/{USER_ID}/Items?SearchTerm=%E6%98%9F%E9%99%85&Recursive=true&Limit=50")
bench("搜索 '第3集' Limit=50",
      "GET", f"/Users/{USER_ID}/Items?SearchTerm=%E7%AC%AC3%E9%9B%86&Recursive=true&Limit=50")
bench("搜索 'movie_0099' (精确)",
      "GET", f"/Users/{USER_ID}/Items?SearchTerm=movie_0099&Recursive=true&Limit=50")

print("\n── 排序 ──")
bench("SortBy=CommunityRating DESC Limit=50",
      "GET", f"/Users/{USER_ID}/Items?IncludeItemTypes=Movie&Recursive=true&Limit=50&SortBy=CommunityRating&SortOrder=Descending")
bench("SortBy=PremiereDate DESC Limit=50",
      "GET", f"/Users/{USER_ID}/Items?IncludeItemTypes=Movie&Recursive=true&Limit=50&SortBy=PremiereDate&SortOrder=Descending")
bench("SortBy=DateCreated DESC Limit=50",
      "GET", f"/Users/{USER_ID}/Items?IncludeItemTypes=Movie&Recursive=true&Limit=50&SortBy=DateCreated&SortOrder=Descending")

print("\n── 流派/人物 ──")
bench("GET /Genres", "GET", "/Genres")
bench("GET /Persons", "GET", "/Persons")

print("\n── 管理端点 ──")
bench("GET /ScheduledTasks", "GET", "/ScheduledTasks")
bench("GET /Sessions", "GET", "/Sessions")

# 总结
print(f"\n{'═' * 70}")
print(f"  百万片源性能测试报告（优化后）")
print(f"{'═' * 70}")

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
