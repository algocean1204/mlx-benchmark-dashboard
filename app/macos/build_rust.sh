#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
WORKSPACE_DIR="$PROJECT_ROOT/core"

PROFILE="${CARGO_PROFILE:-release}"
export PATH="$HOME/.cargo/bin:$PATH"

cd "$WORKSPACE_DIR"
cargo build --profile "$PROFILE" -p aidash-frb

DYLIB="$WORKSPACE_DIR/target/$PROFILE/libaidash_frb.dylib"
if [[ ! -f "$DYLIB" ]]; then
  echo "error: Rust library not found at $DYLIB" >&2
  exit 1
fi

# Xcode build phase: copy into app bundle Frameworks
if [[ -n "${BUILT_PRODUCTS_DIR:-}" && -n "${FRAMEWORKS_FOLDER_PATH:-}" ]]; then
  DEST="$BUILT_PRODUCTS_DIR/$FRAMEWORKS_FOLDER_PATH/libaidash_frb.dylib"
  mkdir -p "$(dirname "$DEST")"
  cp -f "$DYLIB" "$DEST"
  echo "Copied $DYLIB -> $DEST"
fi

# Xcode build phase: bundle python adapters/tools for release installs
if [[ -n "${BUILT_PRODUCTS_DIR:-}" && -n "${UNLOCALIZED_RESOURCES_FOLDER_PATH:-}" ]]; then
  PYTHON_SRC="$PROJECT_ROOT/python"
  PYTHON_DEST="$BUILT_PRODUCTS_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/python"
  rm -rf "$PYTHON_DEST"
  mkdir -p "$PYTHON_DEST"
  rsync -a \
    "$PYTHON_SRC/adapters" \
    "$PYTHON_SRC/tools" \
    "$PYTHON_SRC/pyproject.toml" \
    "$PYTHON_SRC/uv.lock" \
    "$PYTHON_DEST/"
  echo "Copied python resources -> $PYTHON_DEST"
fi