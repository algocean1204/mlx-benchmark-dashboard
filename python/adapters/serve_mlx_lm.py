"""mlx-lm backend adapter — LLM (fallback) — /v1/chat/completions (stream).

Uses mlx_lm.load() + mlx_lm.stream_generate() with tokenizer apply_chat_template.
Server starts before model load; /health and /metrics follow the shared adapter contract.
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
from typing import Any

import mlx.core as mx
import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.responses import JSONResponse, StreamingResponse
from pydantic import BaseModel

from mlx_lm import load, stream_generate

from adapters.chat_graph import build_chat_graph, build_summary_prompt, run_chat_graph

_UVICORN_SERVER: uvicorn.Server | None = None
_SHUTDOWN = threading.Event()
_CONTEXT_SIZE = 4096

_MODEL = None
_TOKENIZER = None
_DRAFT_MODEL = None
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


def _apply_chat_template(tokenizer: Any, messages: list[dict[str, Any]]) -> str:
    if hasattr(tokenizer, "apply_chat_template"):
        return tokenizer.apply_chat_template(
            messages, tokenize=False, add_generation_prompt=True
        )
    if hasattr(tokenizer, "tokenizer") and hasattr(
        tokenizer.tokenizer, "apply_chat_template"
    ):
        return tokenizer.tokenizer.apply_chat_template(
            messages, tokenize=False, add_generation_prompt=True
        )
    raise RuntimeError("tokenizer does not support apply_chat_template")


def _load_draft_model(draft_model_path: str | None) -> Any | None:
    if not draft_model_path:
        return None
    try:
        resolved = _resolve_model_path(draft_model_path)
        _log_json("draft_load_start", draft_model_path=draft_model_path)
        draft_model, _draft_tokenizer = load(resolved)
        _log_json("draft_loaded", draft_model_path=draft_model_path)
        return draft_model
    except Exception as exc:
        _log_json("draft_load_failed", draft_model_path=draft_model_path, error=str(exc))
        return None


def _background_model_load(model_path: str, draft_model_path: str | None = None) -> None:
    global _MODEL, _TOKENIZER, _DRAFT_MODEL, _LOAD_ERROR, _MODEL_LOADED

    try:
        _log_json("model_load_start", model_path=model_path)
        model, tokenizer = load(model_path)
        draft_model = _load_draft_model(draft_model_path)
        with _LOAD_LOCK:
            _MODEL = model
            _TOKENIZER = tokenizer
            _DRAFT_MODEL = draft_model
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
    return JSONResponse(_mlx_metrics_payload())


def _require_model() -> tuple[Any, Any, Any | None]:
    with _LOAD_LOCK:
        if _LOAD_ERROR is not None:
            raise HTTPException(status_code=503, detail=f"model load failed: {_LOAD_ERROR}")
        if not _MODEL_LOADED or _MODEL is None or _TOKENIZER is None:
            raise HTTPException(status_code=503, detail="model not loaded")
        return _MODEL, _TOKENIZER, _DRAFT_MODEL


def _generation_kwargs(body: ChatCompletionRequest) -> dict[str, Any]:
    kwargs: dict[str, Any] = {}
    if body.temperature is not None:
        kwargs["temp"] = body.temperature
    if body.top_p is not None:
        kwargs["top_p"] = body.top_p
    return kwargs


def _messages_to_dicts(messages: list[ChatMessage]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for message in messages:
        if isinstance(message.content, str):
            out.append({"role": message.role, "content": message.content})
        else:
            out.append({"role": message.role, "content": message.content})
    return out


def _summarize_history(model: Any, tokenizer: Any, old_messages: list[dict[str, Any]]) -> str:
    """LangGraph summarize 노드용 — 같은 로컬 모델로 동기(비스트리밍) 요약 생성."""
    prompt = _apply_chat_template(
        tokenizer, [{"role": "user", "content": build_summary_prompt(old_messages)}]
    )
    text_parts: list[str] = []
    for response in stream_generate(model, tokenizer, prompt, max_tokens=256, temp=0.0):
        text_parts.append(response.text)
    return "".join(text_parts).strip()


@app.post("/v1/chat/completions")
async def chat_completions(body: ChatCompletionRequest) -> Any:
    model, tokenizer, draft_model = _require_model()
    messages = _messages_to_dicts(body.messages)
    graph = build_chat_graph(lambda old: _summarize_history(model, tokenizer, old))
    messages, compressed = run_chat_graph(graph, messages, _CONTEXT_SIZE)
    if compressed:
        _log_json("chat_history_compressed", kept_messages=len(messages))
    prompt = _apply_chat_template(tokenizer, messages)
    max_tokens = body.max_tokens if body.max_tokens is not None else 256
    gen_kwargs = _generation_kwargs(body)
    completion_id = f"chatcmpl-{uuid.uuid4().hex}"
    model_name = body.model or _SERVED_MODEL_NAME

    if body.stream:

        def event_stream():
            prompt_tokens = 0
            completion_tokens = 0
            for response in stream_generate(
                model,
                tokenizer,
                prompt,
                max_tokens=max_tokens,
                draft_model=draft_model,
                **gen_kwargs,
            ):
                prompt_tokens = int(response.prompt_tokens)
                completion_tokens = int(response.generation_tokens)
                chunk = {
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": int(time.time()),
                    "model": model_name,
                    "choices": [{"index": 0, "delta": {"content": response.text}}],
                }
                yield f"data: {json.dumps(chunk, ensure_ascii=False)}\n\n"

            usage_chunk = {
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": model_name,
                "choices": [{"index": 0, "delta": {}}],
                "usage": {
                    "prompt_tokens": prompt_tokens,
                    "completion_tokens": completion_tokens,
                },
            }
            yield f"data: {json.dumps(usage_chunk, ensure_ascii=False)}\n\n"
            yield "data: [DONE]\n\n"

        return StreamingResponse(event_stream(), media_type="text/event-stream")

    text_parts: list[str] = []
    prompt_tokens = 0
    completion_tokens = 0
    for response in stream_generate(
        model,
        tokenizer,
        prompt,
        max_tokens=max_tokens,
        draft_model=draft_model,
        **gen_kwargs,
    ):
        text_parts.append(response.text)
        prompt_tokens = int(response.prompt_tokens)
        completion_tokens = int(response.generation_tokens)

    return JSONResponse(
        {
            "id": completion_id,
            "object": "chat.completion",
            "created": int(time.time()),
            "model": model_name,
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": "".join(text_parts)},
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
    global _UVICORN_SERVER, _SERVED_MODEL_NAME, _MODEL, _TOKENIZER, _CONTEXT_SIZE

    parser = argparse.ArgumentParser(description="aidash mlx-lm adapter")
    parser.add_argument("--model-path", required=True)
    parser.add_argument("--context-size", type=int, required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--profile-json", required=True)
    parser.add_argument("--draft-model-path", default=None)
    args = parser.parse_args()

    _configure_logging()

    try:
        profile = json.loads(args.profile_json)
    except json.JSONDecodeError as exc:
        _log_json("startup_failed", error=f"invalid profile-json: {exc}")
        sys.exit(1)

    signal.signal(signal.SIGTERM, _handle_sigterm)
    _SERVED_MODEL_NAME = args.model_path
    _CONTEXT_SIZE = args.context_size

    try:
        resolved_model_path = _resolve_model_path(args.model_path)
    except Exception as exc:
        _log_json("startup_failed", error=str(exc))
        sys.exit(1)

    load_thread = threading.Thread(
        target=_background_model_load,
        args=(resolved_model_path, args.draft_model_path),
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
        _TOKENIZER = None
        _DRAFT_MODEL = None

    _log_json("exit", code=0)
    sys.exit(0)


if __name__ == "__main__":
    main()