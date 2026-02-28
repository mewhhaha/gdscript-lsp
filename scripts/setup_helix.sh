#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_DIR="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"

detect_binary_path() {
    if [[ $# -gt 0 && -n "$1" ]]; then
        printf '%s\n' "$1"
        return 0
    fi

    if command -v gdscript-lsp >/dev/null 2>&1; then
        command -v gdscript-lsp
        return 0
    fi
    if command -v gdscript_lsp >/dev/null 2>&1; then
        command -v gdscript_lsp
        return 0
    fi
    local cargo_bin_dash="${CARGO_HOME:-${HOME}/.cargo}/bin/gdscript-lsp"
    if [[ -x "${cargo_bin_dash}" ]]; then
        printf '%s\n' "${cargo_bin_dash}"
        return 0
    fi
    local cargo_bin_underscore="${CARGO_HOME:-${HOME}/.cargo}/bin/gdscript_lsp"
    if [[ -x "${cargo_bin_underscore}" ]]; then
        printf '%s\n' "${cargo_bin_underscore}"
        return 0
    fi
    if [[ -x "${REPO_DIR}/target/release/gdscript-lsp" ]]; then
        printf '%s\n' "${REPO_DIR}/target/release/gdscript-lsp"
        return 0
    fi
    if [[ -x "${REPO_DIR}/target/debug/gdscript-lsp" ]]; then
        printf '%s\n' "${REPO_DIR}/target/debug/gdscript-lsp"
        return 0
    fi
    if [[ -x "${REPO_DIR}/target/release/gdscript_lsp" ]]; then
        printf '%s\n' "${REPO_DIR}/target/release/gdscript_lsp"
        return 0
    fi
    if [[ -x "${REPO_DIR}/target/debug/gdscript_lsp" ]]; then
        printf '%s\n' "${REPO_DIR}/target/debug/gdscript_lsp"
        return 0
    fi

    return 1
}

BIN_PATH="$(detect_binary_path "${1:-}" || true)"
if [[ -z "${BIN_PATH}" ]]; then
    echo "Could not find gdscript-lsp binary."
    echo "Checked: PATH, \${CARGO_HOME:-\$HOME/.cargo}/bin, and ${REPO_DIR}/target/{release,debug}/gdscript-lsp."
    echo "Build it first with: cargo build --release"
    echo "Or pass an explicit binary path: scripts/setup_helix.sh /abs/path/to/gdscript-lsp"
    exit 1
fi

if [[ ! -x "${BIN_PATH}" ]]; then
    echo "Binary is not executable: ${BIN_PATH}"
    exit 1
fi

if command -v realpath >/dev/null 2>&1; then
    BIN_PATH="$(realpath "${BIN_PATH}")"
fi

HELIX_COMMAND="${BIN_PATH}"
if [[ "${BIN_PATH}" == "${CARGO_HOME:-${HOME}/.cargo}/bin/gdscript-lsp" ]]; then
    HELIX_COMMAND="gdscript-lsp"
elif [[ "${BIN_PATH}" == "${CARGO_HOME:-${HOME}/.cargo}/bin/gdscript_lsp" ]]; then
    HELIX_COMMAND="gdscript_lsp"
fi

HELIX_DIR="${HELIX_CONFIG_DIR:-${HOME}/.config/helix}"
LANG_FILE="${HELIX_DIR}/languages.toml"
TMP_FILE="$(mktemp)"
BLOCK_FILE="$(mktemp)"

cleanup() {
    rm -f "${TMP_FILE}" "${BLOCK_FILE}"
}
trap cleanup EXIT

mkdir -p "${HELIX_DIR}"
touch "${LANG_FILE}"

cat >"${BLOCK_FILE}" <<EOF
# >>> gdscript-lsp start >>>
[language-server.gdscript-lsp]
command = "${HELIX_COMMAND}"
args = ["lsp"]

[[language]]
name = "gdscript"
language-id = "gdscript"
file-types = ["gd", "gdscript"]
roots = ["project.godot", ".git"]
auto-format = true
language-servers = ["gdscript-lsp"]
# <<< gdscript-lsp end <<<
EOF

# Remove existing managed blocks (current + legacy) and legacy underscore blocks.
sed '/^# >>> gdscript_lsp start >>>$/,/^# <<< gdscript_lsp end <<<$/{d}' "${LANG_FILE}" \
    | sed '/^# >>> gdscript-lsp start >>>$/,/^# <<< gdscript-lsp end <<<$/{d}' \
    | sed '/^\[language-server.gdscript_lsp\]$/,/^# <<< gdscript_lsp end <<<$/{d}' \
    | awk '
        BEGIN { skipping_server = 0 }
        {
            if (skipping_server) {
                if ($0 ~ /^\[/) {
                    skipping_server = 0
                } else {
                    next
                }
            }

            if ($0 ~ /^\[language-server\.gdscript-lsp\]$/ || $0 ~ /^\[language-server\.gdscript_lsp\]$/) {
                skipping_server = 1
                next
            }

            print
        }
    ' \
    >"${TMP_FILE}"
mv "${TMP_FILE}" "${LANG_FILE}"

# Ensure there is a separating newline before appending.
if [[ -s "${LANG_FILE}" ]]; then
    printf '\n' >>"${LANG_FILE}"
fi
cat "${BLOCK_FILE}" >>"${LANG_FILE}"
printf '\n' >>"${LANG_FILE}"

echo "Updated Helix language config:"
echo "  ${LANG_FILE}"
echo
echo "Configured gdscript-lsp server binary:"
echo "  ${BIN_PATH}"
echo
echo "Helix command entry:"
echo "  ${HELIX_COMMAND}"
echo
echo "Next: restart Helix."
