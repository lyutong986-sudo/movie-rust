"""
在 Docker 容器内运行此脚本，对比网络环境差异。
用法: docker exec <container_name> python3 /tmp/test_docker_network.py
或者直接在宿主机对比运行。
"""
import socket
import json
import sys
from urllib.request import Request, urlopen
from urllib.error import URLError
import ssl

print("=" * 60)
print("网络环境诊断")
print("=" * 60)

# 1. DNS 解析
print("\n[DNS 解析]")
for domain in ["aaa.204cloud.com", "emby.340773.xyz"]:
    try:
        results = socket.getaddrinfo(domain, 80, socket.AF_INET)
        ips = set(addr[4][0] for addr in results)
        print(f"  {domain} -> {', '.join(ips)}")
    except Exception as e:
        print(f"  {domain} -> 失败: {e}")

# 2. 出口 IP
print("\n[出口 IP]")
for url in ["https://httpbin.org/ip", "https://api.ipify.org?format=json"]:
    try:
        ctx = ssl.create_default_context()
        req = Request(url, headers={"User-Agent": "curl/8.0"})
        resp = urlopen(req, timeout=10, context=ctx)
        data = json.loads(resp.read())
        ip = data.get("origin") or data.get("ip")
        print(f"  {ip}")
        break
    except Exception as e:
        print(f"  {url} 失败: {e}")

# 3. TCP 直连测试
print("\n[TCP 直连测试]")
for host, port in [("aaa.204cloud.com", 80), ("emby.340773.xyz", 443)]:
    try:
        sock = socket.create_connection((host, port), timeout=10)
        local = sock.getsockname()
        remote = sock.getpeername()
        print(f"  {host}:{port} -> 连接成功")
        print(f"    本地: {local[0]}:{local[1]}")
        print(f"    远端: {remote[0]}:{remote[1]}")
        sock.close()
    except Exception as e:
        print(f"  {host}:{port} -> 连接失败: {e}")

# 4. HTTP 登录测试
print("\n[HTTP 登录测试 - aaa.204cloud.com]")
import uuid
device_id = uuid.uuid4().hex
auth = f'Emby Client="Infuse-Direct", Device="Apple TV", DeviceId="{device_id}", Version="8.2.4"'
body = json.dumps({"Username": "bbll", "Pw": "bbll", "Password": "bbll"}).encode()

try:
    req = Request(
        "http://aaa.204cloud.com/Users/AuthenticateByName",
        data=body,
        headers={
            "User-Agent": "Infuse-Direct/8.2.4",
            "Content-Type": "application/json",
            "Accept": "application/json",
            "Authorization": auth,
            "X-Emby-Authorization": auth,
        },
        method="POST",
    )
    resp = urlopen(req, timeout=15)
    data = json.loads(resp.read())
    print(f"  状态: {resp.status}")
    print(f"  Token: {data.get('AccessToken', '?')[:16]}...")
    print(f"  结果: 成功")
except Exception as e:
    print(f"  结果: 失败 -> {e}")

# 5. HTTPS 登录测试
print("\n[HTTPS 登录测试 - emby.340773.xyz]")
device_id2 = uuid.uuid4().hex
auth2 = f'Emby Client="Infuse-Direct", Device="Apple TV", DeviceId="{device_id2}", Version="8.2.4"'
body2 = json.dumps({"Username": "bbll", "Pw": "bbll", "Password": "bbll"}).encode()
try:
    ctx = ssl.create_default_context()
    req2 = Request(
        "https://emby.340773.xyz/Users/AuthenticateByName",
        data=body2,
        headers={
            "User-Agent": "Infuse-Direct/8.2.4",
            "Content-Type": "application/json",
            "Accept": "application/json",
            "Authorization": auth2,
            "X-Emby-Authorization": auth2,
        },
        method="POST",
    )
    resp2 = urlopen(req2, timeout=15, context=ctx)
    data2 = json.loads(resp2.read())
    print(f"  状态: {resp2.status}")
    print(f"  Token: {data2.get('AccessToken', '?')[:16]}...")
    print(f"  结果: 成功")
except Exception as e:
    print(f"  结果: 失败 -> {e}")

print("\n" + "=" * 60)
print("诊断完成")
print("=" * 60)
