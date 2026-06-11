#!/usr/bin/env python3
"""Minimal fake adapter for integration tests (stdlib only)."""

from __future__ import annotations

import argparse
import json
import signal
import sys
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from threading import Thread
from urllib.parse import parse_qs, urlparse

START_TIME = time.time()
LOAD_DELAY_SEC = 0.5
CHAT_DELAY_SEC = 0.0
ALLOC_ON_CHAT_MB = 0
ALLOC_ON_READY_MB = 0
ALLOCATED_ON_READY = False
ALLOCATIONS: list[bytearray] = []
STREAM_TOKENS = ["Hello", " ", "world", "!", " Done"]


class Handler(BaseHTTPRequestHandler):
    def log_message(self, format: str, *args) -> None:  # noqa: A003
        return

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        if parsed.path == "/health":
            global ALLOCATED_ON_READY
            loaded = (time.time() - START_TIME) >= LOAD_DELAY_SEC
            if loaded and ALLOC_ON_READY_MB > 0 and not ALLOCATED_ON_READY:
                ALLOCATIONS.append(bytearray(ALLOC_ON_READY_MB * 1024 * 1024))
                ALLOCATED_ON_READY = True
            body = json.dumps({"status": "ok", "model_loaded": loaded})
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body.encode())
            return

        if parsed.path == "/metrics":
            body = json.dumps(
                {
                    "mlx_active_bytes": 1024 * 1024,
                    "mlx_peak_bytes": 2 * 1024 * 1024,
                    "mlx_cache_bytes": 512 * 1024,
                }
            )
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body.encode())
            return

        if parsed.path == "/alloc":
            qs = parse_qs(parsed.query)
            mb = int(qs.get("mb", ["0"])[0])
            ALLOCATIONS.append(bytearray(mb * 1024 * 1024))
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"ok")
            return

        self.send_response(404)
        self.end_headers()

    def do_POST(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        if parsed.path != "/v1/chat/completions":
            self.send_response(404)
            self.end_headers()
            return

        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length else b"{}"
        try:
            payload = json.loads(raw.decode("utf-8"))
        except json.JSONDecodeError:
            payload = {}

        stream = bool(payload.get("stream", False))
        prompt_tokens = max(1, len(str(payload.get("messages", ""))) // 4)

        if ALLOC_ON_CHAT_MB > 0:
            ALLOCATIONS.append(bytearray(ALLOC_ON_CHAT_MB * 1024 * 1024))

        if not stream:
            body = json.dumps(
                {
                    "choices": [
                        {
                            "message": {
                                "role": "assistant",
                                "content": "".join(STREAM_TOKENS),
                            }
                        }
                    ],
                    "usage": {
                        "prompt_tokens": prompt_tokens,
                        "completion_tokens": len(STREAM_TOKENS),
                    },
                }
            )
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(body.encode())
            return

        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()

        per_token_delay = CHAT_DELAY_SEC / max(len(STREAM_TOKENS), 1)
        for token in STREAM_TOKENS:
            chunk = {
                "choices": [{"delta": {"content": token}}],
            }
            line = f"data: {json.dumps(chunk)}\n\n"
            self.wfile.write(line.encode())
            self.wfile.flush()
            time.sleep(0.02 + per_token_delay)

        usage_chunk = {
            "choices": [{"delta": {}}],
            "usage": {
                "prompt_tokens": prompt_tokens,
                "completion_tokens": len(STREAM_TOKENS),
            },
        }
        self.wfile.write(f"data: {json.dumps(usage_chunk)}\n\n".encode())
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


def handle_sigterm(_signum: int, _frame) -> None:
    sys.exit(0)


def main() -> None:
    global START_TIME, LOAD_DELAY_SEC, CHAT_DELAY_SEC, ALLOC_ON_CHAT_MB, ALLOC_ON_READY_MB, ALLOCATED_ON_READY

    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--load-delay-sec", type=float, default=0.5)
    parser.add_argument("--chat-delay-sec", type=float, default=0.0)
    parser.add_argument("--alloc-on-chat-mb", type=int, default=0)
    parser.add_argument("--alloc-on-ready-mb", type=int, default=0)
    # 기동 즉시 할당: 로딩 단계 내내 한계 초과 상태를 유지해
    # 100ms 와치독 틱이 반드시 잡도록 한다 (레이스 없는 결정적 테스트용)
    parser.add_argument("--alloc-at-start-mb", type=int, default=0)
    args = parser.parse_args()

    if args.alloc_at_start_mb > 0:
        ALLOCATIONS.append(bytearray(args.alloc_at_start_mb * 1024 * 1024))

    START_TIME = time.time()
    LOAD_DELAY_SEC = args.load_delay_sec
    CHAT_DELAY_SEC = args.chat_delay_sec
    ALLOC_ON_CHAT_MB = args.alloc_on_chat_mb
    ALLOC_ON_READY_MB = args.alloc_on_ready_mb
    ALLOCATED_ON_READY = False

    signal.signal(signal.SIGTERM, handle_sigterm)

    server = HTTPServer(("127.0.0.1", args.port), Handler)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    thread.join()


if __name__ == "__main__":
    main()