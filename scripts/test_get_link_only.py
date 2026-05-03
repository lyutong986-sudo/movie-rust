"""只获取直链，不访问"""
import requests, uuid

SERVER_URL = "https://emby.340773.xyz"
CLIENT, DEVICE, VERSION = "Infuse-Direct", "Apple TV", "8.2.4"
DEVICE_ID = uuid.uuid4().hex
UA = f"{CLIENT}/{VERSION}"
auth = f'Emby Client="{CLIENT}", Device="{DEVICE}", DeviceId="{DEVICE_ID}", Version="{VERSION}"'

r = requests.post(f"{SERVER_URL}/Users/AuthenticateByName",
    headers={"User-Agent": UA, "Content-Type": "application/json", "Accept": "application/json",
             "Authorization": auth, "X-Emby-Authorization": auth},
    json={"Username": "bbll", "Pw": "bbll", "Password": "bbll"}, timeout=15)
d = r.json()
TOKEN, USER_ID = d["AccessToken"], d["User"]["Id"]

r2 = requests.get(f"{SERVER_URL}/Users/{USER_ID}/Items",
    headers={"User-Agent": UA, "Accept": "application/json", "X-Emby-Token": TOKEN},
    params={"ParentId": "7", "IncludeItemTypes": "Movie", "Recursive": "true",
            "Limit": "1", "Fields": "MediaSources", "api_key": TOKEN}, timeout=15)
movie = r2.json()["Items"][0]
movie_id = movie["Id"]

r3 = requests.post(f"{SERVER_URL}/Items/{movie_id}/PlaybackInfo",
    headers={"User-Agent": UA, "Accept": "application/json", "X-Emby-Token": TOKEN,
             "Content-Type": "application/json"},
    params={"UserId": USER_ID, "StartTimeTicks": "0", "IsPlayback": "false",
            "AutoOpenLiveStream": "false", "api_key": TOKEN},
    json={}, timeout=15)
ms = r3.json()["MediaSources"][0]
ms_id, container = ms["Id"], ms.get("Container", "mp4")

stream_url = f"{SERVER_URL}/Videos/{movie_id}/stream.{container}?Static=true&MediaSourceId={ms_id}&api_key={TOKEN}"
r4 = requests.get(stream_url, headers={"User-Agent": UA, "X-Emby-Token": TOKEN},
    allow_redirects=False, timeout=15, stream=True)
direct_url = r4.headers.get("Location", "")
r4.close()

print(direct_url)
