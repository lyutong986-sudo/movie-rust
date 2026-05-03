"""
精确对比 Python requests vs 项目 Rust reqwest 的请求差异。
逐层排查：TLS 指纹、HTTP 版本、Header 顺序、Accept-Encoding 等。
"""
import requests
import urllib3
import uuid
import json
import sys
import socket
import ssl
import http.client

SERVER_URL = "http://aaa.204cloud.com"
USERNAME = "bbll"
PASSWORD = "bbll"

CLIENT = "Infuse-Direct"
DEVICE = "Apple TV"
DEVICE_ID = uuid.uuid4().hex
VERSION = "8.2.4"
USER_AGENT = f"{CLIENT}/{VERSION}"
AUTH_VALUE = f'Emby Client="{CLIENT}", Device="{DEVICE}", DeviceId="{DEVICE_ID}", Version="{VERSION}"'

endpoint = f"{SERVER_URL}/Users/AuthenticateByName"

# ==========================================
# Test 1: 标准 Python requests（能成功的方式）
# ==========================================
print("=" * 70)
print("Test 1: Python requests（标准方式）")
print("=" * 70)
headers1 = {
    "User-Agent": USER_AGENT,
    "Content-Type": "application/json",
    "Accept": "application/json",
    "Authorization": AUTH_VALUE,
    "X-Emby-Authorization": AUTH_VALUE,
}
body = {"Username": USERNAME, "Pw": PASSWORD, "Password": PASSWORD}
try:
    resp = requests.post(endpoint, headers=headers1, json=body, timeout=15)
    print(f"  Status: {resp.status_code}")
    print(f"  实际发送的 Headers:")
    print(f"    User-Agent: {resp.request.headers.get('User-Agent')}")
    print(f"    Content-Type: {resp.request.headers.get('Content-Type')}")
    print(f"    Accept: {resp.request.headers.get('Accept')}")
    print(f"    Accept-Encoding: {resp.request.headers.get('Accept-Encoding')}")
    print(f"    Connection: {resp.request.headers.get('Connection')}")
    print(f"    Content-Length: {resp.request.headers.get('Content-Length')}")
    print(f"    Authorization: {resp.request.headers.get('Authorization', '')[:60]}...")
    print(f"    X-Emby-Authorization: {'有' if resp.request.headers.get('X-Emby-Authorization') else '无'}")
    print(f"    Transfer-Encoding: {resp.request.headers.get('Transfer-Encoding')}")
    print(f"  Header 顺序: {list(resp.request.headers.keys())}")
    print(f"  HTTP version: HTTP/1.1 (requests 固定)")
    print(f"  结果: {'成功' if resp.status_code == 200 else '失败'}")
except Exception as e:
    print(f"  失败: {e}")

# ==========================================
# Test 2: 模拟 reqwest 默认行为（无 Accept-Encoding、无 Connection）
# ==========================================
print("\n" + "=" * 70)
print("Test 2: 模拟 Rust reqwest 行为（去掉 Accept-Encoding / Connection）")
print("=" * 70)
sess = requests.Session()
sess.headers.clear()  # 清掉 requests 默认的 Accept-Encoding, Connection 等
headers2 = {
    "user-agent": USER_AGENT,
    "content-type": "application/json",
    "accept": "application/json",
    "authorization": AUTH_VALUE,
    "x-emby-authorization": AUTH_VALUE,
}
try:
    resp2 = sess.post(endpoint, headers=headers2, json=body, timeout=15)
    print(f"  Status: {resp2.status_code}")
    print(f"  实际发送的 Headers:")
    print(f"    {dict(resp2.request.headers)}")
    print(f"  结果: {'成功' if resp2.status_code == 200 else '失败'}")
except Exception as e:
    print(f"  失败: {e}")

# ==========================================
# Test 3: 使用低层 http.client 精确控制（最接近 reqwest）
# ==========================================
print("\n" + "=" * 70)
print("Test 3: 低层 http.client（精确控制 headers，无多余 headers）")
print("=" * 70)
from urllib.parse import urlparse
parsed = urlparse(endpoint)
host = parsed.hostname
port = parsed.port or (443 if parsed.scheme == "https" else 80)
path = parsed.path

try:
    if parsed.scheme == "https":
        conn = http.client.HTTPSConnection(host, port, timeout=15)
    else:
        conn = http.client.HTTPConnection(host, port, timeout=15)

    json_body = json.dumps(body)
    conn.request("POST", path, body=json_body, headers={
        "User-Agent": USER_AGENT,
        "Content-Type": "application/json",
        "Accept": "application/json",
        "Authorization": AUTH_VALUE,
        "X-Emby-Authorization": AUTH_VALUE,
        "Content-Length": str(len(json_body)),
    })
    resp3 = conn.getresponse()
    print(f"  Status: {resp3.status}")
    print(f"  HTTP version: HTTP/{resp3.version / 10:.1f}")
    resp3_body = resp3.read()
    if resp3.status == 200:
        data = json.loads(resp3_body)
        print(f"  Token: {data.get('AccessToken', '?')[:16]}...")
    print(f"  结果: {'成功' if resp3.status == 200 else '失败'}")
    conn.close()
except Exception as e:
    print(f"  失败: {e}")

# ==========================================
# Test 4: 查看出口 IP
# ==========================================
print("\n" + "=" * 70)
print("Test 4: 出口 IP 检查")
print("=" * 70)
try:
    ip_resp = requests.get("https://httpbin.org/ip", timeout=10)
    print(f"  本机出口 IP: {ip_resp.json().get('origin', '?')}")
except:
    try:
        ip_resp = requests.get("https://api.ipify.org?format=json", timeout=10)
        print(f"  本机出口 IP: {ip_resp.json().get('ip', '?')}")
    except Exception as e:
        print(f"  无法获取: {e}")

# ==========================================
# Test 5: DNS 解析
# ==========================================
print("\n" + "=" * 70)
print("Test 5: DNS 解析")
print("=" * 70)
for domain in ["aaa.204cloud.com", "emby.340773.xyz"]:
    try:
        ips = socket.getaddrinfo(domain, 80, socket.AF_INET)
        resolved = set(addr[4][0] for addr in ips)
        print(f"  {domain} -> {', '.join(resolved)}")
    except Exception as e:
        print(f"  {domain} -> 解析失败: {e}")

print("\n" + "=" * 70)
print("完成!")
print("=" * 70)
