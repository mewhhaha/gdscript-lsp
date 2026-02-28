#!/usr/bin/env bash
set -euo pipefail

UPSTREAM_ROOT="${1:-/tmp/godot-upstream}"
GD_WARN_H="$UPSTREAM_ROOT/modules/gdscript/gdscript_warning.h"
GD_UTIL_CPP="$UPSTREAM_ROOT/modules/gdscript/gdscript_utility_functions.cpp"
OUT_DIR="$(cd "$(dirname "$0")/.." && pwd)/data"

if [[ ! -f "$GD_WARN_H" ]]; then
  echo "Missing file: $GD_WARN_H" >&2
  exit 1
fi

if [[ ! -f "$GD_UTIL_CPP" ]]; then
  echo "Missing file: $GD_UTIL_CPP" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

# Extract enum warning codes from GDScriptWarning::Code.
sed -n '/enum Code {/,/WARNING_MAX/p' "$GD_WARN_H" \
  | sed -nE 's/^[[:space:]]*([A-Z0-9_]+),.*/\1/p' \
  | sed -E '/^WARNING_MAX$/d' \
  | tr 'A-Z' 'a-z' \
  > "$OUT_DIR/godot_4_6_warning_codes.txt"

# Extract utility function registration names (strip leading underscore helper names like _char).
rg -o 'REGISTER_FUNC\([[:space:]]*[A-Za-z0-9_]+' "$GD_UTIL_CPP" \
  | sed -E 's/REGISTER_FUNC\([[:space:]]*//' \
  | sed -E 's/^_//' \
  | tr 'A-Z' 'a-z' \
  | sed -E '/^m_func$/d' \
  | sort -u \
  > "$OUT_DIR/godot_4_6_utility_functions.txt"

# Keep deterministic order and no trailing spaces.
LC_ALL=C sort -u -o "$OUT_DIR/godot_4_6_warning_codes.txt" "$OUT_DIR/godot_4_6_warning_codes.txt"
LC_ALL=C sort -u -o "$OUT_DIR/godot_4_6_utility_functions.txt" "$OUT_DIR/godot_4_6_utility_functions.txt"

echo "Wrote:"
echo "  $OUT_DIR/godot_4_6_warning_codes.txt"
echo "  $OUT_DIR/godot_4_6_utility_functions.txt"
