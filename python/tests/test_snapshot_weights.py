"""Regression test: _snapshot_has_weights must actually evaluate each glob.

any(Path.glob(...) for ext in exts) is a footgun — the outer any() sees unconsumed
generator objects (always truthy) instead of their contents, so it always returns
True even for an empty directory. Caught this live: an incomplete HF cache (config
only, no adapter_model.safetensors) was silently accepted as "cached", so the
adapter never fell back to downloading the missing weights.
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from adapters.serve_transformers import _snapshot_has_weights  # noqa: E402


class SnapshotHasWeightsTest(unittest.TestCase):
    def test_empty_directory_has_no_weights(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            self.assertFalse(_snapshot_has_weights(d))

    def test_config_only_directory_has_no_weights(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "adapter_config.json").write_text("{}")
            self.assertFalse(_snapshot_has_weights(d))

    def test_directory_with_safetensors_has_weights(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "adapter_config.json").write_text("{}")
            (Path(d) / "adapter_model.safetensors").write_bytes(b"\x00")
            self.assertTrue(_snapshot_has_weights(d))


if __name__ == "__main__":
    unittest.main()
