"""
测试 emby.340773.xyz 媒体存储直链的完整重定向链和有效性。
"""
import requests
import uuid
import json
import sys

SERVER_URL = "https://emby.340773.xyz"
USERNAME = "bbll"
PASSWORD = "bbll"

CLIENT = "Infuse-Direct"
DEVICE = "Apple TV"
DEVICE_ID = uuid.uuid4().hex
VERSION = "8.2.4"
USER_AGENT = f"{CLIENT}/{VERSION}"

def auth_header(user_id=""):
    parts = []
    if user_id:
        parts.append(f'UserId="{user_id}"')
    parts.extend([
        f'Client="{CLIENT}"', f'Device="{DEVICE}"',
        f'DeviceId="{DEVICE_ID}"', f'Version="{VERSION}"',
    ])
    return f'Emby {", ".join(parts)}'

def api_headers(token, user_id=""):
    a = auth_header(user_id)
    return {"User-Agent": USER_AGENT, "Accept": "application/json",
            "X-Emby-Token": token, "Authorization": a, "X-Emby-Authorization": a}

# Step 1: 登录
print("=" * 70)
print("Step 1: 登录")
login_auth = auth_header()
resp = requests.post(f"{SERVER_URL}/Users/AuthenticateByName",
    headers={"User-Agent": USER_AGENT, "Content-Type": "application/json",
             "Accept": "application/json", "Authorization": login_auth,
             "X-Emby-Authorization": login_auth},
    json={"Username": USERNAME, "Pw": PASSWORD, "Password": PASSWORD}, timeout=15)
assert resp.status_code == 200, f"登录失败: {resp.status_code}"
data = resp.json()
TOKEN = data["AccessToken"]
USER_ID = data["User"]["Id"]
print(f"  Token: {TOKEN[:16]}...  UserId: {USER_ID}")

# Step 2: 获取电影库
print("\n" + "=" * 70)
print("Step 2: 获取媒体库")
resp2 = requests.get(f"{SERVER_URL}/Users/{USER_ID}/Views",
    headers=api_headers(TOKEN, USER_ID),
    params={"Fields": "CollectionType", "api_key": TOKEN}, timeout=15)
views = resp2.json().get("Items", [])
movie_view = None
for v in views:
    ct = v.get("CollectionType", "")
    name = v.get("Name", "?")
    print(f"  {name} (Id={v['Id']}, Type={ct})")
    if ct == "movies" and movie_view is None:
        movie_view = v

assert movie_view, "没找到电影库!"
print(f"\n  选中: {movie_view['Name']} (Id={movie_view['Id']})")

# Step 3: 获取前3部电影
print("\n" + "=" * 70)
print("Step 3: 获取电影列表")
resp3 = requests.get(f"{SERVER_URL}/Users/{USER_ID}/Items",
    headers=api_headers(TOKEN, USER_ID),
    params={"ParentId": movie_view["Id"], "IncludeItemTypes": "Movie",
            "Recursive": "true", "Limit": "3", "Fields": "Path,MediaSources",
            "api_key": TOKEN}, timeout=15)
items = resp3.json().get("Items", [])
assert items, "电影库为空!"
for i, m in enumerate(items):
    print(f"  [{i}] {m.get('Name','?')} (Id={m['Id']})")

# 测试每部电影的流媒体直链
for movie in items:
    movie_id = movie["Id"]
    movie_name = movie.get("Name", "?")
    print("\n" + "=" * 70)
    print(f"测试: {movie_name} (Id={movie_id})")
    print("=" * 70)

    # Step 4: PlaybackInfo
    resp4 = requests.post(f"{SERVER_URL}/Items/{movie_id}/PlaybackInfo",
        headers={**api_headers(TOKEN, USER_ID), "Content-Type": "application/json"},
        params={"UserId": USER_ID, "StartTimeTicks": "0", "IsPlayback": "false",
                "AutoOpenLiveStream": "false", "api_key": TOKEN},
        json={}, timeout=15)
    pb = resp4.json()
    media_sources = pb.get("MediaSources", [])
    print(f"  MediaSources: {len(media_sources)}")

    if not media_sources:
        print("  (无 MediaSource)")
        continue

    ms = media_sources[0]
    ms_id = ms.get("Id", movie_id)
    container = ms.get("Container", "mp4")
    print(f"  [{0}] {ms.get('Name','?')} ({container}) Protocol={ms.get('Protocol','?')}")

    # Step 5: 请求流 URL，不跟随重定向
    stream_url = f"{SERVER_URL}/Videos/{movie_id}/stream.{container}?Static=true&MediaSourceId={ms_id}&api_key={TOKEN}"
    print(f"\n  Stream URL: {stream_url[:100]}...")

    try:
        r = requests.get(stream_url,
            headers={"User-Agent": USER_AGENT, "X-Emby-Token": TOKEN},
            allow_redirects=False, timeout=15, stream=True)
        print(f"  响应: {r.status_code}")

        if r.status_code in (301, 302, 303, 307, 308):
            loc = r.headers.get("Location", "")
            print(f"  302 -> {loc[:120]}...")
            r.close()

            # 跟踪重定向链
            current = loc
            hop = 1
            while hop <= 8:
                r2 = requests.get(current, headers={"User-Agent": USER_AGENT},
                    allow_redirects=False, timeout=15, stream=True)
                if r2.status_code in (301, 302, 303, 307, 308):
                    current = r2.headers.get("Location", "")
                    print(f"  [Hop {hop}] {r2.status_code} -> {current[:120]}...")
                    r2.close()
                    hop += 1
                else:
                    ct = r2.headers.get("Content-Type", "?")
                    cl = r2.headers.get("Content-Length", "?")
                    accept_ranges = r2.headers.get("Accept-Ranges", "?")
                    print(f"  [最终] {r2.status_code}")
                    print(f"    Content-Type: {ct}")
                    print(f"    Content-Length: {cl}")
                    print(f"    Accept-Ranges: {accept_ranges}")
                    print(f"    URL: {current[:150]}")

                    # 验证直链：用 Range 请求前 1 字节
                    try:
                        r3 = requests.get(current,
                            headers={"User-Agent": USER_AGENT, "Range": "bytes=0-0"},
                            timeout=15, stream=True)
                        print(f"    Range 验证: {r3.status_code} (Content-Range: {r3.headers.get('Content-Range','N/A')})")
                        r3.close()
                        if r3.status_code in (200, 206):
                            print(f"    >>> 直链有效!")
                        else:
                            print(f"    >>> 直链可能无效")
                    except Exception as e:
                        print(f"    Range 验证失败: {e}")
                    r2.close()
                    break
        elif r.status_code == 200:
            ct = r.headers.get("Content-Type", "?")
            cl = r.headers.get("Content-Length", "?")
            print(f"  (无重定向，直接返回)")
            print(f"    Content-Type: {ct}")
            print(f"    Content-Length: {cl}")
            r.close()
        else:
            print(f"  异常响应: {r.status_code} {r.text[:200]}")
            r.close()
    except Exception as e:
        print(f"  请求失败: {e}")

print("\n" + "=" * 70)
print("全部完成!")
print("=" * 70)
