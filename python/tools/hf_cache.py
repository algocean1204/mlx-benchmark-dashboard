#!/usr/bin/env python3
"""HF hub cache scan/delete — stdout JSON only, logs to stderr."""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path


def _log(msg: str) -> None:
    print(msg, file=sys.stderr)


def cmd_scan() -> int:
    try:
        from huggingface_hub import scan_cache_dir
    except ImportError:
        print(
            json.dumps({"error": "huggingface_hub not installed"}),
            file=sys.stdout,
        )
        return 1

    from huggingface_hub.constants import HF_HUB_CACHE

    info = scan_cache_dir()
    repos = []
    total_size = 0
    for repo in sorted(info.repos, key=lambda r: r.repo_id):
        size = repo.size_on_disk
        total_size += size
        def _ts_iso(ts) -> str | None:
            if ts is None:
                return None
            if isinstance(ts, (int, float)):
                return datetime.fromtimestamp(ts, tz=timezone.utc).isoformat()
            if hasattr(ts, "isoformat"):
                return ts.isoformat()
            return str(ts)

        revisions = [
            {
                "revision": rev.commit_hash,
                "size_bytes": rev.size_on_disk,
                "last_modified": _ts_iso(rev.last_modified),
            }
            for rev in repo.revisions
        ]
        last_modified = None
        if repo.revisions:
            dates = [r.last_modified for r in repo.revisions if r.last_modified]
            if dates:
                last_modified = _ts_iso(max(dates))

        repos.append(
            {
                "repo_id": repo.repo_id,
                "size_bytes": size,
                "last_modified": last_modified,
                "revisions": revisions,
            }
        )

    repos.sort(key=lambda r: r.get("last_modified") or "", reverse=True)
    out = {
        "cache_dir": str(HF_HUB_CACHE),
        "total_size_bytes": info.size_on_disk,
        "repo_count": len(repos),
        "repos": repos,
    }
    print(json.dumps(out))
    return 0


def cmd_delete(repo_id: str) -> int:
    try:
        from huggingface_hub import scan_cache_dir
    except ImportError:
        print(
            json.dumps({"error": "huggingface_hub not installed"}),
            file=sys.stdout,
        )
        return 1

    info = scan_cache_dir()
    target = None
    for repo in info.repos:
        if repo.repo_id == repo_id:
            target = repo
            break

    if target is None:
        print(
            json.dumps(
                {
                    "repo_id": repo_id,
                    "deleted": False,
                    "error": "repo not found in cache",
                }
            )
        )
        return 1

    _log(f"Deleting all revisions of {repo_id} ({target.size_on_disk} bytes)")
    hashes = [rev.commit_hash for rev in target.revisions]
    strategy = info.delete_revisions(*hashes)
    expected = strategy.expected_freed_size
    try:
        strategy.execute()
    except Exception as exc:  # noqa: BLE001
        print(
            json.dumps(
                {
                    "repo_id": repo_id,
                    "deleted": False,
                    "freed_bytes": 0,
                    "error": str(exc),
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                }
            )
        )
        return 1

    result = {
        "repo_id": repo_id,
        "deleted": True,
        "freed_bytes": expected,
        "timestamp": datetime.now(timezone.utc).isoformat(),
    }
    print(json.dumps(result))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="HF cache utilities")
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("scan", help="Scan HF cache and output JSON")

    delete_p = sub.add_parser("delete", help="Delete a repo from HF cache")
    delete_p.add_argument("--repo", required=True, help="HF repo id (org/name)")

    args = parser.parse_args()

    if args.command == "scan":
        return cmd_scan()
    if args.command == "delete":
        return cmd_delete(args.repo)

    return 1


if __name__ == "__main__":
    raise SystemExit(main())