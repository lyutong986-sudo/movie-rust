"""
测试远端 Emby 媒体资源是否存在 302 重定向链。
登录 → 获取媒体库 → 获取第一个电影 → 请求播放信息 → 测试流媒体 URL 的重定向。
"""
import requests
import uuid
import json
import sys

SERVER_URL = "http://aaa.204cloud.com"
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
        f'Client="{CLIENT}"',
        f'Device="{DEVICE}"',
        f'DeviceId="{DEVICE_ID}"',
        f'Version="{VERSION}"',
    ])
    return f'Emby {", ".join(parts)}'

def api_headers(token, user_id=""):
    auth = auth_header(user_id)
    return {
        "User-Agent": USER_AGENT,
        "Accept": "application/json",
        "X-Emby-Token": token,
        "Authorization": auth,
        "X-Emby-Authorization": auth,
    }

# Step 1: 登录
print("=" * 70)
print("Step 1: 登录")
print("=" * 70)
login_auth = auth_header()
resp = requests.post(
    f"{SERVER_URL}/Users/AuthenticateByName",
    headers={
        "User-Agent": USER_AGENT,
        "Content-Type": "application/json",
        "Accept": "application/json",
        "Authorization": login_auth,
        "X-Emby-Authorization": login_auth,
    },
    json={"Username": USERNAME, "Pw": PASSWORD, "Password": PASSWORD},
    timeout=15,
)
if resp.status_code != 200:
    print(f"登录失败: {resp.status_code}")
    sys.exit(1)
data = resp.json()
TOKEN = data["AccessToken"]
USER_ID = data["User"]["Id"]
print(f"Token: {TOKEN[:16]}...  UserId: {USER_ID}")

# Step 2: 获取媒体库，找到电影库
print("\n" + "=" * 70)
print("Step 2: 获取媒体库")
print("=" * 70)
resp2 = requests.get(
    f"{SERVER_URL}/Users/{USER_ID}/Views",
    headers=api_headers(TOKEN, USER_ID),
    params={"Fields": "CollectionType", "api_key": TOKEN},
    timeout=15,
)
views = resp2.json().get("Items", [])
movie_view = None
for v in views:
    ct = v.get("CollectionType", "")
    print(f"  {v['Name']} (Id={v['Id']}, Type={ct})")
    if ct == "movies" and movie_view is None:
        movie_view = v

if not movie_view:
    print("没找到电影库!")
    sys.exit(1)
print(f"\n选中电影库: {movie_view['Name']} (Id={movie_view['Id']})")

# Step 3: 获取第一部电影
print("\n" + "=" * 70)
print("Step 3: 获取第一部电影")
print("=" * 70)
resp3 = requests.get(
    f"{SERVER_URL}/Users/{USER_ID}/Items",
    headers=api_headers(TOKEN, USER_ID),
    params={
        "ParentId": movie_view["Id"],
        "IncludeItemTypes": "Movie",
        "Recursive": "true",
        "Limit": "1",
        "Fields": "Path,MediaSources",
        "api_key": TOKEN,
    },
    timeout=15,
)
items = resp3.json().get("Items", [])
if not items:
    print("电影库为空!")
    sys.exit(1)
movie = items[0]
movie_id = movie["Id"]
movie_name = movie.get("Name", "?")
print(f"电影: {movie_name} (Id={movie_id})")
print(f"Path: {movie.get('Path', 'N/A')}")

# Step 4: 获取 PlaybackInfo
print("\n" + "=" * 70)
print("Step 4: 获取 PlaybackInfo")
print("=" * 70)
resp4 = requests.post(
    f"{SERVER_URL}/Items/{movie_id}/PlaybackInfo",
    headers={
        **api_headers(TOKEN, USER_ID),
        "Content-Type": "application/json",
    },
    params={
        "UserId": USER_ID,
        "StartTimeTicks": "0",
        "IsPlayback": "false",
        "AutoOpenLiveStream": "false",
        "api_key": TOKEN,
    },
    json={},
    timeout=15,
)
pb = resp4.json()
media_sources = pb.get("MediaSources", [])
print(f"MediaSources 数量: {len(media_sources)}")

for i, ms in enumerate(media_sources):
    print(f"\n  [{i}] Id={ms.get('Id', '?')}")
    print(f"      Name: {ms.get('Name', '?')}")
    print(f"      Path: {ms.get('Path', 'N/A')}")
    print(f"      Protocol: {ms.get('Protocol', '?')}")
    print(f"      Container: {ms.get('Container', '?')}")
    direct_url = ms.get("DirectStreamUrl") or ms.get("Path", "")
    print(f"      DirectStreamUrl: {direct_url[:120] if direct_url else 'N/A'}")

# Step 5: 测试流媒体 URL 的 302 重定向
print("\n" + "=" * 70)
print("Step 5: 测试流媒体 URL 重定向链")
print("=" * 70)

if media_sources:
    ms = media_sources[0]
    ms_id = ms.get("Id", movie_id)

    # 构造 Emby 标准的直接串流 URL
    stream_urls = []
    container = ms.get("Container", "mp4")
    
    # 尝试多种 URL 格式
    stream_urls.append(f"{SERVER_URL}/Videos/{movie_id}/stream.{container}?Static=true&MediaSourceId={ms_id}&api_key={TOKEN}")
    stream_urls.append(f"{SERVER_URL}/Videos/{movie_id}/stream?Static=true&MediaSourceId={ms_id}&api_key={TOKEN}")
    
    direct = ms.get("DirectStreamUrl")
    if direct:
        if direct.startswith("/"):
            stream_urls.insert(0, f"{SERVER_URL}{direct}")
        elif direct.startswith("http"):
            stream_urls.insert(0, direct)

    for url in stream_urls:
        print(f"\n测试 URL: {url[:120]}...")
        try:
            # allow_redirects=False: 不自动跟随重定向，手动查看每一跳
            r = requests.get(
                url,
                headers={
                    "User-Agent": USER_AGENT,
                    "X-Emby-Token": TOKEN,
                },
                allow_redirects=False,
                timeout=15,
                stream=True,
            )
            print(f"  Status: {r.status_code}")
            if r.status_code in (301, 302, 303, 307, 308):
                location = r.headers.get("Location", "")
                print(f"  >>> 重定向到: {location}")
                
                # 跟踪完整重定向链
                hop = 1
                current_url = location
                while hop < 10:
                    r2 = requests.get(
                        current_url,
                        headers={"User-Agent": USER_AGENT},
                        allow_redirects=False,
                        timeout=15,
                        stream=True,
                    )
                    print(f"  [Hop {hop}] {r2.status_code} -> {current_url[:100]}")
                    if r2.status_code in (301, 302, 303, 307, 308):
                        current_url = r2.headers.get("Location", "")
                        print(f"           >>> 重定向到: {current_url[:120]}")
                        hop += 1
                    else:
                        ct = r2.headers.get("Content-Type", "")
                        cl = r2.headers.get("Content-Length", "?")
                        print(f"           最终响应: Content-Type={ct}, Content-Length={cl}")
                        break
                r.close()
            else:
                ct = r.headers.get("Content-Type", "")
                cl = r.headers.get("Content-Length", "?")
                print(f"  Content-Type: {ct}")
                print(f"  Content-Length: {cl}")
                print(f"  (无重定向，直接返回内容)")
                r.close()
            break  # 测试第一个有效 URL 即可
        except Exception as e:
            print(f"  失败: {e}")

print("\n" + "=" * 70)
print("完成!")
print("=" * 70)
