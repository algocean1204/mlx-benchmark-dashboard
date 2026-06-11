"""vllm-mlx backend adapter — LLM (main) — /v1/chat/completions (stream).

Uses vllm-mlx 0.3.0's in-process FastAPI app (``vllm_mlx.server.app``) for
OpenAI-compatible endpoints.  The server starts before model load (lazy residency)
and contract-specific ``/health`` + ``/metrics`` routes are layered on top.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import logging
import signal
import sys
import threading
from contextlib import asynccontextmanager
from typing import Any

import mlx.core as mx
import uvicorn
from fastapi.responses import JSONResponse

import vllm_mlx.server as vllm_server
from vllm_mlx.scheduler import SchedulerConfig
from vllm_mlx.server import app, load_model

_UVICORN_SERVER: uvicorn.Server | None = None
_SHUTDOWN = threading.Event()


def _log_json(event: str, **fields: Any) -> None:
    payload = {"event": event, **fields}
    print(json.dumps(payload, ensure_ascii=False), flush=True)


class _JsonStdoutHandler(logging.Handler):
    def emit(self, record: logging.LogRecord) -> None:
        try:
            payload = {
                "event": "log",
                "level": record.levelname.lower(),
                "message": self.format(record),
            }
            print(json.dumps(payload, ensure_ascii=False), flush=True)
        except Exception:
            self.handleError(record)


def _configure_logging() -> None:
    root = logging.getLogger()
    root.handlers.clear()
    handler = _JsonStdoutHandler()
    handler.setFormatter(logging.Formatter("%(message)s"))
    root.addHandler(handler)
    root.setLevel(logging.INFO)
    logging.getLogger("vllm_mlx").propagate = False


def _mlx_metrics_payload() -> dict[str, int]:
    return {
        "mlx_active_bytes": int(mx.get_active_memory()),
        "mlx_peak_bytes": int(mx.get_peak_memory()),
        "mlx_cache_bytes": int(mx.get_cache_memory()),
    }


def _replace_route(path: str) -> None:
    app.router.routes = [
        route
        for route in app.router.routes
        if not (getattr(route, "path", None) == path)
    ]


async def _aidash_metrics() -> JSONResponse:
    return JSONResponse(_mlx_metrics_payload())


async def _aidash_health() -> JSONResponse:
    engine = vllm_server._engine
    residency = vllm_server._get_lifecycle_status()
    model_loaded = engine is not None
    if residency is not None and residency.get("state") == "loaded":
        model_loaded = engine is not None

    status = "ok"
    if residency is not None and residency.get("state") == "failed":
        status = "error"

    return JSONResponse({"status": status, "model_loaded": model_loaded})


def _install_aidash_routes() -> None:
    _replace_route("/metrics")
    _replace_route("/health")
    app.add_api_route("/metrics", _aidash_metrics, methods=["GET"])
    app.add_api_route("/health", _aidash_health, methods=["GET"])


async def _background_model_load() -> None:
    manager = vllm_server._residency_manager
    model_key = vllm_server._default_model_key
    if manager is None or model_key is None:
        _log_json("model_load_skipped", reason="residency_disabled")
        return

    try:
        _log_json("model_load_start", model_key=model_key)
        await manager.ensure_loaded(model_key)
        vllm_server._sync_engine_from_residency()
        _log_json("model_loaded", model_key=model_key)
    except Exception as exc:
        _log_json("model_load_failed", error=str(exc))


def _wrap_lifespan() -> None:
    original = app.router.lifespan_context

    @asynccontextmanager
    async def aidash_lifespan(fastapi_app):
        async with original(fastapi_app):
            asyncio.create_task(_background_model_load())
            yield

    app.router.lifespan_context = aidash_lifespan


def _resolve_model_path(model_path: str) -> str:
    from pathlib import Path

    local = Path(model_path).expanduser()
    if local.is_dir():
        _log_json("model_resolve_local_path", path=str(local))
        return str(local)

    from huggingface_hub import snapshot_download

    try:
        resolved = snapshot_download(repo_id=model_path, local_files_only=True)
        _log_json("model_resolve_cache", repo_id=model_path, path=resolved)
        return resolved
    except Exception as exc:
        _log_json("model_resolve_online_fallback", repo_id=model_path, reason=str(exc))
        resolved = snapshot_download(repo_id=model_path, local_files_only=False)
        _log_json("model_resolve_online", repo_id=model_path, path=resolved)
        return resolved


def _handle_sigterm(_signum: int, _frame: Any) -> None:
    _log_json("shutdown", signal="SIGTERM")
    _SHUTDOWN.set()
    if _UVICORN_SERVER is not None:
        _UVICORN_SERVER.should_exit = True


def main() -> None:
    global _UVICORN_SERVER

    parser = argparse.ArgumentParser(description="aidash vllm-mlx adapter")
    parser.add_argument("--model-path", required=True)
    parser.add_argument("--context-size", type=int, required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--profile-json", required=True)
    args = parser.parse_args()

    _configure_logging()

    try:
        profile = json.loads(args.profile_json)
    except json.JSONDecodeError as exc:
        _log_json("startup_failed", error=f"invalid profile-json: {exc}")
        sys.exit(1)

    signal.signal(signal.SIGTERM, _handle_sigterm)

    vllm_server._metrics_enabled = False
    vllm_server._metrics.configure(enabled=False)

    _install_aidash_routes()
    _wrap_lifespan()

    scheduler_config = SchedulerConfig(max_kv_size=args.context_size)
    max_request_tokens = max(args.context_size, 1)
    default_max_tokens = min(
        max_request_tokens,
        profile.get("default_params", {}).get("max_tokens", 512) or 512,
    )

    try:
        resolved_model_path = _resolve_model_path(args.model_path)
        load_model(
            resolved_model_path,
            scheduler_config=scheduler_config,
            max_tokens=int(default_max_tokens),
            max_request_tokens=max_request_tokens,
            served_model_name=args.model_path,
            lazy_load_model=True,
        )
    except Exception as exc:
        _log_json("startup_failed", error=str(exc))
        sys.exit(1)

    _log_json(
        "server_starting",
        port=args.port,
        model_path=args.model_path,
        context_size=args.context_size,
    )

    config = uvicorn.Config(
        app,
        host="127.0.0.1",
        port=args.port,
        log_level="warning",
        access_log=False,
    )
    _UVICORN_SERVER = uvicorn.Server(config)

    server_thread = threading.Thread(target=_UVICORN_SERVER.run, daemon=False)
    server_thread.start()
    _log_json("server_started", port=args.port)

    try:
        while server_thread.is_alive() and not _SHUTDOWN.is_set():
            server_thread.join(timeout=0.25)
    except KeyboardInterrupt:
        _SHUTDOWN.set()
        if _UVICORN_SERVER is not None:
            _UVICORN_SERVER.should_exit = True
        server_thread.join(timeout=5.0)

    _log_json("exit", code=0)
    sys.exit(0)


if __name__ == "__main__":
    main()