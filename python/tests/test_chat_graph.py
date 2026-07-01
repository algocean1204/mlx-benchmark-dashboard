"""LangGraph 멀티턴 채팅 그래프 테스트 (모델 로드 없음 — summarize_fn을 mock)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from adapters.chat_graph import build_chat_graph, run_chat_graph  # noqa: E402


def _turns(n: int) -> list[dict]:
    out = []
    for i in range(n):
        role = "user" if i % 2 == 0 else "assistant"
        out.append({"role": role, "content": f"메시지 {i} " * 50})
    return out


class ChatGraphTest(unittest.TestCase):
    def test_short_history_passes_through_unchanged(self) -> None:
        calls = []
        graph = build_chat_graph(lambda old: calls.append(old) or "요약")
        messages = _turns(4)
        final, compressed = run_chat_graph(graph, messages, context_size=4096)
        self.assertEqual(final, messages)
        self.assertFalse(compressed)
        self.assertEqual(calls, [])

    def test_long_history_triggers_summary_and_keeps_recent_turns(self) -> None:
        graph = build_chat_graph(lambda old: "요약본")
        messages = _turns(20)
        final, compressed = run_chat_graph(graph, messages, context_size=512)
        self.assertTrue(compressed)
        # 요약 메시지 1개 + 최근 8개(keep_recent_turns=4 * 2) 유지
        self.assertEqual(len(final), 9)
        self.assertEqual(final[0]["role"], "assistant")
        self.assertIn("[이전 대화 요약]", final[0]["content"])
        self.assertIn("요약본", final[0]["content"])
        self.assertEqual(final[1:], messages[-8:])

    def test_summarize_fn_receives_older_messages_only(self) -> None:
        seen = {}

        def fake_summarize(old):
            seen["old"] = old
            return "요약"

        graph = build_chat_graph(fake_summarize)
        messages = _turns(20)
        run_chat_graph(graph, messages, context_size=512)
        self.assertEqual(seen["old"], messages[:-8])

    def test_zero_context_size_never_compresses(self) -> None:
        graph = build_chat_graph(lambda old: "요약")
        messages = _turns(20)
        final, compressed = run_chat_graph(graph, messages, context_size=0)
        self.assertFalse(compressed)
        self.assertEqual(final, messages)


if __name__ == "__main__":
    unittest.main()
