"""Adapter CLI argument parsing tests (no model load)."""

from __future__ import annotations

import argparse
import unittest


def _mlx_lm_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="aidash mlx-lm adapter")
    parser.add_argument("--model-path", required=True)
    parser.add_argument("--context-size", type=int, required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--profile-json", required=True)
    parser.add_argument("--draft-model-path", default=None)
    return parser


def _mlx_vlm_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="aidash mlx-vlm adapter")
    parser.add_argument("--model-path", required=True)
    parser.add_argument("--context-size", type=int, required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--profile-json", required=True)
    parser.add_argument("--draft-model-path", default=None)
    return parser


def _transformers_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="aidash transformers adapter")
    parser.add_argument("--model-path", required=True)
    parser.add_argument("--context-size", type=int, required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--profile-json", required=True)
    parser.add_argument("--adapter-path", default=None)
    return parser


class AdapterArgsTest(unittest.TestCase):
    def test_mlx_lm_without_draft(self) -> None:
        args = _mlx_lm_parser().parse_args(
            [
                "--model-path",
                "org/model",
                "--context-size",
                "4096",
                "--port",
                "18080",
                "--profile-json",
                "{}",
            ]
        )
        self.assertIsNone(args.draft_model_path)

    def test_mlx_lm_with_draft(self) -> None:
        args = _mlx_lm_parser().parse_args(
            [
                "--model-path",
                "org/main",
                "--context-size",
                "4096",
                "--port",
                "18080",
                "--profile-json",
                "{}",
                "--draft-model-path",
                "org/assistant",
            ]
        )
        self.assertEqual(args.draft_model_path, "org/assistant")

    def test_mlx_vlm_with_draft(self) -> None:
        args = _mlx_vlm_parser().parse_args(
            [
                "--model-path",
                "org/main",
                "--context-size",
                "4096",
                "--port",
                "18081",
                "--profile-json",
                "{}",
                "--draft-model-path",
                "org/assistant",
            ]
        )
        self.assertEqual(args.draft_model_path, "org/assistant")

    def test_transformers_without_adapter(self) -> None:
        args = _transformers_parser().parse_args(
            [
                "--model-path",
                "kakaocorp/kanana-nano-2.1b-instruct",
                "--context-size",
                "4096",
                "--port",
                "18082",
                "--profile-json",
                "{}",
            ]
        )
        self.assertIsNone(args.adapter_path)

    def test_transformers_with_lora_adapter(self) -> None:
        args = _transformers_parser().parse_args(
            [
                "--model-path",
                "kakaocorp/kanana-nano-2.1b-instruct",
                "--context-size",
                "4096",
                "--port",
                "18082",
                "--profile-json",
                "{}",
                "--adapter-path",
                "test-org/test-lora-adapter",
            ]
        )
        self.assertEqual(args.adapter_path, "test-org/test-lora-adapter")


if __name__ == "__main__":
    unittest.main()