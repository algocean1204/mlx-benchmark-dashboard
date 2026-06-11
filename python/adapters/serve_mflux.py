"""mflux backend adapter — image generation — /v1/images/generations."""

from __future__ import annotations

import argparse
import base64
import io
import json
import logging
import random
import signal
import sys
import threading
import time
from pathlib import Path
from typing import Any

import mlx.core as mx
import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.responses import JSONResponse
from pydantic import BaseModel

_UVICORN_SERVER: uvicorn.Server | None = None
_SHUTDOWN = threading.Event()

_MODEL = None
_MODEL_CONFIG = None
_LOAD_ERROR: str | None = None
_MODEL_LOADED = False
_LOAD_LOCK = threading.Lock()
_SERVED_MODEL_NAME = ""


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


def _mlx_metrics_payload() -> dict[str, int]:
    return {
        "mlx_active_bytes": int(mx.get_active_memory()),
        "mlx_peak_bytes": int(mx.get_peak_memory()),
        "mlx_cache_bytes": int(mx.get_cache_memory()),
    }


def _resolve_model_path(model_path: str) -> str:
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


def _pick_model_config(profile: dict[str, Any]):
    from mflux.models.common.config.model_config import ModelConfig

    hint = str(profile.get("notes", "")).lower()
    model_name = str(profile.get("id", "")).lower()
    combined = f"{hint} {model_name}"
    if "schnell" in combined:
        return ModelConfig.schnell()
    if "dev" in combined:
        return ModelConfig.dev()
    return ModelConfig.schnell()


def _background_model_load(model_path: str, profile: dict[str, Any]) -> None:
    global _MODEL, _MODEL_CONFIG, _LOAD_ERROR, _MODEL_LOADED

    try:
        from mflux.models.flux.variants.txt2img.flux import Flux1

        model_config = _pick_model_config(profile)
        _log_json("model_load_start", model_path=model_path)
        model = Flux1(model_path=model_path, model_config=model_config)
        with _LOAD_LOCK:
            _MODEL = model
            _MODEL_CONFIG = model_config
            _LOAD_ERROR = None
            _MODEL_LOADED = True
        _log_json("model_loaded", model_path=model_path)
    except Exception as exc:
        with _LOAD_LOCK:
            _LOAD_ERROR = str(exc)
            _MODEL_LOADED = False
        _log_json("model_load_failed", error=str(exc))


class ImageGenerationRequest(BaseModel):
    prompt: str
    n: int = 1
    size: str | None = "1024x1024"
    model: str | None = None


app = FastAPI()


@app.get("/health")
async def health() -> JSONResponse:
    with _LOAD_LOCK:
        loaded = _MODEL_LOADED
        error = _LOAD_ERROR

    status = "ok"
    if error is not None:
        status = "error"

    return JSONResponse({"status": status, "model_loaded": loaded})


@app.get("/metrics")
async def metrics() -> JSONResponse:
    return JSONResponse(_mlx_metrics_payload())


def _require_model() -> Any:
    with _LOAD_LOCK:
        if _LOAD_ERROR is not None:
            raise HTTPException(
                status_code=503, detail=f"model load failed: {_LOAD_ERROR}"
            )
        if not _MODEL_LOADED or _MODEL is None:
            raise HTTPException(status_code=503, detail="model not loaded")
        return _MODEL


def _parse_size(size: str | None) -> tuple[int, int]:
    if not size:
        return 1024, 1024
    try:
        width_str, height_str = size.lower().split("x", 1)
        return int(width_str), int(height_str)
    except Exception as exc:
        raise HTTPException(status_code=400, detail=f"invalid size: {size}") from exc


@app.post("/v1/images/generations")
async def images_generations(body: ImageGenerationRequest) -> JSONResponse:
    model = _require_model()
    if not body.prompt.strip():
        raise HTTPException(status_code=400, detail="prompt must not be empty")

    width, height = _parse_size(body.size)
    count = max(1, min(body.n, 4))
    data: list[dict[str, str]] = []

    for _ in range(count):
        seed = random.randint(0, 2**31 - 1)
        generated = model.generate_image(
            seed=seed,
            prompt=body.prompt,
            width=width,
            height=height,
        )
        buffer = io.BytesIO()
        generated.image.save(buffer, format="PNG")
        b64 = base64.b64encode(buffer.getvalue()).decode("ascii")
        data.append({"b64_json": b64})

    return JSONResponse(
        {
            "created": int(time.time()),
            "data": data,
        }
    )


def _handle_sigterm(_signum: int, _frame: Any) -> None:
    _log_json("shutdown", signal="SIGTERM")
    _SHUTDOWN.set()
    if _UVICORN_SERVER is not None:
        _UVICORN_SERVER.should_exit = True


def main() -> None:
    global _UVICORN_SERVER, _SERVED_MODEL_NAME, _MODEL, _MODEL_CONFIG

    parser = argparse.ArgumentParser(description="aidash mflux adapter")
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
    _SERVED_MODEL_NAME = args.model_path

    try:
        resolved_model_path = _resolve_model_path(args.model_path)
    except Exception as exc:
        _log_json("startup_failed", error=str(exc))
        sys.exit(1)

    load_thread = threading.Thread(
        target=_background_model_load,
        args=(resolved_model_path, profile),
        daemon=True,
    )
    load_thread.start()

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

    with _LOAD_LOCK:
        _MODEL = None
        _MODEL_CONFIG = None

    _log_json("exit", code=0)
    sys.exit(0)


if __name__ == "__main__":
    main()