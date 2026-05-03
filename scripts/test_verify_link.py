"""重新获取直链并立即验证"""
import requests, uuid

SERVER_URL = "https://emby.340773.xyz"
CLIENT, DEVICE, VERSION = "Infuse-Direct", "Apple TV", "8.2.4"
DEVICE_ID = uuid.uuid4().hex
UA = f"{CLIENT}/{VERSION}"
auth = f'Emby Client="{CLIENT}", Device="{DEVICE}", DeviceId="{DEVICE_ID}", Version="{VERSION}"'

# 登录
r = requests.post(f"{SERVER_URL}/Users/AuthenticateByName",
    headers={"User-Agent": UA, "Content-Type": "application/json", "Accept": "application/json",
             "Authorization": auth, "X-Emby-Authorization": auth},
    json={"Username": "bbll", "Pw": "bbll", "Password": "bbll"}, timeout=15)
d = r.json()
TOKEN, USER_ID = d["AccessToken"], d["User"]["Id"]
print(f"登录成功 Token={TOKEN[:16]}...")

# 获取第一部电影
r2 = requests.get(f"{SERVER_URL}/Users/{USER_ID}/Items",
    headers={"User-Agent": UA, "Accept": "application/json", "X-Emby-Token": TOKEN},
    params={"ParentId": "7", "IncludeItemTypes": "Movie", "Recursive": "true",
            "Limit": "1", "Fields": "MediaSources", "api_key": TOKEN}, timeout=15)
movie = r2.json()["Items"][0]
movie_id = movie["Id"]
print(f"电影: {movie.get('Name','?')} (Id={movie_id})")

# PlaybackInfo
r3 = requests.post(f"{SERVER_URL}/Items/{movie_id}/PlaybackInfo",
    headers={"User-Agent": UA, "Accept": "application/json", "X-Emby-Token": TOKEN,
             "Content-Type": "application/json"},
    params={"UserId": USER_ID, "StartTimeTicks": "0", "IsPlayback": "false",
            "AutoOpenLiveStream": "false", "api_key": TOKEN},
    json={}, timeout=15)
ms = r3.json()["MediaSources"][0]
ms_id, container = ms["Id"], ms.get("Container", "mp4")
print(f"MediaSource: {ms.get('Name','?')} ({container})")

# 获取 302 直链
stream_url = f"{SERVER_URL}/Videos/{movie_id}/stream.{container}?Static=true&MediaSourceId={ms_id}&api_key={TOKEN}"
print(f"\n请求流: {stream_url[:100]}...")
r4 = requests.get(stream_url, headers={"User-Agent": UA, "X-Emby-Token": TOKEN},
    allow_redirects=False, timeout=15, stream=True)
print(f"远端响应: {r4.status_code}")
direct_url = r4.headers.get("Location", "")
r4.close()
print(f"直链: {direct_url}")

# 立即验证
print("\n--- 立即 Range 验证 ---")
r5 = requests.get(direct_url,
    headers={"User-Agent": UA, "Range": "bytes=0-1023"},
    timeout=15, stream=True)
print(f"Status: {r5.status_code}")
print(f"Content-Range: {r5.headers.get('Content-Range', 'N/A')}")
print(f"Content-Type: {r5.headers.get('Content-Type', '?')}")
print(f"Content-Length: {r5.headers.get('Content-Length', '?')}")
chunk = r5.content
print(f"实际读取: {len(chunk)} bytes")
r5.close()

if r5.status_code in (200, 206):
    print("\n>>> 直链有效! 可以播放!")
else:
    print(f"\n>>> 直链无效! 响应: {chunk[:200]}")
