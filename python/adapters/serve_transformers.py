"""transformers backend adapter — 일반 PyTorch/safetensors 모델 — /v1/chat/completions.

Apple Silicon에서 MPS(Metal Performance Shaders)를 자동 감지해 사용하고, 없으면 CPU로
폴백한다. LoRA 어댑터(`--adapter-path`)가 주어지면 베이스 모델에 병합해 서빙한다.
멀티턴 히스토리는 chat_graph(LangGraph)를 거쳐 필요 시 자동 압축된다.
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

import torch
import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.responses import JSONResponse, StreamingResponse
from pydantic import BaseModel

from adapters.chat_graph import build_chat_graph, build_summary_prompt, run_chat_graph

_UVICORN_SERVER: uvicorn.Server | None = None
_SHUTDOWN = threading.Event()

_MODEL = None
_TOKENIZER = None
_DEVICE: "torch.device | None" = None
_LOAD_ERROR: str | None = None
_MODEL_LOADED = False
_LOAD_LOCK = threading.Lock()
_SERVED_MODEL_NAME = ""
_CONTEXT_SIZE = 4096


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


def _apply_chat_template(tokenizer: Any, messages: list[dict[str, Any]]) -> str:
    if hasattr(tokenizer, "apply_chat_template"):
        return tokenizer.apply_chat_template(
            messages, tokenize=False, add_generation_prompt=True
        )
    raise RuntimeError("tokenizer does not support apply_chat_template")


def _select_device() -> "torch.device":
    if torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def _patch_llama_head_dim_validation() -> None:
    # ponytail: transformers>=5's LlamaConfig.validate_architecture() unconditionally requires
    # hidden_size % num_attention_heads == 0, ignoring an explicit head_dim override (valid HF
    # config pattern, e.g. kanana-nano: hidden_size=1792, heads=24, head_dim=128 explicit).
    # __class_validators__ caches the function at @strict decoration time, so a plain attribute
    # reassignment doesn't take effect — patch the list entry too. Drop once upstream respects
    # explicit head_dim (https://github.com/huggingface/transformers issue for validate_architecture).
    try:
        from transformers.models.llama.configuration_llama import LlamaConfig

        original = LlamaConfig.validate_architecture

        def patched(self: Any) -> None:
            if (
                getattr(self, "head_dim", None)
                and self.head_dim * self.num_attention_heads != self.hidden_size
            ):
                return
            original(self)

        LlamaConfig.validate_architecture = patched
        validators = getattr(LlamaConfig, "__class_validators__", None)
        if validators is not None:
            for i, v in enumerate(validators):
                if getattr(v, "__name__", "") == "validate_architecture":
                    validators[i] = patched
    except Exception:
        pass


def _background_model_load(model_path: str, adapter_path: str | None = None) -> None:
    global _MODEL, _TOKENIZER, _DEVICE, _LOAD_ERROR, _MODEL_LOADED

    try:
        from transformers import AutoModelForCausalLM, AutoTokenizer

        _patch_llama_head_dim_validation()

        device = _select_device()
        dtype = torch.float16 if device.type == "mps" else torch.float32
        _log_json("model_load_start", model_path=model_path, device=device.type)

        tokenizer = AutoTokenizer.from_pretrained(model_path, local_files_only=True)
        model = AutoModelForCausalLM.from_pretrained(
            model_path,
            dtype=dtype,
            local_files_only=True,
        )

        if adapter_path:
            from peft import PeftModel

            resolved_adapter = _resolve_model_path(adapter_path)
            _log_json("adapter_load_start", adapter_path=adapter_path)
            model = PeftModel.from_pretrained(model, resolved_adapter)
            model = model.merge_and_unload()
            _log_json("adapter_loaded", adapter_path=adapter_path)

        model = model.to(device)
        model.eval()
        with _LOAD_LOCK:
            _MODEL = model
            _TOKENIZER = tokenizer
            _DEVICE = device
            _LOAD_ERROR = None
            _MODEL_LOADED = True
        _log_json("model_loaded", model_path=model_path, device=device.type)
    except Exception as exc:
        with _LOAD_LOCK:
            _LOAD_ERROR = str(exc)
            _MODEL_LOADED = False
        _log_json("model_load_failed", error=str(exc))


def _generate_once(model: Any, tokenizer: Any, prompt_text: str, max_new_tokens: int) -> str:
    """요약 등 내부 용도의 동기(비스트리밍) 생성 — LangGraph summarize 노드에서 사용."""
    input_ids = tokenizer(prompt_text, return_tensors="pt").input_ids.to(model.device)
    output_ids = model.generate(
        input_ids, max_new_tokens=max_new_tokens, do_sample=False
    )
    generated = output_ids[0, input_ids.shape[-1] :]
    return tokenizer.decode(generated, skip_special_tokens=True).strip()


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


def _require_model() -> tuple[Any, Any]:
    with _LOAD_LOCK:
        if _LOAD_ERROR is not None:
            raise HTTPException(
                status_code=503, detail=f"model load failed: {_LOAD_ERROR}"
            )
        if not _MODEL_LOADED or _MODEL is None or _TOKENIZER is None:
            raise HTTPException(status_code=503, detail="model not loaded")
        return _MODEL, _TOKENIZER


def _messages_to_dicts(messages: list[ChatMessage]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for message in messages:
        if isinstance(message.content, str):
            out.append({"role": message.role, "content": message.content})
        else:
            out.append({"role": message.role, "content": message.content})
    return out


def _generation_kwargs(body: ChatCompletionRequest) -> dict[str, Any]:
    kwargs: dict[str, Any] = {"do_sample": body.temperature is not None}
    if body.max_tokens is not None:
        kwargs["max_new_tokens"] = body.max_tokens
    if body.temperature is not None:
        kwargs["temperature"] = body.temperature
    if body.top_p is not None:
        kwargs["top_p"] = body.top_p
    return kwargs


def _stream_chat(model: Any, tokenizer: Any, prompt: str, kwargs: dict[str, Any]):
    from threading import Thread

    from transformers import TextIteratorStreamer

    streamer = TextIteratorStreamer(tokenizer, skip_special_tokens=True)
    gen_kwargs = {
        **kwargs,
        "input_ids": tokenizer(prompt, return_tensors="pt").input_ids.to(model.device),
        "streamer": streamer,
    }
    thread = Thread(target=model.generate, kwargs=gen_kwargs, daemon=True)
    thread.start()

    prompt_tokens = int(gen_kwargs["input_ids"].shape[-1])
    completion_tokens = 0
    for text in streamer:
        if text:
            completion_tokens += 1
            yield text
    thread.join()
    yield {"__usage__": {"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens}}


def _summarize_history(model: Any, tokenizer: Any, old_messages: list[dict[str, Any]]) -> str:
    summary_prompt = _apply_chat_template(
        tokenizer, [{"role": "user", "content": build_summary_prompt(old_messages)}]
    )
    return _generate_once(model, tokenizer, summary_prompt, max_new_tokens=256)


@app.post("/v1/chat/completions")
async def chat_completions(body: ChatCompletionRequest) -> Any:
    model, tokenizer = _require_model()
    messages = _messages_to_dicts(body.messages)
    graph = build_chat_graph(lambda old: _summarize_history(model, tokenizer, old))
    messages, compressed = run_chat_graph(graph, messages, _CONTEXT_SIZE)
    if compressed:
        _log_json("chat_history_compressed", kept_messages=len(messages))
    prompt = _apply_chat_template(tokenizer, messages)
    max_tokens = body.max_tokens if body.max_tokens is not None else 256
    gen_kwargs = _generation_kwargs(body)
    gen_kwargs.setdefault("max_new_tokens", max_tokens)
    completion_id = f"chatcmpl-{uuid.uuid4().hex}"
    model_name = body.model or _SERVED_MODEL_NAME

    if body.stream:

        def event_stream():
            prompt_tokens = 0
            completion_tokens = 0
            for chunk in _stream_chat(model, tokenizer, prompt, gen_kwargs):
                if isinstance(chunk, dict) and "__usage__" in chunk:
                    usage = chunk["__usage__"]
                    prompt_tokens = int(usage["prompt_tokens"])
                    completion_tokens = int(usage["completion_tokens"])
                    continue
                sse_chunk = {
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": int(time.time()),
                    "model": model_name,
                    "choices": [{"index": 0, "delta": {"content": chunk}}],
                }
                yield f"data: {json.dumps(sse_chunk, ensure_ascii=False)}\n\n"

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

    input_ids = tokenizer(prompt, return_tensors="pt").input_ids.to(model.device)
    output_ids = model.generate(input_ids, **gen_kwargs)
    generated = output_ids[0, input_ids.shape[-1] :]
    text = tokenizer.decode(generated, skip_special_tokens=True)
    prompt_tokens = int(input_ids.shape[-1])
    completion_tokens = int(generated.shape[-1])

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
    global _UVICORN_SERVER, _SERVED_MODEL_NAME, _MODEL, _TOKENIZER, _DEVICE, _CONTEXT_SIZE

    parser = argparse.ArgumentParser(description="aidash transformers adapter")
    parser.add_argument("--model-path", required=True)
    parser.add_argument("--context-size", type=int, required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--profile-json", required=True)
    parser.add_argument("--adapter-path", default=None)
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
        args=(resolved_model_path, args.adapter_path),
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
        _DEVICE = None

    _log_json("exit", code=0)
    sys.exit(0)


if __name__ == "__main__":
    main()