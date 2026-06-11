"""mlx-audio backend adapter — TTS — /v1/audio/speech."""

from __future__ import annotations

import argparse
import io
import json
import logging
import queue
import signal
import sys
import threading
from pathlib import Path
from typing import Any

import mlx.core as mx
import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.responses import Response
from pydantic import BaseModel

from mlx_audio.audio_io import write as audio_write
from mlx_audio.tts.utils import load as load_tts_model

_UVICORN_SERVER: uvicorn.Server | None = None
_SHUTDOWN = threading.Event()

_MODEL = None
_LOAD_ERROR: str | None = None
_MODEL_LOADED = False
_LOAD_LOCK = threading.Lock()
_SERVED_MODEL_NAME = ""
_WORKER_QUEUE: queue.Queue | None = None
_WORKER_THREAD: threading.Thread | None = None


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


def _worker_loop(task_queue: queue.Queue) -> None:
    global _MODEL, _LOAD_ERROR, _MODEL_LOADED

    while True:
        task = task_queue.get()
        if task is None:
            break

        kind = task[0]
        if kind == "load":
            model_path = task[1]
            try:
                _log_json("model_load_start", model_path=model_path)
                model = load_tts_model(model_path)
                with _LOAD_LOCK:
                    _MODEL = model
                    _LOAD_ERROR = None
                    _MODEL_LOADED = True
                _log_json("model_loaded", model_path=model_path)
            except Exception as exc:
                with _LOAD_LOCK:
                    _LOAD_ERROR = str(exc)
                    _MODEL_LOADED = False
                _log_json("model_load_failed", error=str(exc))
        elif kind == "generate":
            _, text, gen_kwargs, done_event, result_box = task
            try:
                if _MODEL is None:
                    raise RuntimeError("model not loaded")
                result_box["chunks"] = list(_MODEL.generate(text, **gen_kwargs))
            except Exception as exc:
                result_box["error"] = exc
            finally:
                done_event.set()


def _start_worker() -> queue.Queue:
    global _WORKER_QUEUE, _WORKER_THREAD
    task_queue: queue.Queue = queue.Queue()
    worker = threading.Thread(target=_worker_loop, args=(task_queue,), daemon=True)
    worker.start()
    _WORKER_QUEUE = task_queue
    _WORKER_THREAD = worker
    return task_queue


def _enqueue_model_load(model_path: str) -> None:
    if _WORKER_QUEUE is None:
        raise RuntimeError("worker not started")
    _WORKER_QUEUE.put(("load", model_path))


def _generate_on_worker(text: str, gen_kwargs: dict[str, Any]) -> list[Any]:
    if _WORKER_QUEUE is None:
        raise RuntimeError("worker not started")
    done_event = threading.Event()
    result_box: dict[str, Any] = {}
    _WORKER_QUEUE.put(("generate", text, gen_kwargs, done_event, result_box))
    done_event.wait()
    if "error" in result_box:
        raise result_box["error"]
    return result_box["chunks"]


class SpeechRequest(BaseModel):
    input: str
    voice: str | None = None
    model: str | None = None
    response_format: str | None = "wav"


app = FastAPI()


@app.get("/health")
async def health() -> Response:
    with _LOAD_LOCK:
        loaded = _MODEL_LOADED
        error = _LOAD_ERROR

    status = "ok"
    if error is not None:
        status = "error"

    return Response(
        content=json.dumps({"status": status, "model_loaded": loaded}),
        media_type="application/json",
    )


@app.get("/metrics")
async def metrics() -> Response:
    return Response(
        content=json.dumps(_mlx_metrics_payload()),
        media_type="application/json",
    )


def _require_model() -> Any:
    with _LOAD_LOCK:
        if _LOAD_ERROR is not None:
            raise HTTPException(
                status_code=503, detail=f"model load failed: {_LOAD_ERROR}"
            )
        if not _MODEL_LOADED or _MODEL is None:
            raise HTTPException(status_code=503, detail="model not loaded")
        return _MODEL


def _audio_to_wav_bytes(audio: Any, sample_rate: int) -> bytes:
    buffer = io.BytesIO()
    audio_write(buffer, audio, sample_rate, format="wav")
    buffer.seek(0)
    return buffer.read()


@app.post("/v1/audio/speech")
def audio_speech(body: SpeechRequest) -> Response:
    model = _require_model()
    if not body.input.strip():
        raise HTTPException(status_code=400, detail="input must not be empty")

    gen_kwargs: dict[str, Any] = {"verbose": False, "stream": False}
    if body.voice:
        gen_kwargs["voice"] = body.voice

    del model
    try:
        chunks = _generate_on_worker(body.input, gen_kwargs)
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc

    if not chunks:
        raise HTTPException(status_code=500, detail="no audio generated")

    wav_bytes = _audio_to_wav_bytes(chunks[-1].audio, chunks[-1].sample_rate)
    return Response(content=wav_bytes, media_type="audio/wav")


def _handle_sigterm(_signum: int, _frame: Any) -> None:
    _log_json("shutdown", signal="SIGTERM")
    _SHUTDOWN.set()
    if _UVICORN_SERVER is not None:
        _UVICORN_SERVER.should_exit = True


def main() -> None:
    global _UVICORN_SERVER, _SERVED_MODEL_NAME, _MODEL

    parser = argparse.ArgumentParser(description="aidash mlx-audio adapter")
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
    _SERVED_MODEL_NAME = args.model_path

    try:
        resolved_model_path = _resolve_model_path(args.model_path)
    except Exception as exc:
        _log_json("startup_failed", error=str(exc))
        sys.exit(1)

    _start_worker()
    _enqueue_model_load(resolved_model_path)

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

    if _WORKER_QUEUE is not None:
        _WORKER_QUEUE.put(None)

    with _LOAD_LOCK:
        _MODEL = None

    _log_json("exit", code=0)
    sys.exit(0)


if __name__ == "__main__":
    main()