#!/usr/bin/env bash
set -euo pipefail

GODOT_BIN="${1:-${GODOT_BIN:-godot}}"
ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$ROOT_DIR/data"
TMP_DIR="$(mktemp -d)"
HOME_DIR="${HOME_DIR:-/tmp/godot_home}"
JSON_FILE="$TMP_DIR/extension_api.json"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

mkdir -p "$OUT_DIR" "$HOME_DIR"

(
  cd "$TMP_DIR"
  HOME="$HOME_DIR" "$GODOT_BIN" --headless --dump-extension-api-with-docs --quit >/dev/null
)

if [[ ! -f "$JSON_FILE" ]]; then
  echo "Missing generated file: $JSON_FILE" >&2
  exit 1
fi

jq -r '
def clean:
  tostring
  | gsub("[\\r\\n\\t]+"; " ")
  | gsub("  +"; " ")
  | gsub("^ +| +$"; "");
. as $root
| ["name","inherits","summary","note"] | @tsv,
  ($root.classes[]
    | [.name, (.inherits // ""), ((.brief_description // .description // "") | clean), ""] | @tsv),
  ($root.builtin_classes[]
    | [.name, "", ((.brief_description // .description // "") | clean), ""] | @tsv)
' "$JSON_FILE" > "$OUT_DIR/godot_4_6_class_meta.tsv"

jq -r '
def clean:
  tostring
  | gsub("[\\r\\n\\t]+"; " ")
  | gsub("  +"; " ")
  | gsub("^ +| +$"; "");
def arg_sig:
  .name + ": " + (.type // "Variant")
  + (if has("default_value") then " = " + (.default_value | tostring) else "" end);
def class_signature:
  .name + "("
  + (((.arguments // []) | map(arg_sig)) + (if .is_vararg then ["..."] else [] end) | join(", "))
  + ") -> "
  + ((.return_value.type // "void") | tostring);
def builtin_signature:
  .name + "("
  + (((.arguments // []) | map(arg_sig)) + (if .is_vararg then ["..."] else [] end) | join(", "))
  + ") -> "
  + ((.return_type // "void") | tostring);
. as $root
| ["name","class_name","signature","hover"] | @tsv,
  ($root.classes[] as $cls
    | ($cls.methods // [])[]
    | [
        .name,
        $cls.name,
        class_signature,
        ((.description // $cls.brief_description // ("Method on " + $cls.name + ".")) | clean)
      ] | @tsv),
  ($root.builtin_classes[] as $cls
    | ($cls.methods // [])[]
    | [
        .name,
        $cls.name,
        builtin_signature,
        ((.description // $cls.brief_description // ("Method on " + $cls.name + ".")) | clean)
      ] | @tsv)
' "$JSON_FILE" > "$OUT_DIR/godot_4_6_node_method_meta.tsv"

{
  echo -e "name\tinherits\tsummary\tnote"
  tail -n +2 "$OUT_DIR/godot_4_6_class_meta.tsv" | LC_ALL=C sort -u
} > "$OUT_DIR/godot_4_6_class_meta.tsv.tmp"
mv "$OUT_DIR/godot_4_6_class_meta.tsv.tmp" "$OUT_DIR/godot_4_6_class_meta.tsv"
{
  echo -e "name\tclass_name\tsignature\thover"
  tail -n +2 "$OUT_DIR/godot_4_6_node_method_meta.tsv" | LC_ALL=C sort -u
} > "$OUT_DIR/godot_4_6_node_method_meta.tsv.tmp"
mv "$OUT_DIR/godot_4_6_node_method_meta.tsv.tmp" "$OUT_DIR/godot_4_6_node_method_meta.tsv"

echo "Wrote:"
echo "  $OUT_DIR/godot_4_6_class_meta.tsv"
echo "  $OUT_DIR/godot_4_6_node_method_meta.tsv"
