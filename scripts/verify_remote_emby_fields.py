#!/usr/bin/env python3
"""PB40 远端 Emby 字段完整性核对脚本。

用途：
  通过本地后端的诊断接口
  `GET /api/admin/remote-emby/sources/{source_id}/diagnostic/sample-items`
  抓取若干样本条目，然后核对：
    - ImageTags 是否包含 Primary / Logo / Thumb / Banner / Art / Disc 6 类海报
    - BackdropImageTags 包含几张多背景
    - 是否有 People（演职员）数组（剧集 cast）
    - 是否有 Overview / Genres / Studios / Tags / Taglines / ProductionLocations / Status / EndDate / AirDays / AirTime
    - MediaSources / MediaStreams 是否齐全

如果上述字段在远端 BaseItemDto 已经全部齐全，**就不需要再走 TMDB 兜底**，
PB40 已经把 sync 链路改为一次性写入这些字段。

使用方法：
  python scripts/verify_remote_emby_fields.py \
      --backend https://test.emby.yun:4443 \
      --token  <local-admin-X-Emby-Token> \
      --source <source_id_uuid> \
      --view   3 \
      --limit  5

`--token` 从浏览器 localStorage `movie-rust-token` 取；可以是 admin 账号 token。
`--source` 是 `/api/admin/remote-emby/sources` 列表里目标源的 Id。
`--view` 是要采样的远端视图 id（可选；不传则跨视图随机采样）。
"""
from __future__ import annotations

import argparse
import json
import sys
from typing import Any, Iterable
from urllib.parse import urlencode

try:
    import requests
except ImportError:
    print("缺依赖：pip install requests", file=sys.stderr)
    raise


IMAGE_KEYS = ("Primary", "Logo", "Thumb", "Banner", "Art", "Disc", "Box", "BoxRear", "Menu")
EXPECTED_BASE_FIELDS = (
    "Overview",
    "Genres",
    "Studios",
    "Tags",
    "Taglines",
    "ProductionLocations",
    "Status",
    "EndDate",
    "AirDays",
    "AirTime",
    "PremiereDate",
    "OfficialRating",
    "CommunityRating",
    "CriticRating",
    "ProviderIds",
    "MediaSources",
    "MediaStreams",
    "People",
)


def summarize_item(item: dict[str, Any]) -> dict[str, Any]:
    image_tags = item.get("ImageTags") or {}
    backdrops = item.get("BackdropImageTags") or []
    image_coverage = {key: bool(image_tags.get(key)) for key in IMAGE_KEYS}
    base_coverage = {field: _present(item.get(field)) for field in EXPECTED_BASE_FIELDS}
    people = item.get("People") or []
    return {
        "Id": item.get("Id"),
        "Type": item.get("Type"),
        "Name": item.get("Name"),
        "ImageTagsKeys": sorted(image_tags.keys()) if isinstance(image_tags, dict) else [],
        "ImageCoverage": image_coverage,
        "BackdropCount": len(backdrops),
        "BaseFieldCoverage": base_coverage,
        "PeopleCount": len(people),
        "PeopleSample": [
            {"Name": p.get("Name"), "Type": p.get("Type"), "Role": p.get("Role")}
            for p in people[:3]
        ],
    }


def _present(value: Any) -> bool:
    if value is None:
        return False
    if isinstance(value, str):
        return bool(value.strip())
    if isinstance(value, (list, dict)):
        return bool(value)
    return True


def aggregate(items: Iterable[dict[str, Any]]) -> dict[str, Any]:
    summaries = [summarize_item(it) for it in items]
    total = len(summaries)
    if total == 0:
        return {"Total": 0}
    image_aggregate = {key: 0 for key in IMAGE_KEYS}
    base_aggregate = {field: 0 for field in EXPECTED_BASE_FIELDS}
    backdrop_total = 0
    people_with_cast = 0
    for s in summaries:
        for k, v in s["ImageCoverage"].items():
            if v:
                image_aggregate[k] += 1
        for k, v in s["BaseFieldCoverage"].items():
            if v:
                base_aggregate[k] += 1
        backdrop_total += s["BackdropCount"]
        if s["PeopleCount"] > 0:
            people_with_cast += 1
    return {
        "Total": total,
        "ImageCoverageRatio": {
            k: f"{v}/{total} ({v / total * 100:.0f}%)" for k, v in image_aggregate.items()
        },
        "BaseFieldCoverageRatio": {
            k: f"{v}/{total} ({v / total * 100:.0f}%)" for k, v in base_aggregate.items()
        },
        "AvgBackdrops": round(backdrop_total / total, 2),
        "ItemsWithCast": f"{people_with_cast}/{total}",
        "Items": summaries,
    }


def fetch_sample(backend: str, token: str, source_id: str, view: str | None, limit: int) -> dict[str, Any]:
    qs = {"limit": limit}
    if view:
        qs["parent_id"] = view
    url = f"{backend.rstrip('/')}/api/admin/remote-emby/sources/{source_id}/diagnostic/sample-items?{urlencode(qs)}"
    r = requests.get(url, headers={"X-Emby-Token": token}, timeout=60, verify=True)
    if r.status_code != 200:
        sys.stderr.write(f"诊断接口失败：HTTP {r.status_code}\n{r.text[:1000]}\n")
        sys.exit(2)
    return r.json()


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--backend", required=True, help="本地后端基址，例如 https://test.emby.yun:4443")
    ap.add_argument("--token", required=True, help="本地 admin 的 X-Emby-Token")
    ap.add_argument("--source", required=True, help="远端 Emby source 的 UUID")
    ap.add_argument("--view", default=None, help="远端视图 ID（可选）")
    ap.add_argument("--limit", type=int, default=5, help="样本数（1-50，默认 5）")
    ap.add_argument("--json", action="store_true", help="输出结构化 JSON 而不是表格摘要")
    args = ap.parse_args()

    raw = fetch_sample(args.backend, args.token, args.source, args.view, args.limit)
    items = raw.get("Items") if isinstance(raw, dict) else raw
    if not isinstance(items, list):
        sys.stderr.write(f"远端响应结构异常：\n{json.dumps(raw, ensure_ascii=False)[:1000]}\n")
        return 3
    report = aggregate(items)

    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0

    print(f"\n=== PB40 远端 Emby 字段完整性核对 ===")
    print(f"Source : {args.source}")
    print(f"View   : {args.view or '(任意)'}")
    print(f"样本数 : {report['Total']}")
    print(f"\n[图片类型] (期望全部 100%；任何 < 100% 都意味着远端缺图，需要 TMDB 兜底)")
    for k, v in report["ImageCoverageRatio"].items():
        print(f"  {k:<8}: {v}")
    print(f"\n[平均 Backdrop 数]：{report['AvgBackdrops']}（远端通常 1–4 张；> 1 则我们必须保留全部）")
    print(f"\n[基础字段] (期望全部 100%；< 100% 仅说明这部分需 TMDB 兜底)")
    for k, v in report["BaseFieldCoverageRatio"].items():
        print(f"  {k:<22}: {v}")
    print(f"\n[演职员表]: {report['ItemsWithCast']} 条目带 People")

    print(f"\n[样本明细]")
    for s in report["Items"]:
        print(f"  - {s['Type']:<8} {s['Name'][:40]:<40} ImageTags={s['ImageTagsKeys']} Backdrops={s['BackdropCount']} People={s['PeopleCount']}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
