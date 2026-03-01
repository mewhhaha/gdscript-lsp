set shell := ["bash", "-euo", "pipefail", "-c"]

# Install gdscript-lsp and update Helix to use it.
install:
    cargo install --path .
    scripts/setup_helix.sh

# Rebuild class/method hover metadata from the installed Godot binary docs dump.
sync-docs-meta GODOT_BIN="~/.local/bin/godot":
    scripts/sync_godot_docs_metadata.sh {{GODOT_BIN}}

# Validate docs metadata headers used by parser/hover.
verify-docs-meta:
    cargo test docs_meta -- --nocapture

# Local strict gate for formatter, tests, metadata validation, and parity drift.
ci-local:
    cargo fmt --all -- --check
    cargo check
    cargo test
    just verify-docs-meta
    cargo run -- parity-report --strict --limit 50
