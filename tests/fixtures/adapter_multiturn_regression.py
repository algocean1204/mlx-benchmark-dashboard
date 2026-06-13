#!/usr/bin/env python3
"""Adapter standalone multiturn regression (27B 4K). Stdlib + urllib only."""

from __future__ import annotations

import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROFILE_ID = "mlx-community/Qwen3.6-27B-OBLITERATED-MLX-4bit"
CONTEXT = 4096
PORT = 18743
LOAD_TIMEOUT_SEC = 600


def mem_stats() -> tuple[float, float, float]:
    out = subprocess.check_output(["vm_stat"]).decode()
    pages: dict[str, int] = {}
    for line in out.splitlines():
        m = re.match(r"Pages\s+(\w+):\s+(\d+)", line)
        if m:
            pages[m.group(1)] = int(m.group(2))
    page_size = int(subprocess.check_output(["sysctl", "-n", "hw.pagesize"]).decode().strip())
    total = int(subprocess.check_output(["sysctl", "-n", "hw.memsize"]).decode().strip())
    avail = (pages.get("free", 0) + pages.get("inactive", 0)) * page_size
    return total / (1024**3), avail / (1024**3), avail / total * 100


def load_profile() -> dict:
    slug = PROFILE_ID.replace("/", "-").lower()
    path = ROOT / "profiles" / f"{slug}.json"
    return json.loads(path.read_text())


def http_json(url: str, payload: dict | None = None, timeout: float = 300.0) -> dict:
    data = None
    headers = {"Content-Type": "application/json"}
    if payload is not None:
        data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers=headers, method="POST" if payload else "GET")
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode())


def wait_health() -> None:
    deadline = time.time() + LOAD_TIMEOUT_SEC
    while time.time() < deadline:
        try:
            body = http_json(f"http://127.0.0.1:{PORT}/health", timeout=5.0)
            if body.get("model_loaded"):
                print(f"health ready: {body}")
                return
            if body.get("status") == "error":
                raise RuntimeError(f"adapter health error: {body}")
        except urllib.error.URLError:
            pass
        time.sleep(0.5)
    raise TimeoutError("model load timeout")


def chat(messages: list[dict]) -> tuple[str, dict]:
    payload = {
        "model": PROFILE_ID,
        "messages": messages,
        "stream": False,
        "max_tokens": 128,
    }
    body = http_json(
        f"http://127.0.0.1:{PORT}/v1/chat/completions",
        payload,
        timeout=300.0,
    )
    text = body["choices"][0]["message"]["content"]
    usage = body.get("usage") or {}
    return text, usage


def main() -> int:
    total_gb, avail_gb, avail_pct = mem_stats()
    print(f"memory: total={total_gb:.1f}GB avail={avail_gb:.1f}GB ({avail_pct:.1f}%)")
    if avail_pct < 50:
        print(f"WARN: available memory {avail_pct:.1f}% < 50% threshold")

    profile = load_profile()
    profile_json = json.dumps(profile)
    model_path = profile["source"]["hf_repo"]

    uv = subprocess.check_output(["which", "uv"]).decode().strip()
    cmd = [
        uv,
        "run",
        "--project",
        str(ROOT / "python"),
        "python",
        "-m",
        "adapters.serve_vllm_mlx",
        "--model-path",
        model_path,
        "--context-size",
        str(CONTEXT),
        "--port",
        str(PORT),
        "--profile-json",
        profile_json,
    ]
    print("starting adapter:", " ".join(cmd[:8]), "...")
    proc = subprocess.Popen(cmd, cwd=ROOT)
    try:
        wait_health()

        turn1_user = "비밀 코드는 ALPHA-42 입니다. 꼭 기억하세요."
        reply1, usage1 = chat([{"role": "user", "content": turn1_user}])
        print(f"turn1 usage: {usage1}")
        print(f"turn1 reply excerpt: {reply1[:120]!r}")

        turn2_user = "제가 말한 비밀 코드가 뭐였죠?"
        messages = [
            {"role": "user", "content": turn1_user},
            {"role": "assistant", "content": reply1},
            {"role": "user", "content": turn2_user},
        ]
        reply2, usage2 = chat(messages)
        print(f"turn2 usage: {usage2}")
        print(f"turn2 reply excerpt: {reply2[:200]!r}")

        if "ALPHA-42" not in reply2.upper().replace("-", "-"):
            # allow case variants
            normalized = reply2.upper()
            if "ALPHA" not in normalized or "42" not in normalized:
                print("FAIL: turn2 did not reference first-turn secret code")
                return 1

        if not usage2.get("prompt_tokens") or not usage2.get("completion_tokens"):
            print(f"FAIL: missing usage in turn2: {usage2}")
            return 1

        print("PASS: multiturn reference + usage OK")
        return 0
    finally:
        print("stopping adapter...")
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
        print(f"adapter exit code: {proc.returncode}")


if __name__ == "__main__":
    sys.exit(main())