"""llama.cpp (llama-cpp-python) backend adapter — CPU mode — /v1/chat/completions.

Uses llama_cpp.Llama with n_gpu_layers=0. GGUF single-file models resolve via
profile source.hf_file. Server starts before model load; /metrics returns zeros.
"""

from __future__ import annotations

import argparse
import json
import logging
import signal
import sys
import threading
import time
import uuid
from pathlib import Path
from typing import Any

import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.responses import JSONResponse, StreamingResponse
from pydantic import BaseModel

_UVICORN_SERVER: uvicorn.Server | None = None
_SHUTDOWN = threading.Event()

_LLAMA = None
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


def _zero_metrics_payload() -> dict[str, int]:
    return {
        "mlx_active_bytes": 0,
        "mlx_peak_bytes": 0,
        "mlx_cache_bytes": 0,
    }


def _resolve_gguf_path(model_path: str, profile: dict[str, Any]) -> str:
    local = Path(model_path).expanduser()
    if local.is_file() and local.suffix.lower() == ".gguf":
        _log_json("model_resolve_local_file", path=str(local))
        return str(local)

    source = profile.get("source", {})
    hf_file = source.get("hf_file") or ""
    repo_id = source.get("hf_repo") or model_path

    if hf_file:
        from huggingface_hub import hf_hub_download

        try:
            resolved = hf_hub_download(
                repo_id=repo_id, filename=hf_file, local_files_only=True
            )
            _log_json(
                "model_resolve_cache",
                repo_id=repo_id,
                hf_file=hf_file,
                path=resolved,
            )
            return resolved
        except Exception as exc:
            _log_json(
                "model_resolve_online_fallback",
                repo_id=repo_id,
                hf_file=hf_file,
                reason=str(exc),
            )
            resolved = hf_hub_download(
                repo_id=repo_id, filename=hf_file, local_files_only=False
            )
            _log_json(
                "model_resolve_online",
                repo_id=repo_id,
                hf_file=hf_file,
                path=resolved,
            )
            return resolved

    if local.is_file():
        _log_json("model_resolve_local_file", path=str(local))
        return str(local)

    raise RuntimeError(
        "llama_cpp requires a GGUF file path or profile.source.hf_file"
    )


def _background_model_load(model_path: str, context_size: int) -> None:
    global _LLAMA, _LOAD_ERROR, _MODEL_LOADED

    try:
        from llama_cpp import Llama

        _log_json("model_load_start", model_path=model_path)
        llama = Llama(
            model_path=model_path,
            n_ctx=context_size,
            n_gpu_layers=0,
            verbose=False,
        )
        with _LOAD_LOCK:
            _LLAMA = llama
            _LOAD_ERROR = None
            _MODEL_LOADED = True
        _log_json("model_loaded", model_path=model_path)
    except Exception as exc:
        with _LOAD_LOCK:
            _LOAD_ERROR = str(exc)
            _MODEL_LOADED = False
        _log_json("model_load_failed", error=str(exc))


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
    return JSONResponse(_zero_metrics_payload())


def _require_llama() -> Any:
    with _LOAD_LOCK:
        if _LOAD_ERROR is not None:
            raise HTTPException(
                status_code=503, detail=f"model load failed: {_LOAD_ERROR}"
            )
        if not _MODEL_LOADED or _LLAMA is None:
            raise HTTPException(status_code=503, detail="model not loaded")
        return _LLAMA


def _messages_to_dicts(messages: list[ChatMessage]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for message in messages:
        if isinstance(message.content, str):
            out.append({"role": message.role, "content": message.content})
        else:
            out.append({"role": message.role, "content": message.content})
    return out


def _completion_kwargs(body: ChatCompletionRequest) -> dict[str, Any]:
    kwargs: dict[str, Any] = {"stream": body.stream}
    if body.max_tokens is not None:
        kwargs["max_tokens"] = body.max_tokens
    if body.temperature is not None:
        kwargs["temperature"] = body.temperature
    if body.top_p is not None:
        kwargs["top_p"] = body.top_p
    kwargs["model"] = body.model or _SERVED_MODEL_NAME
    return kwargs


def _chunk_to_sse(chunk: Any) -> str:
    if hasattr(chunk, "model_dump"):
        payload = chunk.model_dump()
    elif isinstance(chunk, dict):
        payload = chunk
    else:
        payload = dict(chunk)
    return f"data: {json.dumps(payload, ensure_ascii=False)}\n\n"


def _stream_chat(llama: Any, messages: list[dict[str, Any]], kwargs: dict[str, Any]):
    kwargs = {**kwargs, "stream": True}
    stream = llama.create_chat_completion(messages=messages, **kwargs)

    prompt_tokens: int | None = None
    completion_tokens: int | None = None
    last_chunk: dict[str, Any] | None = None
    tokens_before = int(llama.n_tokens)
    completion_text = ""

    for chunk in stream:
        if hasattr(chunk, "model_dump"):
            chunk_dict = chunk.model_dump()
        elif isinstance(chunk, dict):
            chunk_dict = chunk
        else:
            chunk_dict = dict(chunk)

        usage = chunk_dict.get("usage")
        if usage:
            prompt_tokens = int(usage.get("prompt_tokens", prompt_tokens or 0))
            completion_tokens = int(
                usage.get("completion_tokens", completion_tokens or 0)
            )

        for choice in chunk_dict.get("choices", []):
            content = (choice.get("delta", {}) or {}).get("content")
            if content:
                completion_text += content

        last_chunk = chunk_dict
        yield _chunk_to_sse(chunk_dict)

    if prompt_tokens is None or completion_tokens is None:
        if completion_text:
            completion_tokens = len(llama.tokenize(completion_text.encode("utf-8")))
        else:
            completion_tokens = max(0, int(llama.n_tokens) - tokens_before)
        total_new = max(0, int(llama.n_tokens) - tokens_before)
        if completion_tokens > total_new:
            completion_tokens = total_new
        prompt_tokens = max(1, total_new - completion_tokens)

    usage_chunk = {
        "id": (last_chunk or {}).get("id", f"chatcmpl-{uuid.uuid4().hex}"),
        "object": "chat.completion.chunk",
        "created": int(time.time()),
        "model": kwargs.get("model", _SERVED_MODEL_NAME),
        "choices": [{"index": 0, "delta": {}}],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
        },
    }
    yield f"data: {json.dumps(usage_chunk, ensure_ascii=False)}\n\n"
    yield "data: [DONE]\n\n"


@app.post("/v1/chat/completions")
async def chat_completions(body: ChatCompletionRequest) -> Any:
    llama = _require_llama()
    messages = _messages_to_dicts(body.messages)
    kwargs = _completion_kwargs(body)

    if body.stream:
        return StreamingResponse(
            _stream_chat(llama, messages, kwargs),
            media_type="text/event-stream",
        )

    kwargs["stream"] = False
    result = llama.create_chat_completion(messages=messages, **kwargs)
    if hasattr(result, "model_dump"):
        return JSONResponse(result.model_dump())
    return JSONResponse(result)


def _handle_sigterm(_signum: int, _frame: Any) -> None:
    _log_json("shutdown", signal="SIGTERM")
    _SHUTDOWN.set()
    if _UVICORN_SERVER is not None:
        _UVICORN_SERVER.should_exit = True


def main() -> None:
    global _UVICORN_SERVER, _SERVED_MODEL_NAME, _LLAMA

    parser = argparse.ArgumentParser(description="aidash llama-cpp adapter")
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
        resolved_model_path = _resolve_gguf_path(args.model_path, profile)
    except Exception as exc:
        _log_json("startup_failed", error=str(exc))
        sys.exit(1)

    load_thread = threading.Thread(
        target=_background_model_load,
        args=(resolved_model_path, args.context_size),
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
        if _LLAMA is not None:
            try:
                _LLAMA.close()
            except Exception:
                pass
        _LLAMA = None

    _log_json("exit", code=0)
    sys.exit(0)


if __name__ == "__main__":
    main()