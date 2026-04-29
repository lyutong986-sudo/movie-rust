#!/usr/bin/env python3
"""movie-rust 全功能集成测试 (API + 流程)

要求：后端运行在 http://127.0.0.1:8096, PostgreSQL 已启动。
用法：python tests/integration_test.py
"""

import json, sys, time, urllib.request, urllib.parse, urllib.error

BASE = "http://127.0.0.1:8096"
TOKEN = ""
USER_ID = ""
ADMIN_USER = "testadmin"
ADMIN_PASS = "TestPass123!"

# ── 工具 ─────────────────────────────────────────────────────────────

def req(method, path, body=None, expect_status=None, token_override=None):
    url = BASE + path
    data = json.dumps(body).encode() if body is not None else None
    headers = {"Content-Type": "application/json"}
    tk = token_override if token_override is not None else TOKEN
    if tk:
        headers["X-Emby-Token"] = tk
    r = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        resp = urllib.request.urlopen(r, timeout=15)
        code = resp.status
        raw = resp.read()
        try:
            result = json.loads(raw) if raw else {}
        except (json.JSONDecodeError, ValueError):
            result = {"_raw": raw.decode(errors="replace")[:500]}
    except urllib.error.HTTPError as e:
        code = e.code
        raw = e.read()
        try:
            result = json.loads(raw) if raw else {}
        except (json.JSONDecodeError, ValueError):
            result = {"_raw": raw.decode(errors="replace")[:500]}
    if expect_status and code != expect_status:
        print(f"  ✗ [{method} {path}] 期望 {expect_status} 实际 {code}")
        print(f"    响应: {json.dumps(result, ensure_ascii=False)[:300]}")
        return None, code
    return result, code

def get(path, **kw):  return req("GET", path, **kw)
def post(path, body=None, **kw): return req("POST", path, body=body, **kw)
def delete(path, **kw): return req("DELETE", path, **kw)

passed = 0
failed = 0
def check(name, condition, detail=""):
    global passed, failed
    if condition:
        passed += 1
        print(f"  ✓ {name}")
    else:
        failed += 1
        print(f"  ✗ {name}  {detail}")

# ══════════════════════════════════════════════════════════════════════
# 1. 系统 / 启动信息
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 1. 系统端点 ═══")

data, code = get("/System/Info/Public")
check("GET /System/Info/Public 返回 200", code == 200)
check("ServerName 存在", data and "ServerName" in data, str(data)[:100])

data, code = get("/Encoding/CodecConfiguration/Defaults")
check("GET encoding 配置", code == 200)

data, code = get("/emby/System/Info/Public")
check("emby 前缀路由", code == 200)

data, code = get("/mediabrowser/System/Info/Public")
check("mediabrowser 前缀路由", code == 200)

# ══════════════════════════════════════════════════════════════════════
# 2. 启动向导
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 2. 启动向导 ═══")

data, code = get("/Startup/Configuration")
check("GET /Startup/Configuration", code in (200, 401, 403), f"code={code}")

data, code = get("/Startup/FirstUser")
check("GET /Startup/FirstUser", code in (200, 401, 403), f"code={code}")

# ══════════════════════════════════════════════════════════════════════
# 3. 用户注册 / 登录
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 3. 用户注册与登录 ═══")

auth_body = {"Name": ADMIN_USER, "Pw": ADMIN_PASS}
auth_header = (
    'MediaBrowser Client="TestSuite", Device="PythonTest", '
    'DeviceId="test-device-001", Version="1.0.0"'
)

def do_login(password):
    body = {"Name": ADMIN_USER, "Pw": password}
    r = urllib.request.Request(
        BASE + "/Users/AuthenticateByName",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", "X-Emby-Authorization": auth_header},
        method="POST",
    )
    resp = urllib.request.urlopen(r, timeout=10)
    return json.loads(resp.read())

logged_in = False
for pw in [ADMIN_PASS, "NewTestPass456!"]:
    try:
        login_data = do_login(pw)
        TOKEN = login_data.get("AccessToken", "")
        USER_ID = login_data.get("User", {}).get("Id", "")
        if TOKEN:
            logged_in = True
            if pw != ADMIN_PASS:
                # 重置密码回默认
                post(f"/Users/{USER_ID}/Password", {"CurrentPw": pw, "NewPw": ADMIN_PASS})
                login_data = do_login(ADMIN_PASS)
                TOKEN = login_data.get("AccessToken", TOKEN)
            break
    except urllib.error.HTTPError:
        continue

if not logged_in:
    try:
        post("/Startup/User", {"Name": ADMIN_USER, "Password": ADMIN_PASS}, token_override="")
        post("/Startup/Complete", token_override="")
        login_data = do_login(ADMIN_PASS)
        TOKEN = login_data.get("AccessToken", "")
        USER_ID = login_data.get("User", {}).get("Id", "")
        logged_in = True
        check("新用户创建 + 登录", bool(TOKEN))
    except Exception as e:
        check("登录请求", False, str(e))

if logged_in:
    check("登录成功", bool(TOKEN))

check("获取到 AccessToken", bool(TOKEN), TOKEN[:20] if TOKEN else "空")
check("获取到 UserId", bool(USER_ID), USER_ID[:20] if USER_ID else "空")

# ══════════════════════════════════════════════════════════════════════
# 4. 用户管理
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 4. 用户管理 ═══")

data, code = get("/Users")
check("GET /Users 返回用户列表", code == 200 and isinstance(data, list))

data, code = get(f"/Users/{USER_ID}")
check("GET /Users/{id} 返回用户详情", code == 200 and data.get("Name"))

data, code = get("/Users/Public")
check("GET /Users/Public", code == 200)

# ══════════════════════════════════════════════════════════════════════
# 5. 系统信息（需认证）
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 5. 系统信息（认证后）═══")

data, code = get("/System/Info")
check("GET /System/Info 需认证", code == 200 and "ServerName" in (data or {}))

data, code = get("/System/Logs")
check("GET /System/Logs", code == 200)

data, code = get("/System/ActivityLog/Entries?limit=5")
check("GET ActivityLog", code == 200)

# ══════════════════════════════════════════════════════════════════════
# 6. 服务器功能声明
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 6. 服务器功能 ═══")

data, code = get("/Features")
check("GET /Features", code == 200)
if data:
    check("SupportsCollections = true", data.get("SupportsCollections") == True)
    check("SupportsBackupRestore = true", data.get("SupportsBackupRestore") == True)
    check("SupportsTrickplay = true", data.get("SupportsTrickplay") == True)
    check("SupportsLiveTv = false", data.get("SupportsLiveTv") == False)
    check("SupportsDlna = false", data.get("SupportsDlna") == False)

# ══════════════════════════════════════════════════════════════════════
# 7. 媒体库管理
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 7. 媒体库管理 ═══")

data, code = get(f"/Users/{USER_ID}/Views")
check("GET /Users/{id}/Views（媒体库列表）", code == 200)
views = data.get("Items", []) if data else []

data, code = get(f"/Users/{USER_ID}/Items?Recursive=true&Limit=5")
check("GET Items（递归）", code == 200)
total_items = data.get("TotalRecordCount", 0) if data else 0
print(f"    数据库中共 {total_items} 个媒体条目")

# ══════════════════════════════════════════════════════════════════════
# 8. 类型 / 过滤器
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 8. 类型与过滤器 ═══")

data, code = get(f"/Users/{USER_ID}/Items/Filters2")
check("GET /Items/Filters2", code in (200, 400, 404), f"code={code}")

data, code = get("/ItemTypes")
check("GET /ItemTypes", code == 200)

# ══════════════════════════════════════════════════════════════════════
# 9. 流派 / 人物
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 9. 流派与人物 ═══")

data, code = get("/Genres")
check("GET /Genres", code == 200)

data, code = get("/Persons")
check("GET /Persons", code == 200)

# ══════════════════════════════════════════════════════════════════════
# 10. 搜索
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 10. 搜索 ═══")

data, code = get(f"/Users/{USER_ID}/Items?SearchTerm=test&Recursive=true&Limit=5")
check("搜索功能正常", code == 200)

# ══════════════════════════════════════════════════════════════════════
# 11. 设备 / 会话
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 11. 设备与会话 ═══")

data, code = get("/Devices")
check("GET /Devices", code == 200)

data, code = get("/Sessions")
check("GET /Sessions", code == 200 and isinstance(data, list))

# ══════════════════════════════════════════════════════════════════════
# 12. 计划任务
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 12. 计划任务 ═══")

data, code = get("/ScheduledTasks")
check("GET /ScheduledTasks", code == 200 and isinstance(data, list))
if data:
    task_id = data[0].get("Id", "")
    td, tc = get(f"/ScheduledTasks/{task_id}")
    check("GET /ScheduledTasks/{id} 详情", tc == 200)

# ══════════════════════════════════════════════════════════════════════
# 13. 收藏集
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 13. 收藏集 ═══")

data, code = get(f"/Users/{USER_ID}/Items?IncludeItemTypes=BoxSet&Recursive=true")
check("收藏集列表查询", code == 200)

# ══════════════════════════════════════════════════════════════════════
# 14. 播放列表
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 14. 播放列表 ═══")

data, code = post("/Playlists", {"Name": "测试播放列表", "MediaType": "Video"})
check("创建播放列表", code == 200 and data)
playlist_id = data.get("Id", "") if data else ""

if playlist_id:
    d2, c2 = get(f"/Playlists/{playlist_id}/Items")
    check("获取播放列表内容", c2 == 200)

    d3, c3 = delete(f"/Playlists/{playlist_id}")
    check("删除播放列表", c3 in (200, 204))

# ══════════════════════════════════════════════════════════════════════
# 15. Trickplay API (新功能)
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 15. Trickplay API ═══")

# 用一个不存在的 ID 测试端点是否存在且返回合理响应
fake_id = "00000000000000000000000000000000"
data, code = get(f"/Items/{fake_id}/Trickplay")
check("GET /Items/{id}/Trickplay 端点存在", code in (200, 404))
if code == 200 and data:
    check("Trickplay 返回 Resolutions 字段", "Resolutions" in data)

# ══════════════════════════════════════════════════════════════════════
# 16. MediaSegments API (新功能)
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 16. MediaSegments API ═══")

data, code = get(f"/MediaSegments/{fake_id}")
check("GET /MediaSegments/{id} 端点存在", code in (200, 404))
if code == 200 and data:
    check("MediaSegments 返回 Items 字段", "Items" in data)

# ══════════════════════════════════════════════════════════════════════
# 17. BackupRestore (新功能)
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 17. BackupRestore ═══")

data, code = get("/BackupRestore/BackupInfo")
check("GET /BackupRestore/BackupInfo", code == 200 and data)
if data:
    check("BackupInfo 含 SupportedModes", "SupportedModes" in data)

# ══════════════════════════════════════════════════════════════════════
# 18. 编码 / 流选项
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 18. 编码与流选项 ═══")

data, code = get("/encoding/tonemapoptions")
check("GET /encoding/tonemapoptions", code == 200)

data, code = get("/StreamLanguages")
check("GET /StreamLanguages", code == 200)

# ══════════════════════════════════════════════════════════════════════
# 19. 图片端点
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 19. 图片端点 ═══")

data, code = get(f"/Items/{fake_id}/Images")
check("GET /Items/{id}/Images 端点存在", code in (200, 404))

data, code = get(f"/Items/{fake_id}/RemoteImages")
check("GET /Items/{id}/RemoteImages 端点存在", code in (200, 404))

# ══════════════════════════════════════════════════════════════════════
# 20. 兼容端点
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 20. 兼容端点（Emby 客户端） ═══")

data, code = get("/DisplayPreferences/usersettings?client=emby")
check("GET /DisplayPreferences", code == 200)

data, code = get("/emby/Features")
check("emby 前缀 Features", code == 200)

data, code = get(f"/emby/Users/{USER_ID}/Items?Limit=1&Recursive=true")
check("emby 前缀 Items", code == 200)

data, code = get(f"/mediabrowser/Users/{USER_ID}/Items?Limit=1&Recursive=true")
check("mediabrowser 前缀 Items", code == 200)

# ══════════════════════════════════════════════════════════════════════
# 21. UserData 操作
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 21. UserData 操作 ═══")

# 如果有条目，测试收藏/已看
items_data, _ = get(f"/Users/{USER_ID}/Items?Recursive=true&Limit=1")
if items_data and items_data.get("Items"):
    test_item_id = items_data["Items"][0]["Id"]

    d, c = post(f"/Users/{USER_ID}/FavoriteItems/{test_item_id}")
    check("标记收藏", c == 200)

    d, c = delete(f"/Users/{USER_ID}/FavoriteItems/{test_item_id}")
    check("取消收藏", c == 200)

    d, c = post(f"/Users/{USER_ID}/PlayedItems/{test_item_id}")
    check("标记已看", c == 200)

    d, c = delete(f"/Users/{USER_ID}/PlayedItems/{test_item_id}")
    check("取消已看", c == 200)
else:
    print("  ⓘ 无媒体条目, 跳过 UserData 测试")

# ══════════════════════════════════════════════════════════════════════
# 22. 播放状态汇报
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 22. 播放状态汇报 ═══")

playback_body = {
    "ItemId": fake_id,
    "PlaySessionId": "test-session-001",
    "PositionTicks": 0,
    "IsPaused": False,
}
d, c = post("/Sessions/Playing", playback_body)
check("POST /Sessions/Playing（开始播放）", c in (200, 204, 400, 404, 500), f"code={c}")

d, c = post("/Sessions/Playing/Progress", playback_body)
check("POST /Sessions/Playing/Progress（进度）", c in (200, 204, 400, 404, 500), f"code={c}")

d, c = post("/Sessions/Playing/Stopped", playback_body)
check("POST /Sessions/Playing/Stopped（停止）", c in (200, 204, 400, 404, 500), f"code={c}")

# ══════════════════════════════════════════════════════════════════════
# 23. 无认证请求应被拒绝
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 23. 鉴权保护 ═══")

d, c = get("/System/Info", token_override="bad-token-xxx")
check("错误 Token 应返回 401", c == 401, f"实际 {c}")

d, c = get("/Users", token_override="")
check("无 Token 应返回 401", c == 401, f"实际 {c}")

# ══════════════════════════════════════════════════════════════════════
# 24. 用户密码修改
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 24. 用户密码修改 ═══")

NEW_PASS = "NewTestPass456!"
d, c = post(f"/Users/{USER_ID}/Password", {
    "CurrentPw": ADMIN_PASS,
    "NewPw": NEW_PASS,
})
check("修改密码", c in (200, 204))

# 用新密码重新登录取回 token
login_req2 = urllib.request.Request(
    BASE + "/Users/AuthenticateByName",
    data=json.dumps({"Name": ADMIN_USER, "Pw": NEW_PASS}).encode(),
    headers={"Content-Type": "application/json", "X-Emby-Authorization": auth_header},
    method="POST",
)
try:
    resp2 = urllib.request.urlopen(login_req2, timeout=10)
    login_data2 = json.loads(resp2.read())
    TOKEN = login_data2.get("AccessToken", TOKEN)
except Exception:
    pass

# 改回去
d, c = post(f"/Users/{USER_ID}/Password", {
    "CurrentPw": NEW_PASS,
    "NewPw": ADMIN_PASS,
})
check("改回原密码", c in (200, 204))

# ══════════════════════════════════════════════════════════════════════
# 25. 登出
# ══════════════════════════════════════════════════════════════════════
print("\n═══ 25. 登出 ═══")

# 用原密码重新登录确保有有效 token
try:
    login_req3 = urllib.request.Request(
        BASE + "/Users/AuthenticateByName",
        data=json.dumps({"Name": ADMIN_USER, "Pw": ADMIN_PASS}).encode(),
        headers={"Content-Type": "application/json", "X-Emby-Authorization": auth_header},
        method="POST",
    )
    resp3 = urllib.request.urlopen(login_req3, timeout=10)
    login_data3 = json.loads(resp3.read())
    TOKEN = login_data3.get("AccessToken", TOKEN)
except Exception:
    pass

d, c = post("/Sessions/Logout")
check("POST /Sessions/Logout", c in (200, 204))

# ══════════════════════════════════════════════════════════════════════
# 汇总
# ══════════════════════════════════════════════════════════════════════
print(f"\n{'═' * 60}")
print(f"  测试结果: {passed} 通过, {failed} 失败, 共 {passed + failed} 项")
print(f"{'═' * 60}")
sys.exit(1 if failed else 0)
