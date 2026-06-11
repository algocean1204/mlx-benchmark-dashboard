"""mlx-whisper backend adapter — ASR — /v1/audio/transcriptions."""

from __future__ import annotations

import argparse
import json
import logging
import signal
import sys
import tempfile
import threading
from pathlib import Path
from typing import Any

import mlx.core as mx
import uvicorn
from fastapi import FastAPI, File, Form, HTTPException, UploadFile
from fastapi.responses import JSONResponse

from mlx_whisper import transcribe
from mlx_whisper.load_models import load_model

_UVICORN_SERVER: uvicorn.Server | None = None
_SHUTDOWN = threading.Event()

_MODEL = None
_MODEL_PATH = ""
_LOAD_ERROR: str | None = None
_MODEL_LOADED = False
_LOAD_LOCK = threading.Lock()


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


def _background_model_load(model_path: str) -> None:
    global _MODEL, _MODEL_PATH, _LOAD_ERROR, _MODEL_LOADED

    try:
        _log_json("model_load_start", model_path=model_path)
        model = load_model(model_path)
        with _LOAD_LOCK:
            _MODEL = model
            _MODEL_PATH = model_path
            _LOAD_ERROR = None
            _MODEL_LOADED = True
        _log_json("model_loaded", model_path=model_path)
    except Exception as exc:
        with _LOAD_LOCK:
            _LOAD_ERROR = str(exc)
            _MODEL_LOADED = False
        _log_json("model_load_failed", error=str(exc))


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


def _require_model() -> str:
    with _LOAD_LOCK:
        if _LOAD_ERROR is not None:
            raise HTTPException(
                status_code=503, detail=f"model load failed: {_LOAD_ERROR}"
            )
        if not _MODEL_LOADED or _MODEL is None:
            raise HTTPException(status_code=503, detail="model not loaded")
        return _MODEL_PATH


@app.post("/v1/audio/transcriptions")
def audio_transcriptions(
    file: UploadFile = File(...),
    model: str | None = Form(default=None),
    language: str | None = Form(default=None),
) -> JSONResponse:
    model_path = _require_model()
    suffix = Path(file.filename or "audio.wav").suffix or ".wav"
    tmp = tempfile.NamedTemporaryFile(delete=False, suffix=suffix)
    try:
        content = file.file.read()
        tmp.write(content)
        tmp.flush()
        tmp.close()

        decode_options: dict[str, Any] = {"fp16": True}
        if language:
            decode_options["language"] = language

        result = transcribe(
            tmp.name,
            path_or_hf_repo=model_path,
            verbose=False,
            **decode_options,
        )
        text = str(result.get("text", ""))
        return JSONResponse({"text": text})
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc
    finally:
        try:
            Path(tmp.name).unlink(missing_ok=True)
        except Exception:
            pass


def _handle_sigterm(_signum: int, _frame: Any) -> None:
    _log_json("shutdown", signal="SIGTERM")
    _SHUTDOWN.set()
    if _UVICORN_SERVER is not None:
        _UVICORN_SERVER.should_exit = True


def main() -> None:
    global _UVICORN_SERVER, _MODEL, _MODEL_PATH

    parser = argparse.ArgumentParser(description="aidash mlx-whisper adapter")
    parser.add_argument("--model-path", required=True)
    parser.add_argument("--context-size", type=int, required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--profile-json", required=True)
    args = parser.parse_args()

    _configure_logging()

    try:
        json.loads(args.profile_json)
    except json.JSONDecodeError as exc:
        _log_json("startup_failed", error=f"invalid profile-json: {exc}")
        sys.exit(1)

    signal.signal(signal.SIGTERM, _handle_sigterm)

    try:
        resolved_model_path = _resolve_model_path(args.model_path)
    except Exception as exc:
        _log_json("startup_failed", error=str(exc))
        sys.exit(1)

    load_thread = threading.Thread(
        target=_background_model_load,
        args=(resolved_model_path,),
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
        _MODEL_PATH = ""

    _log_json("exit", code=0)
    sys.exit(0)


if __name__ == "__main__":
    main()