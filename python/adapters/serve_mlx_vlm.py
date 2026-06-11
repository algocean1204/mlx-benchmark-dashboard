"""mlx-vlm backend adapter — multimodal — /v1/chat/completions (image content parts)."""

from __future__ import annotations

import argparse
import base64
import json
import logging
import queue
import re
import signal
import sys
import tempfile
import threading
import time
import uuid
from pathlib import Path
from typing import Any

import mlx.core as mx
import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.responses import JSONResponse, StreamingResponse
from pydantic import BaseModel

from mlx_vlm import load, stream_generate
from mlx_vlm.prompt_utils import apply_chat_template

_UVICORN_SERVER: uvicorn.Server | None = None
_SHUTDOWN = threading.Event()

_MODEL = None
_PROCESSOR = None
_LOAD_ERROR: str | None = None
_MODEL_LOADED = False
_LOAD_LOCK = threading.Lock()
_SERVED_MODEL_NAME = ""
_TEMP_IMAGE_FILES: list[str] = []
_MLX_CMD_QUEUE: queue.Queue[tuple[Any, ...]] = queue.Queue()
_MLX_THREAD: threading.Thread | None = None
_MLX_THREAD_LOCK = threading.Lock()


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


_DATA_URL_RE = re.compile(r"^data:(?P<mime>[^;]+);base64,(?P<data>.+)$", re.DOTALL)


def _materialize_image_url(url: str) -> str:
    if not url.startswith("data:"):
        return url

    match = _DATA_URL_RE.match(url.strip())
    if not match:
        raise ValueError("invalid image data URL")

    mime = match.group("mime").lower()
    ext = ".png"
    if "jpeg" in mime or "jpg" in mime:
        ext = ".jpg"
    elif "webp" in mime:
        ext = ".webp"
    elif "gif" in mime:
        ext = ".gif"

    raw = base64.b64decode(match.group("data"))
    tmp = tempfile.NamedTemporaryFile(delete=False, suffix=ext)
    tmp.write(raw)
    tmp.flush()
    tmp.close()
    _TEMP_IMAGE_FILES.append(tmp.name)
    return tmp.name


def _extract_text(content: Any) -> str:
    if isinstance(content, str):
        return content
    if not isinstance(content, list):
        return str(content)

    parts: list[str] = []
    for item in content:
        if not isinstance(item, dict):
            continue
        item_type = item.get("type")
        if item_type == "text":
            parts.append(str(item.get("text", "")))
        elif item_type in {"image_url", "input_image"}:
            continue
        else:
            text = item.get("text")
            if text:
                parts.append(str(text))
    return "\n".join(p for p in parts if p)


def _extract_images(content: Any) -> list[str]:
    if not isinstance(content, list):
        return []

    images: list[str] = []
    for item in content:
        if not isinstance(item, dict):
            continue
        item_type = item.get("type")
        if item_type == "image_url":
            image_url = item.get("image_url", {})
            url = image_url.get("url") if isinstance(image_url, dict) else image_url
            if url:
                images.append(_materialize_image_url(str(url)))
        elif item_type == "input_image":
            url = item.get("image_url")
            if url:
                images.append(_materialize_image_url(str(url)))
    return images


_STREAM_DONE = object()


def _ensure_mlx_thread() -> None:
    global _MLX_THREAD

    with _MLX_THREAD_LOCK:
        if _MLX_THREAD is not None and _MLX_THREAD.is_alive():
            return
        _MLX_THREAD = threading.Thread(
            target=_mlx_worker_loop,
            name="mlx-worker",
            daemon=True,
        )
        _MLX_THREAD.start()


def _mlx_worker_loop() -> None:
    """모델 로드·생성을 동일 MLX 스레드에서 직렬 처리한다."""
    global _MODEL, _PROCESSOR, _LOAD_ERROR, _MODEL_LOADED

    while True:
        cmd = _MLX_CMD_QUEUE.get()
        kind = cmd[0]
        if kind == "shutdown":
            break
        if kind == "load":
            _, model_path = cmd
            try:
                _log_json("model_load_start", model_path=model_path)
                model, processor = load(model_path)
                with _LOAD_LOCK:
                    _MODEL = model
                    _PROCESSOR = processor
                    _LOAD_ERROR = None
                    _MODEL_LOADED = True
                _log_json("model_loaded", model_path=model_path)
            except Exception as exc:
                with _LOAD_LOCK:
                    _LOAD_ERROR = str(exc)
                    _MODEL_LOADED = False
                _log_json("model_load_failed", error=str(exc))
            continue

        if kind == "stream":
            _, out_queue, job = cmd
            try:
                prompt_tokens = 0
                completion_tokens = 0
                for response in stream_generate(
                    job["model"],
                    job["processor"],
                    job["prompt"],
                    image=job["image_arg"],
                    max_tokens=job["max_tokens"],
                    **job["gen_kwargs"],
                ):
                    prompt_tokens = int(response.prompt_tokens)
                    completion_tokens = int(response.generation_tokens)
                    chunk = {
                        "id": job["completion_id"],
                        "object": "chat.completion.chunk",
                        "created": int(time.time()),
                        "model": job["model_name"],
                        "choices": [
                            {"index": 0, "delta": {"content": response.text}}
                        ],
                    }
                    out_queue.put(
                        f"data: {json.dumps(chunk, ensure_ascii=False)}\n\n"
                    )

                usage_chunk = {
                    "id": job["completion_id"],
                    "object": "chat.completion.chunk",
                    "created": int(time.time()),
                    "model": job["model_name"],
                    "choices": [{"index": 0, "delta": {}}],
                    "usage": {
                        "prompt_tokens": prompt_tokens,
                        "completion_tokens": completion_tokens,
                    },
                }
                out_queue.put(f"data: {json.dumps(usage_chunk, ensure_ascii=False)}\n\n")
                out_queue.put("data: [DONE]\n\n")
            except Exception as exc:
                _log_json("generation_failed", error=str(exc))
            finally:
                out_queue.put(_STREAM_DONE)
            continue

        if kind == "collect":
            _, result_queue, job = cmd
            try:
                text_parts: list[str] = []
                prompt_tokens = 0
                completion_tokens = 0
                for response in stream_generate(
                    job["model"],
                    job["processor"],
                    job["prompt"],
                    image=job["image_arg"],
                    max_tokens=job["max_tokens"],
                    **job["gen_kwargs"],
                ):
                    text_parts.append(response.text)
                    prompt_tokens = int(response.prompt_tokens)
                    completion_tokens = int(response.generation_tokens)
                result_queue.put(
                    ("ok", "".join(text_parts), prompt_tokens, completion_tokens)
                )
            except Exception as exc:
                _log_json("generation_failed", error=str(exc))
                result_queue.put(("err", str(exc)))


def _enqueue_model_load(model_path: str) -> None:
    _ensure_mlx_thread()
    _MLX_CMD_QUEUE.put(("load", model_path))


class ChatMessage(BaseModel):
    role: str
    content: str | list[Any] = ""


class ChatCompletionRequest(BaseModel):
    model: str | None = None
    messages: list[ChatMessage]
    stream: bool = False
    max_tokens: int | None = None
    temperature: float | None = None
    top_p: float | None = None


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


def _require_model() -> tuple[Any, Any]:
    with _LOAD_LOCK:
        if _LOAD_ERROR is not None:
            raise HTTPException(
                status_code=503, detail=f"model load failed: {_LOAD_ERROR}"
            )
        if not _MODEL_LOADED or _MODEL is None or _PROCESSOR is None:
            raise HTTPException(status_code=503, detail="model not loaded")
        return _MODEL, _PROCESSOR


def _messages_to_dicts(messages: list[ChatMessage]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for message in messages:
        if isinstance(message.content, str):
            out.append({"role": message.role, "content": message.content})
        else:
            out.append(
                {
                    "role": message.role,
                    "content": _extract_text(message.content),
                }
            )
    return out


def _collect_images(messages: list[ChatMessage]) -> list[str]:
    images: list[str] = []
    for message in messages:
        if message.role == "user":
            images.extend(_extract_images(message.content))
    return images


def _generation_kwargs(body: ChatCompletionRequest) -> dict[str, Any]:
    kwargs: dict[str, Any] = {}
    if body.temperature is not None:
        kwargs["temperature"] = body.temperature
    if body.top_p is not None:
        kwargs["top_p"] = body.top_p
    return kwargs


def _generation_job(
    *,
    model: Any,
    processor: Any,
    prompt: str,
    image_arg: str | list[str] | None,
    max_tokens: int,
    gen_kwargs: dict[str, Any],
    completion_id: str,
    model_name: str,
) -> dict[str, Any]:
    return {
        "model": model,
        "processor": processor,
        "prompt": prompt,
        "image_arg": image_arg,
        "max_tokens": max_tokens,
        "gen_kwargs": gen_kwargs,
        "completion_id": completion_id,
        "model_name": model_name,
    }


def _collect_generation_result(job: dict[str, Any]) -> tuple[str, int, int]:
    """비스트림 생성을 MLX 워커 스레드에서 실행하고 결과를 회수한다."""
    result_queue: queue.Queue[Any] = queue.Queue()
    _ensure_mlx_thread()
    _MLX_CMD_QUEUE.put(("collect", result_queue, job))
    item = result_queue.get()
    if item[0] == "err":
        raise HTTPException(status_code=500, detail=f"generation failed: {item[1]}")
    return item[1], item[2], item[3]


@app.post("/v1/chat/completions")
async def chat_completions(body: ChatCompletionRequest) -> Any:
    model, processor = _require_model()
    messages = _messages_to_dicts(body.messages)
    images = _collect_images(body.messages)
    prompt = apply_chat_template(
        processor,
        model.config,
        messages,
        add_generation_prompt=True,
        num_images=len(images),
    )
    max_tokens = body.max_tokens if body.max_tokens is not None else 256
    gen_kwargs = _generation_kwargs(body)
    completion_id = f"chatcmpl-{uuid.uuid4().hex}"
    model_name = body.model or _SERVED_MODEL_NAME
    image_arg = images[0] if len(images) == 1 else (images if images else None)

    job = _generation_job(
        model=model,
        processor=processor,
        prompt=prompt,
        image_arg=image_arg,
        max_tokens=max_tokens,
        gen_kwargs=gen_kwargs,
        completion_id=completion_id,
        model_name=model_name,
    )

    if body.stream:

        def event_stream():
            out_queue: queue.Queue[Any] = queue.Queue()
            _ensure_mlx_thread()
            _MLX_CMD_QUEUE.put(("stream", out_queue, job))

            while True:
                item = out_queue.get()
                if item is _STREAM_DONE:
                    break
                yield item

        return StreamingResponse(event_stream(), media_type="text/event-stream")

    text, prompt_tokens, completion_tokens = _collect_generation_result(job)

    return JSONResponse(
        {
            "id": completion_id,
            "object": "chat.completion",
            "created": int(time.time()),
            "model": model_name,
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": text},
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "total_tokens": prompt_tokens + completion_tokens,
            },
        }
    )


def _handle_sigterm(_signum: int, _frame: Any) -> None:
    _log_json("shutdown", signal="SIGTERM")
    _SHUTDOWN.set()
    if _UVICORN_SERVER is not None:
        _UVICORN_SERVER.should_exit = True


def main() -> None:
    global _UVICORN_SERVER, _SERVED_MODEL_NAME, _MODEL, _PROCESSOR

    parser = argparse.ArgumentParser(description="aidash mlx-vlm adapter")
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

    _MLX_CMD_QUEUE.put(("shutdown",))

    with _LOAD_LOCK:
        _MODEL = None
        _PROCESSOR = None

    for path in _TEMP_IMAGE_FILES:
        try:
            Path(path).unlink(missing_ok=True)
        except Exception:
            pass

    _log_json("exit", code=0)
    sys.exit(0)


if __name__ == "__main__":
    main()