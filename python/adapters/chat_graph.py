"""LangGraph 기반 멀티턴 채팅 히스토리 오케스트레이션.

역할은 오직 "다음 생성 호출에 보낼 최종 messages를 결정하는 것"뿐이다 —
실제 스트리밍 생성은 각 어댑터가 기존 방식 그대로 수행한다(SSE 포맷 불변).

그래프: check_context → (필요시) summarize → END
Rust chat.rs의 압축 정책(COMPRESS_KEEP_RECENT_TURNS=4, COMPRESS_THRESHOLD_RATIO=0.7)과
동일한 상수를 사용해, 클라이언트(Flutter)가 이미 압축했든 안 했든 서버 측에서도
같은 기준으로 안전망을 제공한다.
"""

from __future__ import annotations

from typing import Any, Callable, TypedDict

from langgraph.graph import END, StateGraph

KEEP_RECENT_TURNS = 4
COMPRESS_THRESHOLD_RATIO = 0.7

SummarizeFn = Callable[[list[dict[str, Any]]], str]


class ChatState(TypedDict):
    messages: list[dict[str, Any]]
    context_size: int
    compressed: bool


def _estimate_tokens(messages: list[dict[str, Any]]) -> int:
    """문자수 기반 대략적 토큰수 추정 (한국어/영어 혼용 근사치)."""
    total_chars = sum(len(str(m.get("content", ""))) for m in messages)
    return max(0, total_chars // 2)


def _needs_compression(messages: list[dict[str, Any]], context_size: int) -> bool:
    if context_size <= 0:
        return False
    keep_messages = KEEP_RECENT_TURNS * 2
    if len(messages) <= keep_messages:
        return False
    return _estimate_tokens(messages) > context_size * COMPRESS_THRESHOLD_RATIO


def _split_for_compression(
    messages: list[dict[str, Any]],
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    keep_messages = KEEP_RECENT_TURNS * 2
    if len(messages) <= keep_messages:
        return [], messages
    split_at = len(messages) - keep_messages
    return messages[:split_at], messages[split_at:]


def build_summary_prompt(old_messages: list[dict[str, Any]]) -> str:
    role_labels = {"user": "사용자", "assistant": "어시스턴트", "system": "시스템"}
    lines = [
        "다음 대화를 핵심 사실·결정·맥락 중심으로 한국어로 요약하세요. "
        "불필요한 인사말은 생략하고, 이후 대화에서 참고할 수 있게 간결하게 정리하세요.\n",
    ]
    for msg in old_messages:
        role = role_labels.get(msg.get("role", ""), msg.get("role", ""))
        lines.append(f"{role}: {msg.get('content', '')}")
    return "\n".join(lines)


def build_chat_graph(summarize_fn: SummarizeFn):
    """summarize_fn(old_messages) -> 요약 문자열. 로컬 모델을 동기 호출해 생성한다."""

    def check_context(state: ChatState) -> ChatState:
        compress = _needs_compression(state["messages"], state["context_size"])
        return {**state, "compressed": compress}

    def summarize(state: ChatState) -> ChatState:
        old, recent = _split_for_compression(state["messages"])
        if not old:
            return {**state, "compressed": False}
        summary = summarize_fn(old)
        summary_message = {
            "role": "assistant",
            "content": f"[이전 대화 요약]\n{summary}",
        }
        return {**state, "messages": [summary_message, *recent], "compressed": True}

    def route(state: ChatState) -> str:
        return "summarize" if state["compressed"] else END

    graph = StateGraph(ChatState)
    graph.add_node("check_context", check_context)
    graph.add_node("summarize", summarize)
    graph.set_entry_point("check_context")
    graph.add_conditional_edges("check_context", route, {"summarize": "summarize", END: END})
    graph.add_edge("summarize", END)
    return graph.compile()


def run_chat_graph(
    compiled_graph: Any, messages: list[dict[str, Any]], context_size: int
) -> tuple[list[dict[str, Any]], bool]:
    result = compiled_graph.invoke(
        {"messages": messages, "context_size": context_size, "compressed": False}
    )
    return result["messages"], result["compressed"]
